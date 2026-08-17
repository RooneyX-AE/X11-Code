use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use uuid::Uuid;

fn now_rfc3339_like() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    secs.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmTaskStatus { Pending, Running, Succeeded, Failed, Cancelled }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTaskState {
    pub task_id: String,
    pub status: SwarmTaskStatus,
    #[serde(default)] pub session_id: Option<Uuid>,
    #[serde(default)] pub output: Option<String>,
    #[serde(default)] pub error: Option<String>,
    #[serde(default)] pub files_changed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmState {
    pub schema_version: u32,
    pub swarm_id: Uuid,
    pub goal: String,
    pub updated_at: String,
    pub tasks: BTreeMap<String, SwarmTaskState>,
}

impl SwarmState {
    pub fn new(goal: impl Into<String>, tasks: impl IntoIterator<Item = String>) -> Self {
        let mut map = BTreeMap::new();
        for id in tasks {
            map.insert(id.clone(), SwarmTaskState { task_id: id, status: SwarmTaskStatus::Pending, session_id: None, output: None, error: None, files_changed: Vec::new() });
        }
        Self { schema_version: 1, swarm_id: Uuid::new_v4(), goal: goal.into(), updated_at: now_rfc3339_like(), tasks: map }
    }

    pub fn mark_running(&mut self, task_id: &str, session_id: Uuid) -> Result<()> {
        let task = self.tasks.get_mut(task_id).context("unknown swarm task")?;
        task.status = SwarmTaskStatus::Running;
        task.session_id = Some(session_id);
        self.touch();
        Ok(())
    }

    pub fn mark_finished(&mut self, task_id: &str, result: ResultSnapshot) -> Result<()> {
        let task = self.tasks.get_mut(task_id).context("unknown swarm task")?;
        task.status = if result.cancelled { SwarmTaskStatus::Cancelled } else if result.success { SwarmTaskStatus::Succeeded } else { SwarmTaskStatus::Failed };
        task.output = result.output;
        task.error = result.error;
        task.files_changed = result.files_changed;
        self.touch();
        Ok(())
    }

    pub fn resumable_tasks(&self) -> Vec<String> {
        self.tasks.values().filter(|t| matches!(t.status, SwarmTaskStatus::Pending | SwarmTaskStatus::Running)).map(|t| t.task_id.clone()).collect()
    }

    pub async fn save_atomic(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.touch();
        let path = path.as_ref();
        if let Some(parent) = path.parent() { tokio::fs::create_dir_all(parent).await?; }
        let bytes = serde_json::to_vec_pretty(self)?;
        let tmp = PathBuf::from(format!("{}.tmp-{}", path.display(), Uuid::new_v4()));
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, path).await.context("atomic swarm state rename")?;
        Ok(())
    }

    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let state: Self = serde_json::from_slice(&tokio::fs::read(path.as_ref()).await?).context("invalid swarm state")?;
        if state.schema_version != 1 { anyhow::bail!("unsupported swarm state schema {}", state.schema_version); }
        Ok(state)
    }

    fn touch(&mut self) { self.updated_at = now_rfc3339_like(); }
}

#[derive(Debug, Clone)]
pub struct ResultSnapshot {
    pub success: bool,
    pub cancelled: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub files_changed: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn state_round_trips_and_marks_resumable_tasks() {
        let dir = std::env::temp_dir().join(format!("x11-swarm-state-{}", Uuid::new_v4()));
        let path = dir.join("swarm.json");
        let mut state = SwarmState::new("fix repo", vec!["a".into(), "b".into()]);
        let sid = Uuid::new_v4();
        state.mark_running("a", sid).unwrap();
        state.mark_finished("a", ResultSnapshot { success: true, cancelled: false, output: Some("done".into()), error: None, files_changed: vec!["src/lib.rs".into()] }).unwrap();
        state.save_atomic(&path).await.unwrap();
        let loaded = SwarmState::load(&path).await.unwrap();
        assert_eq!(loaded.tasks["a"].status, SwarmTaskStatus::Succeeded);
        assert_eq!(loaded.resumable_tasks(), vec!["b"]);
        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[test]
    fn rejects_unknown_tasks() {
        let mut state = SwarmState::new("goal", vec!["known".into()]);
        assert!(state.mark_running("missing", Uuid::new_v4()).is_err());
    }
}
