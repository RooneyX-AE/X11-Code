use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::swarm_events::{SwarmEvent, SwarmEventKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SwarmView {
    pub swarm_id: Option<Uuid>,
    pub completed: u32,
    pub failed: u32,
    pub running: u32,
    pub tasks: BTreeMap<String, TaskView>,
    pub agents: BTreeMap<String, AgentView>,
    pub last_event_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TaskView {
    pub state: String,
    pub progress: u8,
    pub agent_id: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentView {
    pub state: String,
    pub task_id: Option<String>,
    pub progress: u8,
}

impl SwarmView {
    pub fn apply(&mut self, event: SwarmEvent) {
        self.swarm_id = Some(event.swarm_id);
        self.last_event_id = Some(event.event_id);
        if let Some(task_id) = event.task_id.clone() {
            let task = self.tasks.entry(task_id).or_default();
            task.state = event.state.clone().unwrap_or_else(|| format!("{:?}", event.kind));
            task.progress = event.progress.unwrap_or(task.progress);
            task.agent_id = event.agent_id.clone().or(task.agent_id.clone());
            task.evidence.extend(event.evidence.clone());
        }
        if let Some(agent_id) = event.agent_id.clone() {
            let agent = self.agents.entry(agent_id).or_default();
            agent.state = event.state.clone().unwrap_or_else(|| format!("{:?}", event.kind));
            agent.task_id = event.task_id.clone().or(agent.task_id.clone());
            agent.progress = event.progress.unwrap_or(agent.progress);
        }
        match event.kind {
            SwarmEventKind::TaskStarted | SwarmEventKind::ResolverStarted => self.running = self.running.saturating_add(1),
            SwarmEventKind::TaskCompleted | SwarmEventKind::VerificationPassed => { self.completed = self.completed.saturating_add(1); self.running = self.running.saturating_sub(1); }
            SwarmEventKind::TaskFailed | SwarmEventKind::TaskCancelled | SwarmEventKind::VerificationFailed => { self.failed = self.failed.saturating_add(1); self.running = self.running.saturating_sub(1); }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_events::{SwarmEvent, SwarmEventKind};

    #[test]
    fn reducer_builds_task_and_agent_state() {
        let id = Uuid::new_v4();
        let task = SwarmEvent::new(id, SwarmEventKind::TaskStarted).task("t1").agent("a1").progress(30).state("running");
        let done = SwarmEvent::new(id, SwarmEventKind::TaskCompleted).task("t1").agent("a1").progress(100).state("done");
        let mut view = SwarmView::default();
        view.apply(task);
        view.apply(done);
        assert_eq!(view.tasks["t1"].progress, 100);
        assert_eq!(view.agents["a1"].state, "done");
        assert_eq!(view.completed, 1);
        assert_eq!(view.running, 0);
    }
}
