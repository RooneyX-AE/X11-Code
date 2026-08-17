pub mod manager;

use anyhow::{Context, Result};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{process::Command, time::timeout};
use uuid::Uuid;
use x11_context::Context as MessageContext;
use x11_core::{orchestration::{HookEvent, Orchestrator}, verification::{VerificationKind, VerificationPlan, VerificationStep}, AgentSnapshot, AgentState};
use x11_model::{CompletionRequest, ModelProvider};
use x11_permissions::{Decision, Operation, Policy};
use x11_protocol::{AgentEvent, SessionId};
use x11_session::Session;
use x11_tools::{ToolContext, ToolKind, ToolRegistry};

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub workspace: PathBuf,
    pub model: String,
    pub max_iterations: u32,
    pub auto_approve: bool,
    pub max_context_tokens: usize,
    pub session_path: Option<PathBuf>,
    pub verification_commands: Vec<String>,
    pub verification_timeout_ms: u64,
    pub hooks_enabled: bool,
}
impl Default for AgentConfig {
    fn default()->Self{Self{
        workspace:std::env::current_dir().unwrap_or_else(|_|PathBuf::from(".")),
        model:"default".into(),
        max_iterations:20,
        auto_approve:false,
        max_context_tokens:12000,
        session_path:None,
        verification_commands:vec!["git diff --check".into()],
        verification_timeout_ms:120_000,
        hooks_enabled:false,
    }}
}

pub struct AgentRuntime<P:ModelProvider>{
    pub snapshot:AgentSnapshot,
    pub config:AgentConfig,
    pub provider:Arc<P>,
    pub tools:ToolRegistry,
    pub policy:Policy,
    pub context:MessageContext,
    pub session:Session,
    pub orchestration:Orchestrator,
    pub verification:VerificationPlan,
}
impl<P:ModelProvider> AgentRuntime<P>{
    pub fn new(goal:impl Into<String>,config:AgentConfig,provider:P)->Self{
        Self::new_shared(goal, config, Arc::new(provider))
    }

    pub fn new_shared(goal:impl Into<String>,config:AgentConfig,provider:Arc<P>)->Self{
        let goal=goal.into();
        let mut orchestration=Orchestrator::default();
        orchestration.install_defaults();
        let verification=Self::verification_plan(&config);
        Self{snapshot:AgentSnapshot::new(goal.clone(),config.max_iterations),session:Session::new(goal),provider,tools:ToolRegistry::builtins(),policy:Policy::default(),context:MessageContext::default(),config,orchestration,verification}
    }

    fn verification_plan(config:&AgentConfig)->VerificationPlan {
        let mut plan=VerificationPlan::default();
        for command in &config.verification_commands {
            plan.push(VerificationStep{kind:Self::kind_for_command(command),description:format!("verify: {command}"),command:Some(command.clone()),required:true});
        }
        plan
    }

    fn kind_for_command(command:&str)->VerificationKind {
        if command.contains("git diff") { VerificationKind::GitDiff }
        else if command.contains("test") || command.contains("pytest") || command.contains("cargo nextest") { VerificationKind::Test }
        else if command.contains("build") || command.contains("cargo check") || command.contains("cargo build") { VerificationKind::Build }
        else { VerificationKind::Command }
    }

    async fn checkpoint(&self)->Result<()> { if let Some(path)=&self.config.session_path { self.session.save_to(path).await?; } Ok(()) }
    fn emit(&mut self,event:AgentEvent){self.session.append(event);}

