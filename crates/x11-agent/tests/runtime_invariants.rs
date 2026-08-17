use x11_agent::{AgentConfig, AgentRuntime};
use x11_model::MockProvider;
use x11_core::AgentState;

#[tokio::test]
async fn mock_runtime_reaches_terminal_state_without_tools() {
    let mut config = AgentConfig::default();
    config.verification_commands.clear();
    let mut runtime = AgentRuntime::new("report the goal", config, MockProvider);
    let result = runtime.run().await.unwrap();
    assert!(result.contains("report the goal"));
    assert_eq!(runtime.snapshot.state, AgentState::Completed);
}

#[tokio::test]
async fn cancellation_is_terminal_and_does_not_panic() {
    let mut config = AgentConfig::default();
    config.verification_commands.clear();
    let mut runtime = AgentRuntime::new("cancel me", config, MockProvider);
    runtime.cancel();
    let _ = runtime.run().await.unwrap();
    assert_eq!(runtime.snapshot.state, AgentState::Cancelled);
}
