use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct SwarmUiState {
    pub swarm_id: Option<Uuid>,
    pub tasks: BTreeMap<String, (String, u8, Option<String>)>,
    pub agents: BTreeMap<String, (String, u8, Option<String>)>,
    pub completed: u32,
    pub failed: u32,
    pub running: u32,
}

impl SwarmUiState {
    pub fn update(&mut self, swarm_id: Uuid, task_id: Option<String>, agent_id: Option<String>, state: String, progress: u8, completed: u32, failed: u32, running: u32) {
        self.swarm_id = Some(swarm_id);
        if let Some(id) = task_id { self.tasks.insert(id, (state.clone(), progress, agent_id.clone())); }
        if let Some(id) = agent_id { self.agents.insert(id, (state, progress, task_id)); }
        self.completed = completed;
        self.failed = failed;
        self.running = running;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_task_and_agent_views() {
        let id = Uuid::new_v4();
        let mut state = SwarmUiState::default();
        state.update(id, Some("task".into()), Some("agent".into()), "running".into(), 50, 0, 0, 1);
        assert_eq!(state.tasks["task"].1, 50);
        assert_eq!(state.agents["agent"].0, "running");
    }
}
