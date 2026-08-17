use std::{collections::BTreeSet, path::PathBuf};

use tokio::fs;
use uuid::Uuid;
use x11_agent::{
    manager::AgentManagerConfig,
    swarm_adapter::SwarmAdapter,
    swarm_event_bus::SwarmEventBus,
    swarm_events::SwarmEventKind,
    AgentConfig, AgentRuntime,
};
use x11_core::{SubagentRole, SubagentSpec};
use x11_model::MockProvider;

use x11_tui::live_swarm::LiveSwarmBridge;

fn spec(id: &str) -> SubagentSpec {
    SubagentSpec {
        id: id.into(),
        role: SubagentRole::Explorer,
        goal: "inspect the temporary workspace".into(),
        max_iterations: 1,
        model: "mock".into(),
        token_budget: 4_000,
        tool_budget: 4,
        allowed_tools: BTreeSet::new(),
        dependencies: BTreeSet::new(),
        priority: 0,
        workspace_scope: None,
    }
}

fn temp_workspace() -> PathBuf {
    std::env::temp_dir().join(format!("x11-e2e-{}", Uuid::new_v4()))
}

#[tokio::test]
async fn swarm_runs_end_to_end_and_reaches_tui_state() {
    let workspace = temp_workspace();
    fs::create_dir_all(&workspace).await.unwrap();

    let mut config = AgentConfig::default();
    config.workspace = workspace.clone();
    config.model = "mock".into();
    config.verification_commands.clear();

    let parent = AgentRuntime::new("run an e2e swarm", config, MockProvider);
    let bus = SwarmEventBus::new(256);
    let mut bridge = LiveSwarmBridge::subscribe(&bus);
    let state_path = workspace.join("swarm.json");

    let report = SwarmAdapter::run_with_parent(
        &parent,
        vec![spec("explorer")],
        AgentManagerConfig {
            max_concurrency: 1,
            timeout_ms: 30_000,
            event_bus: Some(bus.clone()),
            swarm_id: Some(Uuid::new_v4()),
            state_path: Some(state_path.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let applied = bridge.try_poll();
    assert!(applied >= 5, "expected swarm lifecycle events, got {applied}");
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.succeeded, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(bridge.view.completed, 2, "task + verification should complete");
    assert!(bridge.view.running <= 1);
    assert!(bridge.view.last_event_id.is_some());
    assert!(bridge.view.tasks.contains_key("explorer"));
    assert!(fs::try_exists(&state_path).await.unwrap());

    let kinds = {
        let mut replay = SwarmEventBus::new(64);
        let mut rx = replay.subscribe();
        replay.emit(x11_agent::swarm_events::SwarmEvent::new(
            report.swarm_id,
            SwarmEventKind::SwarmStarted,
        ));
        replay.emit(x11_agent::swarm_events::SwarmEvent::new(
            report.swarm_id,
            SwarmEventKind::SwarmCompleted,
        ));
        vec![rx.recv().await.unwrap().kind, rx.recv().await.unwrap().kind]
    };
    assert!(kinds.contains(&SwarmEventKind::SwarmStarted));
    assert!(kinds.contains(&SwarmEventKind::SwarmCompleted));

    fs::remove_dir_all(workspace).await.unwrap();
}
