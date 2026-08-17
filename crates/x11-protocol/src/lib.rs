use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)] pub struct SessionId(pub Uuid);
#[derive(Debug, Clone, Serialize, Deserialize)] pub struct ToolCall { pub id:Uuid,pub name:String,pub input:serde_json::Value }
#[derive(Debug, Clone, Serialize, Deserialize)] pub enum AgentEvent{SessionStarted{session_id:SessionId},StateChanged{state:String},PlanCreated{steps:Vec<String>},AssistantDelta{text:String},ToolRequested{call_id:Uuid,tool:String,input:serde_json::Value},ToolCompleted{call_id:Uuid,success:bool,output:String},ApprovalRequested{call_id:Uuid,tool:String,reason:String},Verification{passed:bool,summary:String},Error{message:String},SessionFinished{success:bool}}
