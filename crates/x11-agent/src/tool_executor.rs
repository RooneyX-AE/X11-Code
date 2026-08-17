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
pub struct ToolExecutor { registry: ToolRegistry, workspace: PathBuf, locks: WorkspaceLockManager }

impl ToolExecutor {
    pub fn new(registry: ToolRegistry, workspace: PathBuf) -> Self { Self { registry, workspace, locks: WorkspaceLockManager::default() } }
    pub fn with_locks(registry: ToolRegistry, workspace: PathBuf, locks: WorkspaceLockManager) -> Self { Self { registry, workspace, locks } }

    fn allowed(name: &str, rules: Option<&BTreeSet<String>>) -> bool {
        let Some(rules) = rules else { return true; };
        if rules.is_empty() { return false; }
        rules.iter().any(|rule| rule == "*" || rule == name || (rule.ends_with("*") && name.starts_with(rule.trim_end_matches('*'))))
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
        if !Self::allowed(&req.name, req.allowed_tools.as_ref()) { anyhow::bail!("tool '{}' is outside the agent allowlist", req.name); }
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
                changed = cancel.changed() => { if changed.is_err() || *cancel.borrow() { Err(anyhow::anyhow!("execution cancelled")) } else { call.await } }
            };
            match timeout(req.timeout, async { result }).await {
                Ok(Ok(output)) => return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:true, output, duration:started.elapsed(), attempts:attempt, timed_out:false, cancelled:false }),
                Ok(Err(error)) => {
                    last_error = error.to_string();
                    if *cancel.borrow() || attempt == attempts { return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:false, output:last_error, duration:started.elapsed(), attempts:attempt, timed_out:false, cancelled:*cancel.borrow() }); }
                }
                Err(_) => return Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name.clone(), success:false, output:format!("tool timed out after {} ms", req.timeout.as_millis()), duration:started.elapsed(), attempts:attempt, timed_out:true, cancelled:false }),
            }
        }
        Ok(ToolExecutionResult { call_id:req.call_id, tool:req.name, success:false, output:last_error, duration:started.elapsed(), attempts, timed_out:false, cancelled:false })
    }
}

#[cfg(test)]
mod tests {
    use super::*; use serde_json::json; use std::collections::BTreeSet;
    fn req(name:&str, args:Value, allowed:Option<BTreeSet<String>>) -> ToolExecutionRequest { ToolExecutionRequest{call_id:Uuid::new_v4(),name:name.into(),arguments:args,timeout:Duration::from_secs(2),max_attempts:1,allowed_tools:allowed,lock_key:None} }
    #[tokio::test] async fn unknown_tool_is_rejected(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);assert!(ex.execute(req("missing",json!({}),None),rx).await.is_err());}
    #[tokio::test] async fn allowlist_is_enforced_at_execution(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);let mut allowed=BTreeSet::new();allowed.insert("read_file".into());let result=ex.execute(req("git_status",json!({}),Some(allowed)),rx).await;assert!(result.is_err());}
    #[tokio::test] async fn wildcard_allowlist_works(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);let mut allowed=BTreeSet::new();allowed.insert("git_*".into());let result=ex.execute(req("git_status",json!({}),Some(allowed)),rx).await.unwrap();assert!(result.success);}
    #[tokio::test] async fn read_tool_executes(){let ex=ToolExecutor::new(ToolRegistry::builtins(),std::env::current_dir().unwrap());let(_tx,rx)=watch::channel(false);let result=ex.execute(req("read_file",json!({"path":"Cargo.toml"}),None),rx).await.unwrap();assert!(result.success);}
}
