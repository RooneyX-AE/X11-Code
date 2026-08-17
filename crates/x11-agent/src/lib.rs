use anyhow::Result;
use x11_core::{AgentSnapshot, AgentState};

pub struct Agent {
    snapshot: AgentSnapshot,
}

impl Agent {
    pub fn new(goal: impl Into<String>) -> Self {
        Self { snapshot: AgentSnapshot::new(goal) }
    }

    pub fn snapshot(&self) -> &AgentSnapshot { &self.snapshot }

    pub fn start(&mut self) -> Result<()> {
        self.snapshot.state = AgentState::Planning;
        self.snapshot.iteration = self.snapshot.iteration.saturating_add(1);
        Ok(())
    }

    pub fn complete(&mut self) {
        self.snapshot.state = AgentState::Completed;
    }
}
