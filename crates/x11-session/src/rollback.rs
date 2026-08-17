use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;
use crate::{Checkpoint, Session};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPoint {
    pub checkpoint_id: Uuid,
    pub session_id: Uuid,
    pub note: String,
    pub event_count: usize,
    pub workspace: PathBuf,
}

impl RollbackPoint {
    pub fn from_session(session: &Session, checkpoint: &Checkpoint, workspace: impl Into<PathBuf>) -> Self {
        Self { checkpoint_id: checkpoint.id, session_id: session.id, note: checkpoint.note.clone(), event_count: checkpoint.event_count, workspace: workspace.into() }
    }

    pub async fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).await?; }
        let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        fs::write(&tmp, serde_json::to_vec_pretty(self)?).await?;
        fs::rename(tmp, path).await?;
        Ok(())
    }

    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = fs::read(path).await.context("read rollback point")?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}
