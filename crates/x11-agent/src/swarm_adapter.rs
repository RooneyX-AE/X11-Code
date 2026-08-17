use anyhow::Result;
use std::sync::Arc;
use x11_core::SubagentSpec;
use x11_model::ModelProvider;
use crate::{manager::{AgentManager, AgentManagerConfig, SubagentResult, SwarmReport}, swarm_reviewer::{ReviewResult, SwarmReviewer}, AgentConfig, AgentRuntime};

pub struct SwarmAdapter;

impl SwarmAdapter {
    pub async fn run_with_parent<P: ModelProvider + 'static>(
        parent: &AgentRuntime<P>,
        specs: Vec<SubagentSpec>,
        mut config: AgentManagerConfig,
    ) -> Result<SwarmReport> {
        config.inherited_policy = Some(parent.policy.clone());
        config.cancellation = Some(parent.cancel.clone());
        if config.state_path.is_none() {
            config.state_path = Some(parent.swarm_state_path());
        }
        let manager = AgentManager::new(
            Arc::clone(&parent.provider),
            parent.config.workspace.clone(),
            AgentConfig { ..parent.config.clone() },
            config,
        );
        manager.run_report(specs).await
    }

    pub fn review(report: &SwarmReport) -> ReviewResult {
        SwarmReviewer::review(report)
    }

    pub async fn run_reviewed<P: ModelProvider + 'static>(
        parent: &AgentRuntime<P>,
        specs: Vec<SubagentSpec>,
        config: AgentManagerConfig,
    ) -> Result<(SwarmReport, ReviewResult)> {
        let report = Self::run_with_parent(parent, specs, config).await?;
        let review = Self::review(&report);
        Ok((report, review))
    }

    pub async fn run_results<P: ModelProvider + 'static>(
        parent: &AgentRuntime<P>,
        specs: Vec<SubagentSpec>,
        config: AgentManagerConfig,
    ) -> Result<Vec<SubagentResult>> {
        Ok(Self::run_with_parent(parent, specs, config).await?.results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, path::PathBuf};
    use x11_core::SubagentRole;
    use x11_model::MockProvider;

    fn spec() -> SubagentSpec {
        SubagentSpec {
            id: "explorer".into(), role: SubagentRole::Explorer, goal: "inspect".into(),
            max_iterations: 1, model: "default".into(), token_budget: 4_000, tool_budget: 4,
            allowed_tools: BTreeSet::from(["read_file".into()]), dependencies: BTreeSet::new(),
            priority: 0, workspace_scope: None,
        }
    }

    #[tokio::test]
    async fn adapter_inherits_parent_controls() {
        let mut cfg = AgentConfig::default();
        cfg.workspace = PathBuf::from(".");
        let parent = AgentRuntime::new("parent", cfg, MockProvider);
        let report = SwarmAdapter::run_with_parent(&parent, vec![spec()], AgentManagerConfig { max_concurrency: 1, timeout_ms: 5_000, ..Default::default() }).await.unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.succeeded, 1);
        assert_eq!(SwarmAdapter::review(&report).verdict, crate::swarm_reviewer::ReviewVerdict::Accept);
    }
}
