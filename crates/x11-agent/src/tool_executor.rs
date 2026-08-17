use anyhow::Result;
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::{Duration, Instant}};
use tokio::{sync::watch, time::timeout};
use uuid::Uuid;
use x11_tools::{ToolContext, ToolKind, ToolRegistry};
use crate::workspace_lock::WorkspaceLockManager;

#[derive(Debug, Clone)]
pub struct ToolExecutionRequest {
    pub call_id: Uuid,
    pub name: String,
    pub arguments: Value,
    pub timeout: Duration,
    pub max_attempts: u32,
    pub allowed_tools: Option<BTreeSet<String>>,
    pub lock_key: Option<String>,
}
#[derive(Debug, Clone)]
pub struct ToolExecutionResult { pub call_id: Uuid, pub tool: String, pub success: bool, pub output: String, pub duration: Duration, pub attempts: u32, pub timed_out: bool, pub cancelled: bool }
#[derive(Clone)]
pub struct ToolExecutor { registry: ToolRegistry, workspace: PathBuf, locks: WorkspaceLockManager, default_allowed_tools: Option<Arc<BTreeSet<String>>> }

impl ToolExecutor {
    pub fn new(registry: ToolRegistry, workspace: PathBuf) -> Self { Self { registry, workspace, locks: WorkspaceLockManager::default(), default_allowed_tools: None } }
    pub fn with_locks(registry: ToolRegistry, workspace: PathBuf, locks: WorkspaceLockManager) -> Self { Self { registry, workspace, locks, default_allowed_tools: None } }
    pub fn with_locks_and_allowlist(registry: ToolRegistry, workspace: PathBuf, locks: WorkspaceLockManager, allowed: BTreeSet<String>) -> Self { Self { registry, workspace, locks, default_allowed_tools: Some(Arc::new(allowed)) } }
    pub fn with_registry(&self, registry: ToolRegistry) -> Self { Self { registry, workspace: self.workspace.clone(), locks: self.locks.clone(), default_allowed_tools: self.default_allowed_tools.clone() } }

    fn allowed(name: &str, rules: Option<&BTreeSet<String>>) -> bool {
        let Some(rules) = rules else { return true; };
        if rules.is_empty() { return false; }
        rules.iter().any(|rule| rule == "*" || rule == name || (rule.ends_with('*') && name.starts_with(rule.trim_end_matches('*'))))
    }

    fn lock_key(&self, req: &ToolExecutionRequest) -> Option<String> {
        if let Some(key) = &req.lock_key { return Some(key.clone()); }
        let tool = self.registry.get(&req.name)?;
        match tool.kind() {
            ToolKind::FilesystemWrite => req.arguments["path"].as_str().map(|p| format!("file:{p}")),
            ToolKind::Shell | ToolKind::GitWrite => Some("workspace:global".into()),
            _ => None,
        }
    }

    pub async fn execute(&self, req: ToolExecutionRequest, mut cancel: watch::Receiver<bool>) -> Result<ToolExecutionResult> {
        if self.registry.get(&req.name).is_none() { anyhow::bail!("unknown tool: {}", req.name); }
        let configured = req.allowed_tools.as_ref().or(self.default_allowed_tools.as_deref());
        if !Self::allowed(&req.name, configured) { anyhow::bail!("tool '{}' is outside the agent allowlist", req.name); }
        let _lock = match self.lock_key(&req) { Some(key) => Some(self.locks.acquire(key).await), None => None };
        let started = Instant::now();
        let attempts = req.max_attempts.clamp(1, 3);
        let mut last_error = String::new();
        for attempt in 1..=attempts {
            if *cancel.borrow() {
                return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:false, output:"cancelled before execution".into(), duration:started.elapsed(), attempts:attempt-1, timed_out:false, cancelled:true });
            }
            let call = self.registry.execute(&ToolContext { workspace:self.workspace.clone() }, &req.name, req.arguments.clone());
            tokio::pin!(call);
            let result = tokio::select! {
                value = &mut call => value,
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() { Err(anyhow::anyhow!("execution cancelled")) } else { call.await }
                }
            };
            match timeout(req.timeout, async { result }).await {
                Ok(Ok(output)) => return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:true, output, duration:started.elapsed(), attempts:attempt, timed_out:false, cancelled:false }),
                Ok(Err(error)) => {
                    last_error = error.to_string();
                    if *cancel.borrow() { return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:false, output:last_error, duration:started.elapsed(), attempts:attempt, timed_out:false, cancelled:true }); }
                    if attempt == attempts { return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:false, output:last_error, duration:started.elapsed(), attempts:attempt, timed_out:false, cancelled:false }); }
                }
                Err(_) => {
                    last_error = format!("tool timed out after {} ms", req.timeout.as_millis());
                    if attempt == attempts { return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:false, output:last_error, duration:started.elapsed(), attempts:attempt, timed_out:true, cancelled:false }); }
                }
            }
        }
        Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name, success:false, output:last_error, duration:started.elapsed(), attempts, timed_out:false, cancelled:false })
    }
}

