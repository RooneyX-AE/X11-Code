use anyhow::{Context, Result};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use x11_context::Context as MessageContext;
use x11_core::{AgentSnapshot, AgentState};
use x11_model::{CompletionRequest, Message, ModelProvider};
use x11_permissions::{Decision, Operation, Policy};
use x11_protocol::{AgentEvent, SessionId};
use x11_session::Session;
use x11_tools::{ToolContext, ToolKind, ToolRegistry};
use uuid::Uuid;

#[derive(Debug, Clone)] pub struct AgentConfig { pub workspace:PathBuf, pub model:String, pub max_iterations:u32, pub auto_approve:bool, pub max_context_tokens:usize }
impl Default for AgentConfig{fn default()->Self{Self{workspace:std::env::current_dir().unwrap_or_else(|_|PathBuf::from(".")),model:"default".into(),max_iterations:20,auto_approve:false,max_context_tokens:12000}}}

pub struct AgentRuntime<P:ModelProvider>{ pub snapshot:AgentSnapshot, pub config:AgentConfig, pub provider:Arc<P>, pub tools:ToolRegistry, pub policy:Policy, pub context:MessageContext, pub session:Session }
impl<P:ModelProvider> AgentRuntime<P>{
 pub fn new(goal:impl Into<String>,config:AgentConfig,provider:P)->Self{let goal=goal.into();Self{snapshot:AgentSnapshot::new(goal.clone(),config.max_iterations),session:Session::new(goal),provider:Arc::new(provider),tools:ToolRegistry::builtins(),policy:Policy::default(),context:MessageContext::default(),config}}
 pub async fn run(&mut self)->Result<String>{
  self.emit(AgentEvent::SessionStarted{session_id:SessionId(self.snapshot.session_id)});self.snapshot.transition(AgentState::Planning)?;self.context.push("system",self.system_prompt());self.context.push("user",self.snapshot.goal.clone());
  for _ in 0..self.snapshot.max_iterations {
   self.snapshot.iteration+=1;self.snapshot.transition(AgentState::Executing)?;self.emit(AgentEvent::StateChanged{state:"executing".into()});self.context.compact(self.config.max_context_tokens);
   let req=CompletionRequest{model:self.config.model.clone(),messages:self.context.to_messages(),tools:self.tools.definitions(),temperature:Some(0.1),max_tokens:Some(8192)};
   let response=self.provider.complete(req).await.context("model completion failed")?;
   if !response.text.is_empty(){self.context.push("assistant",response.text.clone());self.emit(AgentEvent::AssistantDelta{text:response.text.clone()});}
   if response.tool_calls.is_empty(){self.snapshot.transition(AgentState::Verifying)?;self.emit(AgentEvent::Verification{passed:true,summary:"model returned no further tool calls".into()});self.snapshot.transition(AgentState::Completed)?;self.emit(AgentEvent::SessionFinished{success:true});return Ok(response.text)}
   for call in response.tool_calls{
    let id=Uuid::parse_str(&call.id).unwrap_or_else(|_|Uuid::new_v4());self.emit(AgentEvent::ToolRequested{call_id:id,tool:call.name.clone(),input:call.arguments.clone()});
    let tool=self.tools.get(&call.name).ok_or_else(||anyhow::anyhow!("unknown tool: {}",call.name))?;
    let op=match tool.kind(){ToolKind::ReadOnly=>Operation::Read,ToolKind::FilesystemWrite=>Operation::FilesystemWrite,ToolKind::Shell=>Operation::Shell,ToolKind::GitWrite=>Operation::GitWrite,ToolKind::Network=>Operation::Network};
    let decision=self.policy.decide(op);let allowed=matches!(decision,Decision::Allow)||(matches!(decision,Decision::Ask)&&self.config.auto_approve);
    if !allowed {self.emit(AgentEvent::ApprovalRequested{call_id:id,tool:call.name.clone(),reason:format!("permission decision: {decision:?}")});self.emit(AgentEvent::ToolCompleted{call_id:id,success:false,output:"tool call denied by permission policy".into()});continue;}
    let result=self.tools.execute(&ToolContext{workspace:self.config.workspace.clone()},&call.name,call.arguments.clone()).await;match result{Ok(out)=>{self.context.push("tool",format!("{} => {}",call.name,out));self.emit(AgentEvent::ToolCompleted{call_id:id,success:true,output:out});}Err(e)=>{let msg=e.to_string();self.context.push("tool",format!("{} => ERROR: {}",call.name,msg));self.emit(AgentEvent::ToolCompleted{call_id:id,success:false,output:msg});}}
   }
   self.snapshot.transition(AgentState::Verifying)?;self.snapshot.transition(AgentState::Planning)?;
  }
  self.snapshot.last_error=Some("iteration limit reached".into());self.snapshot.transition(AgentState::Failed)?;self.emit(AgentEvent::Error{message:"iteration limit reached".into()});self.emit(AgentEvent::SessionFinished{success:false});anyhow::bail!("agent iteration limit reached")
 }
 fn system_prompt(&self)->String{format!("You are X11 Code, an autonomous coding agent. Work only inside the workspace. Goal: {}\nUse available tools deliberately. Prefer read/search before writes. Make small verifiable edits, run tests when useful, and stop only when the goal is satisfied. Never invent tool results. Workspace: {}",self.snapshot.goal,self.config.workspace.display())}
 fn emit(&mut self,event:AgentEvent){self.session.append(event);}
}

pub struct Agent; impl Agent{pub fn new(goal:impl Into<String>)->Self{let _=goal;Self} }
