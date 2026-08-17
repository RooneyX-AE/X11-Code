use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Planning,
    Executing,
    Verifying,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub state: AgentState,
    pub iteration: u32,
    pub goal: String,
}

impl AgentSnapshot {
    pub fn new(goal: impl Into<String>) -> Self {
        Self { state: AgentState::Idle, iteration: 0, goal: goal.into() }
    }
}
