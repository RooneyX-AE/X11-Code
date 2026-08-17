use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;
use x11_agent::{AgentConfig, AgentRuntime};
use x11_context::Context;
use x11_model::{CompletionRequest, Message, MockProvider, ModelProvider};
use x11_permissions::{Decision, Operation, Policy};
use x11_protocol::{stream::EventBus, AgentEvent};
use x11_session::Session;
use x11_tools::{ToolContext, ToolRegistry};

#[tokio::test]
async fn full_spine_smoke_test() {
    let root = std::env::temp_dir().join(format!("x11-integration-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).await.unwrap();
    fs::write(root.join("hello.txt"), "hello").await.unwrap();

    let provider = MockProvider;
    let request = CompletionRequest {
        model: "mock".into(),
        messages: vec![Message::user("hello")],
        tools: vec![],
        temperature: None,
        max_tokens: Some(128),
    };
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
    policy.rules.push(x11_permissions::Rule {
        decision: Decision::Deny,
        operation: Some(Operation::Shell),
        pattern: Some("rm -rf*".into()),
    });
    assert_eq!(policy.decide_for(Operation::Shell, "rm -rf build"), Decision::Deny);
    assert_eq!(policy.decide_for(Operation::Shell, "cargo test"), Decision::Ask);

    let tools = ToolRegistry::builtins();
    let result = tools
        .execute(&ToolContext { workspace: root.clone() }, "read_file", serde_json::json!({"path":"hello.txt"}))
        .await
        .unwrap();
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

    let mut config = AgentConfig {
        workspace: PathBuf::from(&root),
        model: "mock".into(),
        verification_commands: Vec::new(),
        session_path: Some(session_path),
        ..AgentConfig::default()
    };
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
    let config = AgentConfig {
        workspace: PathBuf::from(&root),
        model: "mock".into(),
        verification_commands: Vec::new(),
        ..AgentConfig::default()
    };
    let mut agent = AgentRuntime::new("cancel me", config, MockProvider);
    agent.cancel();
    let result = agent.run().await;
    assert!(result.is_ok());
    assert!(agent.is_cancelled());
    assert!(agent.snapshot.state.is_terminal());
    let _ = fs::remove_dir_all(root).await;
}
