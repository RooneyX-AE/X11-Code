use serde_json::json;
use std::{collections::BTreeSet, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;
use x11_agent::tool_executor::{ToolExecutionRequest, ToolExecutor};
use x11_tools::ToolRegistry;

fn request(name: &str, arguments: serde_json::Value) -> ToolExecutionRequest {
    ToolExecutionRequest { call_id: Uuid::new_v4(), name: name.to_owned(), arguments, timeout: Duration::from_secs(5), max_attempts: 1, allowed_tools: None, lock_key: None }
}
fn executor() -> ToolExecutor { ToolExecutor::new(ToolRegistry::builtins(), std::env::current_dir().expect("workspace should be available")) }

#[tokio::test]
async fn shell_process_executes_end_to_end_without_sandbox() {
    let executor = executor().with_sandbox_mode("off").expect("off is valid");
    let (_tx, rx) = watch::channel(false);
    let command = if cfg!(windows) { "echo x11-e2e" } else { "printf x11-e2e" };
    let result = executor.execute(request("shell", json!({"command": command})), rx).await.expect("tool result");
    assert!(result.success, "shell failed: {}", result.output);
    assert!(result.output.contains("x11-e2e"), "unexpected output: {}", result.output);
}

#[tokio::test]
async fn git_status_process_executes_through_executor() {
    let executor = executor().with_sandbox_mode("off").expect("off is valid");
    let (_tx, rx) = watch::channel(false);
    let result = executor.execute(request("git_status", json!({})), rx).await.expect("tool result");
    assert!(result.success, "git_status failed: {}", result.output);
    assert!(result.output.contains("exit=0"), "unexpected output: {}", result.output);
}

#[tokio::test]
async fn allowlist_blocks_process_tool_before_spawn() {
    let executor = executor().with_sandbox_mode("off").expect("off is valid");
    let (_tx, rx) = watch::channel(false);
    let mut request = request("shell", json!({"command": "echo should-not-run"}));
    request.allowed_tools = Some(BTreeSet::from(["read_file".to_owned()]));
    assert!(executor.execute(request, rx).await.is_err());
}

#[tokio::test]
async fn timeout_is_reported_for_process_tool() {
    let executor = executor().with_sandbox_mode("off").expect("off is valid");
    let (_tx, rx) = watch::channel(false);
    let command = if cfg!(windows) { "ping 127.0.0.1 -n 6 > nul" } else { "sleep 1" };
    let mut request = request("shell", json!({"command": command}));
    request.timeout = Duration::from_millis(50);
    let result = executor.execute(request, rx).await.expect("tool result");
    assert!(!result.success);
    assert!(result.timed_out, "expected timed_out=true, got: {:?}", result);
}
