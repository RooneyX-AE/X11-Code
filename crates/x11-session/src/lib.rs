use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub goal: String,
    pub events: Vec<serde_json::Value>,
}

impl Session {
    pub fn new(goal: impl Into<String>) -> Self {
        Self { id: Uuid::new_v4(), goal: goal.into(), events: Vec::new() }
    }

    pub fn append(&mut self, event: serde_json::Value) { self.events.push(event); }

    pub fn save_json(&self) -> Result<String> { Ok(serde_json::to_string_pretty(self)?) }

    pub fn load_json(input: &str) -> Result<Self> { Ok(serde_json::from_str(input)?) }
}
