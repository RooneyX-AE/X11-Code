use std::{collections::BTreeSet, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;
use x11_agent::tool_executor::{ToolExecutionRequest, ToolExecutor};
use x11_tools::ToolRegistry;

fn request(name: &str, arguments: serde_json::Value, allowed: Option<BTreeSet<String>>) -> ToolExecutionRequest {
    ToolExecutionRequest {
        call_id: Uuid::new_v4(),
        name: name.to_owned(),
        arguments,
        timeout: Duration::from_secs(2),
        max_attempts: 1,
        allowed_tools: allowed,
        lock_key: None,
    }
}

#[tokio::test]
async fn cancellation_is_observed_before_tool_execution() {
    let executor = ToolExecutor::new(ToolRegistry::builtins(), std::env::current_dir().unwrap());
    let (tx, rx) = watch::channel(true);
    let result = executor.execute(request("read_file", serde_json::json!({"path":"Cargo.toml"}), None), rx).await.unwrap();
    assert!(result.cancelled);
    assert!(!result.success);
    assert_eq!(result.attempts, 0);
    let _ = tx.send(false);
}

#[tokio::test]
async fn execution_allowlist_rejects_unlisted_tool() {
    let executor = ToolExecutor::new(ToolRegistry::builtins(), std::env::current_dir().unwrap());
    let (_tx, rx) = watch::channel(false);
    let allowed = BTreeSet::from(["read_file".to_owned()]);
    let error = executor.execute(request("git_status", serde_json::json!({}), Some(allowed)), rx).await.unwrap_err();
    assert!(error.to_string().contains("allowlist"));
}

#[tokio::test]
async fn execution_allowlist_accepts_exact_tool() {
    let executor = ToolExecutor::new(ToolRegistry::builtins(), std::env::current_dir().unwrap());
    let (_tx, rx) = watch::channel(false);
    let allowed = BTreeSet::from(["read_file".to_owned()]);
    let result = executor.execute(request("read_file", serde_json::json!({"path":"Cargo.toml"}), Some(allowed)), rx).await.unwrap();
    assert!(result.success);
}
