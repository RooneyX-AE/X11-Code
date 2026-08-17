use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    SessionStarted { session_id: SessionId },
    PlanCreated { steps: Vec<String> },
    ToolRequested { call_id: Uuid, tool: String, input: serde_json::Value },
    ToolCompleted { call_id: Uuid, success: bool, output: String },
    AssistantDelta { text: String },
    Verification { passed: bool, summary: String },
    SessionFinished { success: bool },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: Uuid,
    pub name: String,
    pub input: serde_json::Value,
}