    async fn run_hooks(&mut self,event:HookEvent)->Result<()> {
        if !self.config.hooks_enabled { return Ok(()); }
        let hooks=self.orchestration.hooks(event).cloned().collect::<Vec<_>>();
        for hook in hooks {
            let decision=self.policy.decide(Operation::Shell);
            let allowed=matches!(decision,Decision::Allow)||(matches!(decision,Decision::Ask)&&self.config.auto_approve);
            if !allowed {
                self.emit(AgentEvent::Error{message:format!("hook '{}' skipped: shell permission {:?}",hook.name,decision)});
                continue;
            }
            let mut cmd=Command::new(if cfg!(windows){"cmd"}else{"sh"});
            cmd.args(if cfg!(windows){vec!["/C",hook.command.as_str()]}else{vec!["-lc",hook.command.as_str()]});
            cmd.current_dir(&self.config.workspace);
            let result=timeout(Duration::from_millis(self.config.verification_timeout_ms),cmd.output()).await;
            match result {
                Ok(Ok(output)) if output.status.success()=>self.emit(AgentEvent::StateChanged{state:format!("hook:{}:ok",hook.name)}),
                Ok(Ok(output))=>self.emit(AgentEvent::Error{message:format!("hook '{}' failed: exit={} stderr={}",hook.name,output.status.code().unwrap_or(-1),String::from_utf8_lossy(&output.stderr))}),
                Ok(Err(error))=>self.emit(AgentEvent::Error{message:format!("hook '{}' spawn failed: {error}",hook.name)}),
                Err(_)=>self.emit(AgentEvent::Error{message:format!("hook '{}' timed out",hook.name)}),
            }
        }
        Ok(())
    }

    async fn verify(&mut self)->Result<bool> {
        if self.verification.is_empty() {
            self.emit(AgentEvent::Verification{passed:true,summary:"verification plan is empty".into()});
            return Ok(true);
        }
        let steps=self.verification.required_steps().cloned().collect::<Vec<_>>();
        let mut passed=true;
        for step in steps {
            let Some(command)=step.command.clone() else { continue; };
            let mut cmd=Command::new(if cfg!(windows){"cmd"}else{"sh"});
            cmd.args(if cfg!(windows){vec!["/C",command.as_str()]}else{vec!["-lc",command.as_str()]});
            cmd.current_dir(&self.config.workspace);
            let output=timeout(Duration::from_millis(self.config.verification_timeout_ms),cmd.output()).await;
            match output {
                Ok(Ok(out))=>{
                    let success=out.status.success();
                    passed &= success;
                    let details=format!("{}\nstdout:\n{}\nstderr:\n{}",step.description,String::from_utf8_lossy(&out.stdout),String::from_utf8_lossy(&out.stderr));
                    self.emit(AgentEvent::Verification{passed:success,summary:details.clone()});
                    self.context.push("verification",details);
                }
                Ok(Err(error))=>{
                    passed=false;
                    self.emit(AgentEvent::Verification{passed:false,summary:format!("{}: {error}",step.description)});
                }
                Err(_)=>{
                    passed=false;
                    self.emit(AgentEvent::Verification{passed:false,summary:format!("{}: timed out",step.description)});
                }
            }
        }
        Ok(passed)
    }

    fn skill_prompt(&self)->String {
        let skills=self.orchestration.skills().map(|skill|format!("### {}\n{}\nTools: {}",skill.name,skill.instructions,skill.tool_hints.join(", "))).collect::<Vec<_>>().join("\n\n");
        format!("Available X11 Code operating skills:\n{}",skills)
    }