#[cfg(test)]
mod tests {
    use super::*; use async_trait::async_trait; use serde_json::json; use std::collections::BTreeSet; use tokio::time::sleep; use x11_tools::{Tool, ToolContext};
    fn req(name:&str, args:Value, allowed:Option<BTreeSet<String>>) -> ToolExecutionRequest { ToolExecutionRequest{call_id:Uuid::new_v4(),name:name.into(),arguments:args,timeout:Duration::from_secs(2),max_attempts:1,allowed_tools:allowed,lock_key:None} }
    #[tokio::test] async fn unknown_tool_is_rejected(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);assert!(ex.execute(req("missing",json!({}),None),rx).await.is_err());}
    #[tokio::test] async fn allowlist_is_enforced_at_execution(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);let mut allowed=BTreeSet::new();allowed.insert("read_file".into());assert!(ex.execute(req("git_status",json!({}),Some(allowed)),rx).await.is_err());}
    #[tokio::test] async fn executor_default_allowlist_is_hard_boundary(){let mut allowed=BTreeSet::new();allowed.insert("read_file".into());let ex=ToolExecutor{registry:ToolRegistry::builtins(),workspace:std::env::current_dir().unwrap(),locks:WorkspaceLockManager::default(),default_allowed_tools:Some(Arc::new(allowed))};let(_tx,rx)=watch::channel(false);assert!(ex.execute(req("git_status",json!({}),None),rx).await.is_err());}
    #[tokio::test] async fn wildcard_allowlist_works(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);let mut allowed=BTreeSet::new();allowed.insert("git_*".into());let result=ex.execute(req("git_status",json!({}),Some(allowed)),rx).await.unwrap();assert!(result.success);}
    #[tokio::test] async fn read_tool_executes(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);let result=ex.execute(req("read_file",json!({"path":"Cargo.toml"}),rx).await.unwrap();assert!(result.success);}

    struct SlowTool;
    #[async_trait]
    impl Tool for SlowTool { fn name(&self)->&str{"slow_test"} fn description(&self)->&str{"test-only slow tool"} fn kind(&self)->ToolKind{ToolKind::ReadOnly} fn input_schema(&self)->Value{json!({"type":"object","additionalProperties":false})} async fn execute(&self,_:&ToolContext,_:Value)->Result<String>{sleep(Duration::from_millis(40)).await;Ok("done".into())} }
    fn slow_executor() -> ToolExecutor { let mut registry=ToolRegistry::builtins(); registry.register(SlowTool); ToolExecutor::new(registry,std::env::current_dir().unwrap()) }
    #[tokio::test] async fn timeout_retries_but_stays_bounded(){let ex=slow_executor();let(_tx,rx)=watch::channel(false);let mut request=req("slow_test",json!({}),None);request.timeout=Duration::from_millis(5);request.max_attempts=3;let result=ex.execute(request,rx).await.unwrap();assert!(!result.success);assert!(result.timed_out);assert!(!result.cancelled);assert_eq!(result.attempts,3);}
    #[tokio::test] async fn cancellation_does_not_retry(){let ex=slow_executor();let(tx,rx)=watch::channel(false);let mut request=req("slow_test",json!({}),None);request.timeout=Duration::from_millis(500);request.max_attempts=3;let task=tokio::spawn(async move{ex.execute(request,rx).await.unwrap()});sleep(Duration::from_millis(5)).await;tx.send(true).unwrap();let result=task.await.unwrap();assert!(!result.success);assert!(result.cancelled);assert_eq!(result.attempts,0);assert!(!result.timed_out);}
    #[tokio::test] async fn registry_replacement_preserves_allowlist(){let mut allowed=BTreeSet::new();allowed.insert("read_file".into());let base=ToolExecutor::with_locks_and_allowlist(ToolRegistry::builtins(),std::env::current_dir().unwrap(),WorkspaceLockManager::default(),allowed);let replaced=base.with_registry(ToolRegistry::builtins());let(_tx,rx)=watch::channel(false);assert!(replaced.execute(req("git_status",json!({}),None),rx).await.is_err());assert!(replaced.execute(req("read_file",json!({"path":"Cargo.toml"}),None),rx).await.unwrap().success);}
}