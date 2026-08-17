use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::fs;
use uuid::Uuid;
use anyhow::Result;
use async_trait::async_trait;
use x11_agent::{AgentConfig, AgentRuntime};
use x11_agent::conflict_resolution::{ConflictHunk, ConflictResolutionGate, ResolutionProposal};
use x11_agent::conflict_resolver::{ConflictReport, MergeDecision};
use x11_agent::resolution_apply::ResolutionApplier;
use x11_agent::resolution_transaction::ResolutionTransaction;
use x11_context::Context;
use x11_model::{CompletionRequest, CompletionResponse, Message, MockProvider, ModelProvider};
use x11_permissions::{Decision, Operation, Policy};
use x11_protocol::{stream::EventBus, AgentEvent};
use x11_session::Session;
use x11_tools::{ToolContext, ToolRegistry};

struct EmptyProvider;
#[async_trait]
impl ModelProvider for EmptyProvider {
    fn name(&self) -> &'static str { "empty" }
    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse { text: String::new(), tool_calls: Vec::new(), finish_reason: Some("stop".into()), usage: Default::default() })
    }
}

struct CountingProvider { calls: Arc<AtomicUsize> }
#[async_trait]
impl ModelProvider for CountingProvider {
    fn name(&self) -> &'static str { "counting" }
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CompletionResponse { text: request.messages.last().map(|m| m.content.clone()).unwrap_or_default(), tool_calls: Vec::new(), finish_reason: Some("stop".into()), usage: Default::default() })
    }
}