    pub async fn run(&mut self)->Result<String>{
        self.run_hooks(HookEvent::BeforeRun).await?;
        self.emit(AgentEvent::SessionStarted{session_id:SessionId(self.snapshot.session_id)});
        self.snapshot.transition(AgentState::Planning)?;
        self.context.push("system",self.system_prompt());
        self.context.push("system",self.skill_prompt());
        self.context.push("user",self.snapshot.goal.clone());
        self.checkpoint().await?;

        for _ in 0..self.snapshot.max_iterations {
            self.snapshot.iteration+=1;
            self.snapshot.transition(AgentState::Executing)?;
            self.emit(AgentEvent::StateChanged{state:"executing".into()});
            self.context.compact(self.config.max_context_tokens);

            let req=CompletionRequest{model:self.config.model.clone(),messages:self.context.to_messages(),tools:self.tools.definitions(),temperature:Some(0.1),max_tokens:Some(8192)};
            let response=self.provider.complete(req).await.context("model completion failed")?;
            if !response.text.is_empty(){self.context.push("assistant",response.text.clone());self.emit(AgentEvent::AssistantDelta{text:response.text.clone()});}

            if response.tool_calls.is_empty(){
                self.snapshot.transition(AgentState::Verifying)?;
                let verification_ok=self.verify().await?;
                if verification_ok && !response.text.is_empty() {
                    self.snapshot.transition(AgentState::Completed)?;
                    self.emit(AgentEvent::SessionFinished{success:true});
                    self.run_hooks(HookEvent::AfterRun).await?;
                    self.checkpoint().await?;
                    return Ok(response.text)
                }
                let reason=if response.text.is_empty(){"empty model response"}else{"verification failed"};
                self.snapshot.last_error=Some(reason.into());
                self.emit(AgentEvent::Error{message:reason.into()});
                if response.text.is_empty() { self.snapshot.transition(AgentState::Failed)?; self.emit(AgentEvent::SessionFinished{success:false}); self.run_hooks(HookEvent::OnError).await?; self.checkpoint().await?; anyhow::bail!("model returned an empty response"); }
                self.snapshot.transition(AgentState::Planning)?;
                self.context.push("system","Verification failed. Inspect the verification output, repair the root cause, and verify again.");
                self.checkpoint().await?;
                continue;
            }

            for call in response.tool_calls {
                let id=Uuid::parse_str(&call.id).unwrap_or_else(|_|Uuid::new_v4());
                self.emit(AgentEvent::ToolRequested{call_id:id,tool:call.name.clone(),input:call.arguments.clone()});
                self.run_hooks(HookEvent::BeforeTool).await?;
                let tool=self.tools.get(&call.name).ok_or_else(||anyhow::anyhow!("unknown tool: {}",call.name))?;
                let op=match tool.kind(){ToolKind::ReadOnly=>Operation::Read,ToolKind::FilesystemWrite=>Operation::FilesystemWrite,ToolKind::Shell=>Operation::Shell,ToolKind::GitWrite=>Operation::GitWrite,ToolKind::Network=>Operation::Network};
                let decision=self.policy.decide(op);
                let allowed=matches!(decision,Decision::Allow)||(matches!(decision,Decision::Ask)&&self.config.auto_approve);
                if !allowed {
                    let reason=format!("permission decision: {decision:?}");
                    self.context.push("tool",format!("{} => DENIED: {}",call.name,reason));
                    self.emit(AgentEvent::ApprovalRequested{call_id:id,tool:call.name.clone(),reason});
                    self.emit(AgentEvent::ToolCompleted{call_id:id,success:false,output:"tool call denied by permission policy".into()});
                    continue;
                }
                let result=self.tools.execute(&ToolContext{workspace:self.config.workspace.clone()},&call.name,call.arguments.clone()).await;
                match result{
                    Ok(out)=>{self.context.push("tool",format!("{} => {}",call.name,out));self.emit(AgentEvent::ToolCompleted{call_id:id,success:true,output:out});}
                    Err(e)=>{let msg=e.to_string();self.context.push("tool",format!("{} => ERROR: {}",call.name,msg.clone()));self.emit(AgentEvent::ToolCompleted{call_id:id,success:false,output:msg.clone()});self.emit(AgentEvent::Error{message:msg});}
                }
                self.run_hooks(HookEvent::AfterTool).await?;
            }
            self.snapshot.transition(AgentState::Verifying)?;
            self.emit(AgentEvent::Verification{passed:true,summary:format!("iteration {} completed; tool results returned to context",self.snapshot.iteration)});
            self.snapshot.transition(AgentState::Planning)?;
            self.checkpoint().await?;
        }

        self.snapshot.last_error=Some("iteration limit reached".into());
        self.snapshot.transition(AgentState::Failed)?;
        self.emit(AgentEvent::Error{message:"iteration limit reached".into()});
        self.emit(AgentEvent::SessionFinished{success:false});
        self.run_hooks(HookEvent::OnError).await?;
        self.checkpoint().await?;
        anyhow::bail!("agent iteration limit reached")
    }

    fn system_prompt(&self)->String{format!("You are X11 Code, an autonomous coding agent. Work only inside the workspace. Goal: {}\nUse tools deliberately. Inspect before editing, make narrow changes, inspect diffs after edits, and use tests or other verification commands when the task requires them. Treat every tool result as authoritative and never invent output. Workspace: {}",self.snapshot.goal,self.config.workspace.display())}
}

pub struct Agent;
impl Agent{pub fn new(goal:impl Into<String>)->Self{let _=goal;Self}}
