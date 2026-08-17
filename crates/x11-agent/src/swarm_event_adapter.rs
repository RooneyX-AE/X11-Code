use anyhow::Result;
use std::sync::Arc;
use x11_core::SubagentSpec;
use x11_model::ModelProvider;
use uuid::Uuid;

use crate::{manager::{AgentManager, AgentManagerConfig, SwarmReport}, swarm_event_bus::SwarmEventBus, swarm_events::{SwarmEvent, SwarmEventKind}, AgentConfig};

pub struct InstrumentedSwarm<P: ModelProvider + 'static> {
    manager: AgentManager<P>,
    bus: SwarmEventBus,
    swarm_id: Uuid,
}

impl<P: ModelProvider + 'static> InstrumentedSwarm<P> {
    pub fn new(provider: Arc<P>, workspace: std::path::PathBuf, base_config: AgentConfig, config: AgentManagerConfig, bus: SwarmEventBus) -> Self {
        Self { manager: AgentManager::new(provider, workspace, base_config, config), bus, swarm_id: Uuid::new_v4() }
    }

    pub fn swarm_id(&self) -> Uuid { self.swarm_id }
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SwarmEvent> { self.bus.subscribe() }

    pub async fn run(&self, specs: Vec<SubagentSpec>) -> Result<SwarmReport> {
        self.bus.emit(SwarmEvent::new(self.swarm_id, SwarmEventKind::SwarmStarted).progress(0));
        for spec in &specs {
            self.bus.emit(SwarmEvent::new(self.swarm_id, SwarmEventKind::TaskQueued).task(spec.id.clone()).state("queued"));
        }
        let report = self.manager.run_report(specs.clone()).await;
        match &report {
            Ok(value) => {
                for spec in &specs {
                    if let Some(result) = value.results.iter().find(|r| r.id == spec.id) {
                        self.bus.emit(SwarmEvent::new(self.swarm_id, if result.success { SwarmEventKind::TaskCompleted } else { SwarmEventKind::TaskFailed })
                            .task(result.id.clone()).agent(result.id.clone()).progress(100).state(if result.success { "completed" } else { "failed" }).evidence(result.verification.clone()));
                    }
                }
                self.bus.emit(SwarmEvent::new(self.swarm_id, SwarmEventKind::SwarmCompleted).progress(100).state(value.summary()));
            }
            Err(error) => {
                self.bus.emit(SwarmEvent::new(self.swarm_id, SwarmEventKind::TaskFailed).state("scheduler failed").evidence(error.to_string()));
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11_core::{SubagentRole, SubagentSpec};
    use x11_model::MockProvider;
    use std::collections::BTreeSet;

    #[tokio::test]
    async fn emits_lifecycle_events() {
        let bus = SwarmEventBus::new(32);
        let mut rx = bus.subscribe();
        let runner = InstrumentedSwarm::new(Arc::new(MockProvider), ".".into(), AgentConfig::default(), AgentManagerConfig::default(), bus);
        let spec = SubagentSpec { id: "explorer".into(), role: SubagentRole::Explorer, goal: "inspect".into(), max_iterations: 1, model: "default".into(), token_budget: 4000, tool_budget: 4, allowed_tools: BTreeSet::from(["read_file".into()]), dependencies: BTreeSet::new(), priority: 0, workspace_scope: None };
        let report = runner.run(vec![spec]).await.unwrap();
        assert_eq!(report.results.len(), 1);
        let mut kinds = Vec::new();
        while let Ok(event) = rx.try_recv() { kinds.push(event.kind); }
        assert!(kinds.contains(&SwarmEventKind::SwarmStarted));
        assert!(kinds.contains(&SwarmEventKind::TaskQueued));
        assert!(kinds.contains(&SwarmEventKind::SwarmCompleted));
    }
}