#[tokio::test]
async fn full_spine_smoke_test() {
    let root = std::env::temp_dir().join(format!("x11-integration-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).await.unwrap();
    fs::write(root.join("hello.txt"), "hello").await.unwrap();

    let provider = MockProvider;
    let request = CompletionRequest { model: "mock".into(), messages: vec![Message::user("hello")], tools: vec![], temperature: None, max_tokens: Some(128) };
    let completion = provider.complete(request).await.unwrap();
    assert!(completion.text.contains("hello"));

    let mut context = Context::default();
    context.push("system", "rules");
    context.push("user", "hello");
    context.push_assistant_message(completion.text.clone(), completion.tool_calls.clone());
    assert!(context.estimated_tokens() > 0);
    assert_eq!(context.to_messages().len(), 3);

    let mut policy = Policy::default();
    assert_eq!(policy.decide(Operation::Read), Decision::Allow);
    policy.rules.push(x11_permissions::Rule { decision: Decision::Deny, operation: Some(Operation::Shell), pattern: Some("rm -rf*".into()) });
    assert_eq!(policy.decide_for(Operation::Shell, "rm -rf build"), Decision::Deny);
    assert_eq!(policy.decide_for(Operation::Shell, "cargo test"), Decision::Ask);

    let tools = ToolRegistry::builtins();
    let result = tools.execute(&ToolContext { workspace: root.clone() }, "read_file", serde_json::json!({"path":"hello.txt"})).await.unwrap();
    assert_eq!(result, "hello");

    let bus = EventBus::new(16);
    let mut rx = bus.subscribe();
    bus.emit(AgentEvent::StateChanged { state: "executing".into() }).unwrap();
    assert!(matches!(rx.recv().await.unwrap(), AgentEvent::StateChanged { .. }));

    let session = Session::new("integration");
    let session_path = root.join("session.json");
    session.save_to(&session_path).await.unwrap();
    let loaded = Session::load_from(&session_path).await.unwrap();
    assert_eq!(loaded.goal, "integration");

    let mut config = AgentConfig { workspace: PathBuf::from(&root), model: "mock".into(), verification_commands: Vec::new(), session_path: Some(session_path), ..AgentConfig::default() };
    config.auto_approve = false;
    let mut agent = AgentRuntime::new("integration goal", config, MockProvider);
    let output = agent.run().await.unwrap();
    assert!(output.contains("integration goal"));

    let _ = fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn agent_cancellation_is_terminal() {
    let root = std::env::temp_dir().join(format!("x11-integration-cancel-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).await.unwrap();
    let config = AgentConfig { workspace: PathBuf::from(&root), model: "mock".into(), verification_commands: Vec::new(), ..AgentConfig::default() };
    let mut agent = AgentRuntime::new("cancel me", config, MockProvider);
    agent.cancel();
    let result = agent.run().await;
    assert!(result.is_ok());
    assert!(agent.is_cancelled());
    assert!(agent.snapshot.state.is_terminal());
    let _ = fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn empty_model_response_fails_loudly() {
    let root = std::env::temp_dir().join(format!("x11-integration-empty-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).await.unwrap();
    let config = AgentConfig { workspace: root.clone(), model: "empty".into(), verification_commands: Vec::new(), ..AgentConfig::default() };
    let mut agent = AgentRuntime::new("must fail", config, EmptyProvider);
    let result = agent.run().await;
    assert!(result.is_err());
    assert_eq!(agent.snapshot.state, x11_core::AgentState::Failed);
    let _ = fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn session_tampering_is_rejected_after_persistence() {
    let root = std::env::temp_dir().join(format!("x11-integration-session-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).await.unwrap();
    let path = root.join("session.json");
    let session = Session::new("integrity goal");
    session.save_to(&path).await.unwrap();
    let raw = fs::read_to_string(&path).await.unwrap();
    let tampered = raw.replacen("integrity goal", "tampered goal", 1);
    fs::write(&path, tampered).await.unwrap();
    assert!(Session::load_from(&path).await.is_err());
    let _ = fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn read_tool_denies_workspace_escape() {
    let root = std::env::temp_dir().join(format!("x11-integration-tools-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).await.unwrap();
    let result = ToolRegistry::builtins().execute(&ToolContext { workspace: root.clone() }, "read_file", serde_json::json!({"path":"../outside.txt"})).await;
    assert!(result.is_err());
    let _ = fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn policy_deny_is_not_retroactively_overridden_by_default() {
    let mut policy = Policy::default();
    policy.rules.push(x11_permissions::Rule { decision: Decision::Deny, operation: Some(Operation::Shell), pattern: Some("danger*".into()) });
    assert_eq!(policy.decide_for(Operation::Shell, "danger command"), Decision::Deny);
    assert_eq!(policy.decide_for(Operation::Shell, "safe command"), Decision::Ask);
}

#[tokio::test]
async fn context_compaction_preserves_tool_exchange() {
    let mut context = Context::default();
    context.push("system", "rules");
    context.push("user", "goal");
    for _ in 0..10 {
        context.push_assistant_tool_calls(vec![x11_model::ToolCall { id: "call-1".into(), name: "read_file".into(), arguments: serde_json::json!({"path":"a"}) }]);
        context.push_tool_result("call-1", "result");
    }
    context.compact(200);
    let messages = context.to_messages();
    for pair in messages.windows(2) {
        if !pair[0].tool_calls.is_empty() { assert_eq!(pair[1].role, "tool"); assert_eq!(pair[1].tool_call_id.as_deref(), Some("call-1")); }
    }
}

#[tokio::test]
async fn provider_is_called_once_for_successful_completion() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = CountingProvider { calls: Arc::clone(&calls) };
    let response = provider.complete(CompletionRequest { model: "counting".into(), messages: vec![Message::user("hello")], tools: vec![], temperature: None, max_tokens: Some(64) }).await.unwrap();
    assert_eq!(response.text, "hello");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn resolver_apply_then_verification_failure_rolls_back() {
    let root = std::env::temp_dir().join(format!("x11-integration-resolver-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).await.unwrap();
    let path = root.join("src.rs");
    fs::write(&path, "fn main() {\n    old();\n}\n").await.unwrap();

    let report = ConflictReport { decision: MergeDecision::ResolveRequired, overlapping_files: vec!["src.rs".into()], groups: vec![vec!["agent-a".into(), "agent-b".into()]] };
    let hunk = ConflictHunk { path: "src.rs".into(), start_line: 2, end_line: 2, agent_ids: vec!["agent-a".into(), "agent-b".into()], before: "    old();".into(), alternatives: vec!["    new();".into()] };
    let proposal = ResolutionProposal { path: "src.rs".into(), start_line: 2, end_line: 2, source_agents: vec!["agent-a".into(), "agent-b".into()], replacement: "    new();".into(), rationale: "preserve both agent changes".into() };
    assert!(ConflictResolutionGate::validate(&report, &proposal).accepted);

    let snapshot = ResolutionTransaction::snapshot_file(&root, std::path::Path::new("src.rs")).await.unwrap();
    let applied = ResolutionApplier::apply(&root, &hunk, &proposal).await.unwrap();
    assert_eq!(applied, path);
    assert!(fs::read_to_string(&path).await.unwrap().contains("new();"));

    let verification_passed = false;
    if !verification_passed { ResolutionTransaction::rollback(&snapshot).await.unwrap(); }
    assert!(ResolutionTransaction::verify_unchanged(&snapshot).await.unwrap());
    assert!(fs::read_to_string(&path).await.unwrap().contains("old();"));

    let _ = fs::remove_dir_all(root).await;
}
