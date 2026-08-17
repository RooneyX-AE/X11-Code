use anyhow::{anyhow, Result};
use std::{path::PathBuf, sync::Arc};
use tokio::{sync::Semaphore, task::JoinSet, time::{timeout, Duration}};
use x11_core::{SubagentRole, SubagentSpec};
use x11_model::ModelProvider;

use crate::{AgentConfig, AgentRuntime};

#[derive(Debug, Clone)]
pub struct SubagentResult {
    pub id: String,
    pub role: SubagentRole,
    pub success: bool,
    pub output: String,
    pub session_id: uuid::Uuid,
    pub iterations: u32,
}

#[derive(Debug, Clone)]
pub struct AgentManagerConfig {
    pub max_concurrency: usize,
    pub timeout_ms: u64,
}

impl Default for AgentManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            timeout_ms: 7_200_000,
        }
    }
}

pub struct AgentManager<P: ModelProvider + 'static> {
    provider: Arc<P>,
    workspace: PathBuf,
    base_config: AgentConfig,
    config: AgentManagerConfig,
}

impl<P: ModelProvider + 'static> AgentManager<P> {
    pub fn new(
        provider: Arc<P>,
        workspace: PathBuf,
        base_config: AgentConfig,
        config: AgentManagerConfig,
    ) -> Self {
        Self { provider, workspace, base_config, config }
    }

    pub async fn run_parallel(&self, specs: Vec<SubagentSpec>) -> Result<Vec<SubagentResult>> {
        if specs.is_empty() {
            return Ok(Vec::new());
        }
        let limit = self.config.max_concurrency.max(1).min(specs.len());
        let semaphore = Arc::new(Semaphore::new(limit));
        let mut jobs = JoinSet::new();

        for spec in specs {
            let permit = semaphore.clone().acquire_owned().await?;
            let provider = self.provider.clone();
            let workspace = self.workspace.clone();
            let mut cfg = self.base_config.clone();
            let timeout_ms = self.config.timeout_ms;
            jobs.spawn(async move {
                let _permit = permit;
                cfg.workspace = workspace;
                cfg.max_iterations = spec.max_iterations.max(1);
                cfg.auto_approve = false;
                cfg.session_path = None;

                let role_prompt = match spec.role {
                    SubagentRole::Explorer => "Explore the repository. Do not modify files.",
                    SubagentRole::Planner => "Create a concrete implementation and verification plan. Prefer read-only inspection.",
                    SubagentRole::Implementer => "Implement the requested work with narrow edits and verification.",
                    SubagentRole::Reviewer => "Review the repository state and identify correctness, security, and regression risks.",
                    SubagentRole::Tester => "Run targeted tests/checks and diagnose failures. Avoid unrelated edits.",
                };
                let goal = format!("Role: {role_prompt}\nTask: {}", spec.goal);
                let mut runtime = AgentRuntime::new_shared(goal, cfg, provider);
                let session_id = runtime.snapshot.session_id;
                let result = timeout(Duration::from_millis(timeout_ms.max(1)), runtime.run()).await;
                match result {
                    Ok(Ok(output)) => Ok(SubagentResult {
                        id: spec.id,
                        role: spec.role,
                        success: true,
                        output,
                        session_id,
                        iterations: runtime.snapshot.iteration,
                    }),
                    Ok(Err(err)) => Ok(SubagentResult {
                        id: spec.id,
                        role: spec.role,
                        success: false,
                        output: err.to_string(),
                        session_id,
                        iterations: runtime.snapshot.iteration,
                    }),
                    Err(_) => Ok(SubagentResult {
                        id: spec.id,
                        role: spec.role,
                        success: false,
                        output: format!("subagent timed out after {} ms", timeout_ms),
                        session_id,
                        iterations: runtime.snapshot.iteration,
                    }),
                }
            });
        }

        let mut results = Vec::with_capacity(specs.len());
        while let Some(joined) = jobs.join_next().await {
            results.push(joined.map_err(|e| anyhow!("subagent task join failure: {e}"))??);
        }
        results.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11_model::MockProvider;

    #[tokio::test]
    async fn manager_runs_isolated_agents_in_parallel() {
        let provider = Arc::new(MockProvider);
        let manager = AgentManager::new(
            provider,
            PathBuf::from("."),
            AgentConfig::default(),
            AgentManagerConfig { max_concurrency: 2, timeout_ms: 5_000 },
        );
        let specs = vec![
            SubagentSpec { id: "b".into(), role: SubagentRole::Explorer, goal: "inspect".into(), max_iterations: 1 },
            SubagentSpec { id: "a".into(), role: SubagentRole::Tester, goal: "test".into(), max_iterations: 1 },
        ];
        let results = manager.run_parallel(specs).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert_eq!(results[1].id, "b");
        assert!(results.iter().all(|r| r.success));
        assert_ne!(results[0].session_id, results[1].session_id);
    }
}
