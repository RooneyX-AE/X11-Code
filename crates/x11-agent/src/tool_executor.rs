use anyhow::{Context, Result};
use std::{sync::Arc, time::{Duration, Instant}};
use serde_json::Value;
use tokio::{sync::watch, time::timeout};
use uuid::Uuid;
use x11_tools::{ToolContext, ToolRegistry};

#[derive(Debug, Clone)]
pub struct ToolExecutionRequest {
    pub call_id: Uuid,
    pub name: String,
    pub arguments: Value,
    pub timeout: Duration,
    pub max_attempts: u32,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub call_id: Uuid,
    pub tool: String,
    pub success: bool,
    pub output: String,
    pub duration: Duration,
    pub attempts: u32,
    pub timed_out: bool,
    pub cancelled: bool,
}

#[derive(Clone)]
pub struct ToolExecutor {
    registry: ToolRegistry,
    workspace: std::path::PathBuf,
}

impl ToolExecutor {
    pub fn new(registry: ToolRegistry, workspace: std::path::PathBuf) -> Self {
        Self { registry, workspace }
    }

    pub async fn execute(&self, req: ToolExecutionRequest, cancel: watch::Receiver<bool>) -> Result<ToolExecutionResult> {
        if !self.registry.get(&req.name).is_some() {
            anyhow::bail!("unknown tool: {}", req.name);
        }
        let started = Instant::now();
        let attempts = req.max_attempts.clamp(1, 3);
        let mut last_error = String::new();
        for attempt in 1..=attempts {
            if *cancel.borrow() {
                return Ok(ToolExecutionResult { call_id: req.call_id, tool: req.name, success: false, output: "cancelled before execution".into(), duration: started.elapsed(), attempts: attempt - 1, timed_out: false, cancelled: true });
            }
            let ctx = ToolContext { workspace: self.workspace.clone() };
            let call = self.registry.execute(&ctx, &req.name, req.arguments.clone());
            tokio::pin!(call);
            let wait = async {
                tokio::select! {
                    result = &mut call => result,
                    _ = cancel.changed() => {
                        if *cancel.borrow() { Err(anyhow::anyhow!("execution cancelled")) } else { call.await }
                    }
                }
            };
            match timeout(req.timeout, wait).await {
                Ok(Ok(output)) => return Ok(ToolExecutionResult { call_id: req.call_id, tool: req.name, success: true, output, duration: started.elapsed(), attempts: attempt, timed_out: false, cancelled: false }),
                Ok(Err(error)) => { last_error = error.to_string(); if attempt == attempts { return Ok(ToolExecutionResult { call_id: req.call_id, tool: req.name, success: false, output: last_error, duration: started.elapsed(), attempts: attempt, timed_out: false, cancelled: *cancel.borrow() }); } }
                Err(_) => { return Ok(ToolExecutionResult { call_id: req.call_id, tool: req.name, success: false, output: format!("tool timed out after {} ms", req.timeout.as_millis()), duration: started.elapsed(), attempts: attempt, timed_out: true, cancelled: false }); }
            }
        }
        Err(anyhow::anyhow!("tool execution failed: {last_error}")).context("unreachable tool executor state")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::sync::watch;
    use x11_tools::ToolRegistry;

    #[tokio::test]
    async fn unknown_tool_is_rejected() {
        let ex = ToolExecutor::new(ToolRegistry::builtins(), std::env::current_dir().unwrap());
        let (_tx, rx) = watch::channel(false);
        let result = ex.execute(ToolExecutionRequest { call_id: Uuid::new_v4(), name: "missing".into(), arguments: json!({}), timeout: Duration::from_secs(1), max_attempts: 1 }, rx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_tool_executes() {
        let ex = ToolExecutor::new(ToolRegistry::builtins(), std::env::current_dir().unwrap());
        let (_tx, rx) = watch::channel(false);
        let result = ex.execute(ToolExecutionRequest { call_id: Uuid::new_v4(), name: "read_file".into(), arguments: json!({"path":"Cargo.toml"}), timeout: Duration::from_secs(2), max_attempts: 1 }, rx).await.unwrap();
        assert!(result.success);
    }
}
