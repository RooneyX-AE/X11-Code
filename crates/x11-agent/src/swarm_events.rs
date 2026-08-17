use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SwarmEventKind {
    SwarmStarted,
    TaskQueued,
    TaskStarted,
    TaskBlocked,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
    ConflictDetected,
    ResolverStarted,
    ResolverProposed,
    ResolverApplied,
    ResolverRolledBack,
    VerificationStarted,
    VerificationPassed,
    VerificationFailed,
    SwarmResumed,
    SwarmCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmEvent {
    pub event_id: Uuid,
    pub swarm_id: Uuid,
    pub task_id: Option<String>,
    pub agent_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub timestamp_ms: u64,
    pub kind: SwarmEventKind,
    pub progress: Option<u8>,
    pub state: Option<String>,
    pub evidence: Vec<String>,
}

impl SwarmEvent {
    pub fn new(swarm_id: Uuid, kind: SwarmEventKind) -> Self {
        let timestamp_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        Self { event_id: Uuid::new_v4(), swarm_id, task_id: None, agent_id: None, parent_task_id: None, timestamp_ms, kind, progress: None, state: None, evidence: Vec::new() }
    }
    pub fn task(mut self, task_id: impl Into<String>) -> Self { self.task_id = Some(task_id.into()); self }
    pub fn agent(mut self, agent_id: impl Into<String>) -> Self { self.agent_id = Some(agent_id.into()); self }
    pub fn parent_task(mut self, task_id: impl Into<String>) -> Self { self.parent_task_id = Some(task_id.into()); self }
    pub fn progress(mut self, value: u8) -> Self { self.progress = Some(value.min(100)); self }
    pub fn state(mut self, value: impl Into<String>) -> Self { self.state = Some(value.into()); self }
    pub fn evidence(mut self, value: impl Into<String>) -> Self { self.evidence.push(value.into()); self }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builder_preserves_identity_and_bounds_progress() {
        let swarm = Uuid::new_v4();
        let event = SwarmEvent::new(swarm, SwarmEventKind::TaskStarted)
            .task("task-1").agent("agent-1").parent_task("root").progress(250).state("running").evidence("started");
        assert_eq!(event.swarm_id, swarm);
        assert_eq!(event.progress, Some(100));
        assert_eq!(event.task_id.as_deref(), Some("task-1"));
        assert_eq!(event.agent_id.as_deref(), Some("agent-1"));
    }
}
