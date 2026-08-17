use anyhow::{anyhow, Result};
use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}, sync::Arc};
use tokio::{sync::{watch, Semaphore}, task::JoinSet, time::{timeout, Duration}};
use uuid::Uuid;

use x11_core::{AgentState, SubagentRole, SubagentSpec};
use x11_model::ModelProvider;
use x11_permissions::Policy;
use x11_protocol::AgentEvent;

use crate::{
    swarm_event_bus::SwarmEventBus,
    swarm_events::{SwarmEvent, SwarmEventKind},
    swarm_state::{ResultSnapshot, SwarmState, SwarmTaskStatus},
    workspace_lock::WorkspaceLockManager,
    AgentConfig, AgentRuntime,
};
use crate::tool_executor::ToolExecutor;

#[derive(Debug, Clone)]
pub struct SubagentResult {
    pub id: String,
    pub role: SubagentRole,
    pub success: bool,
    pub output: String,
    pub session_id: Uuid,
    pub iterations: u32,
    pub files_changed: Vec<String>,
    pub verification: String,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
pub struct SwarmReport {
    pub swarm_id: Uuid,
    pub results: Vec<SubagentResult>,
    pub succeeded: usize,
    pub failed: usize,
    pub conflict_candidates: Vec<String>,
}

impl SwarmReport {
    pub fn all_success(&self) -> bool { self.failed == 0 }

    pub fn summary(&self) -> String {
        format!(
            "swarm complete: {} succeeded, {} failed, {} conflict candidate(s)",
            self.succeeded,
            self.failed,
            self.conflict_candidates.len()
        )
    }
}

#[derive(Debug, Clone)]
pub struct AgentManagerConfig {
    pub max_concurrency: usize,
    pub timeout_ms: u64,
    pub inherited_policy: Option<Policy>,
    pub cancellation: Option<watch::Sender<bool>>,
    pub state_path: Option<PathBuf>,
    pub event_bus: Option<SwarmEventBus>,
    pub swarm_id: Option<Uuid>,
}

impl Default for AgentManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            timeout_ms: 7_200_000,
            inherited_policy: None,
            cancellation: None,
            state_path: None,
            event_bus: None,
            swarm_id: None,
        }
    }
}

pub struct AgentManager<P: ModelProvider + 'static> {
    provider: Arc<P>,
    workspace: PathBuf,
    base_config: AgentConfig,
    config: AgentManagerConfig,
    locks: WorkspaceLockManager,
}

impl<P: ModelProvider + 'static> AgentManager<P> {
    pub fn new(provider: Arc<P>, workspace: PathBuf, base_config: AgentConfig, config: AgentManagerConfig) -> Self {
        Self { provider, workspace, base_config, config, locks: WorkspaceLockManager::default() }
    }

    fn emit(&self, event: SwarmEvent) {
        if let Some(bus) = &self.config.event_bus { bus.emit(event); }
    }

    fn swarm_id(&self, fallback: Uuid) -> Uuid { self.config.swarm_id.unwrap_or(fallback) }

    fn cancelled(&self) -> bool {
        self.config.cancellation.as_ref().map(|tx| *tx.borrow()).unwrap_or(false)
    }

    async fn run_batch(&self, specs: Vec<SubagentSpec>) -> Result<Vec<SubagentResult>> {
        if specs.is_empty() { return Ok(Vec::new()); }
        let limit = self.config.max_concurrency.max(1).min(specs.len());
        let semaphore = Arc::new(Semaphore::new(limit));
        let mut jobs = JoinSet::new();

        for spec in specs {
            let permit = semaphore.clone().acquire_owned().await?;
            let provider = self.provider.clone();
            let workspace = self.workspace.clone();
            let mut cfg = self.base_config.clone();
            let timeout_ms = self.config.timeout_ms.max(1);
            let locks = self.locks.clone();
            let inherited_policy = self.config.inherited_policy.clone();
            let parent_cancel = self.config.cancellation.clone();
            let bus = self.config.event_bus.clone();
            let swarm_id = self.config.swarm_id.unwrap_or(Uuid::nil());

            jobs.spawn(async move {
                let _permit = permit;
                if let Some(bus) = &bus {
                    bus.emit(SwarmEvent::new(swarm_id, SwarmEventKind::TaskStarted).task(spec.id.clone()).state("running").progress(1));
                }

                cfg.max_iterations = spec.max_iterations.max(1);
                cfg.model = spec.model.clone();
                cfg.max_context_tokens = (spec.token_budget as usize).clamp(2_000, 128_000);
                cfg.auto_approve = false;
                cfg.session_path = None;

                let root = std::fs::canonicalize(&workspace)
                    .map_err(|e| anyhow!("failed to canonicalize workspace {}: {e}", workspace.display()))?;
                cfg.workspace = match &spec.workspace_scope {
                    None => root,
                    Some(scope) => {
                        let rel = Path::new(scope);
                        if rel.is_absolute() || rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                            return Err(anyhow!("invalid workspace_scope for {}", spec.id));
                        }
                        let candidate = root.join(rel);
                        let canonical = std::fs::canonicalize(&candidate)
                            .map_err(|e| anyhow!("workspace_scope does not exist for {}: {} ({e})", spec.id, candidate.display()))?;
                        if !canonical.starts_with(&root) || canonical == root {
                            return Err(anyhow!("workspace_scope escapes parent workspace for {}", spec.id));
                        }
                        canonical
                    }
                };

                let role_prompt = match spec.role {
                    SubagentRole::Explorer => "Explore the repository. Do not modify files.",
                    SubagentRole::Planner => "Create a concrete implementation and verification plan. Prefer read-only inspection.",
                    SubagentRole::Implementer => "Implement the requested work with narrow edits and verification.",
                    SubagentRole::Reviewer => "Review the repository state and identify correctness, regressions, and safety risks.",
                    SubagentRole::Tester => "Run targeted tests/checks and diagnose failures. Avoid unrelated edits.",
                };
                let preferred_tools = if spec.allowed_tools.is_empty() { "all registered tools".to_string() } else { spec.allowed_tools.iter().cloned().collect::<Vec<_>>().join(", ") };
                let goal = format!("Role: {role_prompt}\nTask: {}\nTool budget: {} calls. Preferred tools: {preferred_tools}", spec.goal, spec.tool_budget);

                let mut runtime = AgentRuntime::new_shared(goal, cfg, provider);
                let session_id = runtime.snapshot.session_id;
                if let Some(policy) = inherited_policy { runtime.policy = policy; }
                if let Some(cancel) = parent_cancel { runtime.cancel = cancel; }
                runtime.executor = if spec.allowed_tools.is_empty() {
                    ToolExecutor::with_locks(runtime.tools.clone(), runtime.config.workspace.clone(), locks)
                } else {
                    ToolExecutor::with_locks_and_allowlist(runtime.tools.clone(), runtime.config.workspace.clone(), locks, spec.allowed_tools.clone())
                };

                let run_result = timeout(Duration::from_millis(timeout_ms), runtime.run()).await;
                let cancelled = runtime.snapshot.state == AgentState::Cancelled;
                let files_changed = runtime.session.events.iter().filter_map(|event| match event {
                    AgentEvent::ToolRequested { tool, input, .. } if tool == "write_file" || tool == "edit_file" => input["path"].as_str().map(str::to_owned),
                    _ => None,
                }).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();

                let (success, output, verification) = match run_result {
                    Ok(Ok(output)) if !cancelled => (true, output, "runtime verification passed".to_string()),
                    Ok(Ok(output)) => (false, output, "cancelled".to_string()),
                    Ok(Err(error)) => (false, error.to_string(), "runtime verification failed".to_string()),
                    Err(_) => (false, format!("subagent timed out after {timeout_ms} ms"), "timed out".to_string()),
                };

                if let Some(bus) = &bus {
                    bus.emit(SwarmEvent::new(swarm_id, SwarmEventKind::VerificationStarted).task(spec.id.clone()).agent(spec.id.clone()).state("verifying"));
                    let verification_kind = if success { SwarmEventKind::VerificationPassed } else { SwarmEventKind::VerificationFailed };
                    bus.emit(SwarmEvent::new(swarm_id, verification_kind).task(spec.id.clone()).agent(spec.id.clone()).progress(100).state(if cancelled { "cancelled" } else if success { "passed" } else { "failed" }).evidence(verification.clone()));
                    let task_kind = if success { SwarmEventKind::TaskCompleted } else if cancelled { SwarmEventKind::TaskCancelled } else { SwarmEventKind::TaskFailed };
                    bus.emit(SwarmEvent::new(swarm_id, task_kind).task(spec.id.clone()).agent(spec.id.clone()).progress(100).state(if cancelled { "cancelled" } else if success { "completed" } else { "failed" }).evidence(format!("files_changed={}", files_changed.len())));
                }

                Ok(SubagentResult { id: spec.id, role: spec.role, success, output, session_id, iterations: runtime.snapshot.iteration, files_changed, verification, cancelled })
            });
        }

        let mut results = Vec::new();
        while let Some(joined) = jobs.join_next().await {
            results.push(joined.map_err(|e| anyhow!("subagent task join failure: {e}"))??);
        }
        results.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(results)
    }

    pub async fn run_parallel(&self, specs: Vec<SubagentSpec>) -> Result<Vec<SubagentResult>> { Ok(self.run_report(specs).await?.results) }

    pub async fn run_report(&self, specs: Vec<SubagentSpec>) -> Result<SwarmReport> { self.run_report_internal(specs, self.config.state_path.clone()).await }

    pub async fn run_report_resumable(&self, specs: Vec<SubagentSpec>, state_path: impl Into<PathBuf>) -> Result<SwarmReport> { self.run_report_internal(specs, Some(state_path.into())).await }

    async fn run_report_internal(&self, specs: Vec<SubagentSpec>, state_path: Option<PathBuf>) -> Result<SwarmReport> {
        let task_ids = specs.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
        let mut state = if let Some(path) = &state_path {
            match SwarmState::load(path).await {
                Ok(mut loaded) => {
                    for task in loaded.tasks.values_mut() {
                        if matches!(task.status, SwarmTaskStatus::Running) { task.status = SwarmTaskStatus::Pending; }
                    }
                    self.emit(SwarmEvent::new(loaded.swarm_id, SwarmEventKind::SwarmResumed).state("resumed").progress(0));
                    loaded
                }
                Err(_) => SwarmState::new("swarm", task_ids.clone()),
            }
        } else { SwarmState::new("swarm", task_ids.clone()) };
        let swarm_id = self.swarm_id(state.swarm_id);
        state.swarm_id = swarm_id;
        self.emit(SwarmEvent::new(swarm_id, SwarmEventKind::SwarmStarted).state("running").progress(0));

        let known = task_ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        for id in state.tasks.keys() {
            if !known.contains(id.as_str()) { anyhow::bail!("persisted swarm task '{id}' is absent from supplied specs"); }
        }

        let mut completed = state.tasks.iter().filter(|(_, task)| matches!(task.status, SwarmTaskStatus::Succeeded)).map(|(id, _)| id.clone()).collect::<BTreeSet<_>>();
        let mut pending = specs.into_iter().filter(|spec| !completed.contains(&spec.id)).map(|spec| (spec.id.clone(), spec)).collect::<BTreeMap<_, _>>();
        if let Some(path) = &state_path { state.save_atomic(path).await?; }

        let mut results = Vec::new();
        while !pending.is_empty() {
            if self.cancelled() {
                for id in pending.keys() {
                    let _ = state.mark_finished(id, ResultSnapshot { success: false, cancelled: true, output: Some("cancelled before scheduling".into()), error: Some("parent swarm cancelled".into()), files_changed: Vec::new() });
                    self.emit(SwarmEvent::new(swarm_id, SwarmEventKind::TaskCancelled).task(id.clone()).state("cancelled"));
                }
                if let Some(path) = &state_path { state.save_atomic(path).await?; }
                break;
            }

            let mut ready = pending.values().filter(|spec| spec.dependencies.iter().all(|dependency| completed.contains(dependency))).cloned().collect::<Vec<_>>();
            if ready.is_empty() {
                let unresolved = pending.keys().cloned().collect::<Vec<_>>().join(", ");
                for id in pending.keys() {
                    let _ = state.mark_finished(id, ResultSnapshot { success: false, cancelled: false, output: None, error: Some(format!("blocked by unresolved dependency graph: {unresolved}")), files_changed: Vec::new() });
                    self.emit(SwarmEvent::new(swarm_id, SwarmEventKind::TaskBlocked).task(id.clone()).state("blocked").evidence(unresolved.clone()));
                }
                if let Some(path) = &state_path { state.save_atomic(path).await?; }
                break;
            }

            ready.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
            for spec in &ready {
                self.emit(SwarmEvent::new(swarm_id, SwarmEventKind::TaskQueued).task(spec.id.clone()).parent_task("root").state("queued").progress(0));
                if let Some(path) = &state_path { state.mark_running(&spec.id, Uuid::nil())?; state.save_atomic(path).await?; }
            }

            let batch = self.run_batch(ready).await?;
            for result in &batch {
                if result.success { completed.insert(result.id.clone()); }
                pending.remove(&result.id);
                if let Some(path) = &state_path {
                    state.mark_finished(&result.id, ResultSnapshot { success: result.success, cancelled: result.cancelled, output: Some(result.output.clone()), error: (!result.success).then(|| result.output.clone()), files_changed: result.files_changed.clone() })?;
                    state.save_atomic(path).await?;
                }
            }
            results.extend(batch);
        }

        results.sort_by(|a, b| a.id.cmp(&b.id));
        let succeeded = state.tasks.values().filter(|task| matches!(task.status, SwarmTaskStatus::Succeeded)).count();
        let failed = state.tasks.values().filter(|task| matches!(task.status, SwarmTaskStatus::Failed | SwarmTaskStatus::Cancelled)).count();

        let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for result in &results { for path in &result.files_changed { paths.entry(path.clone()).or_default().push(result.id.clone()); } }
        let conflict_candidates = paths.into_iter().filter_map(|(path, mut owners)| { owners.sort(); owners.dedup(); (owners.len() > 1).then(|| format!("{path} <- {}", owners.join(", "))) }).collect::<Vec<_>>();
        for conflict in &conflict_candidates { self.emit(SwarmEvent::new(swarm_id, SwarmEventKind::ConflictDetected).state("conflict").evidence(conflict.clone())); }
        self.emit(SwarmEvent::new(swarm_id, SwarmEventKind::SwarmCompleted).state(if failed == 0 { "completed" } else { "completed_with_failures" }).progress(100).evidence(format!("succeeded={succeeded},failed={failed}")));

        Ok(SwarmReport { swarm_id, results, succeeded, failed, conflict_candidates })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11_model::MockProvider;

    fn spec(id: &str, role: SubagentRole, priority: i32, deps: &[&str]) -> SubagentSpec {
        SubagentSpec { id: id.into(), role, goal: id.into(), max_iterations: 1, model: "default".into(), token_budget: 4_000, tool_budget: 8, allowed_tools: BTreeSet::new(), dependencies: deps.iter().map(|d| d.to_string()).collect(), priority, workspace_scope: None }
    }

    #[tokio::test]
    async fn manager_runs_isolated_agents_in_parallel() {
        let manager = AgentManager::new(Arc::new(MockProvider), PathBuf::from("."), AgentConfig::default(), AgentManagerConfig { max_concurrency: 2, ..Default::default() });
        let results = manager.run_parallel(vec![spec("b", SubagentRole::Explorer, 0, &[]), spec("a", SubagentRole::Tester, 0, &[])]).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert_eq!(results[1].id, "b");
        assert!(results.iter().all(|r| r.success));
        assert!(results.iter().all(|r| !r.cancelled));
        assert_ne!(results[0].session_id, results[1].session_id);
    }

    #[tokio::test]
    async fn dependencies_are_scheduled_in_batches() {
        let manager = AgentManager::new(Arc::new(MockProvider), PathBuf::from("."), AgentConfig::default(), AgentManagerConfig { max_concurrency: 2, ..Default::default() });
        let report = manager.run_report(vec![spec("review", SubagentRole::Reviewer, 0, &["impl"]), spec("impl", SubagentRole::Implementer, 1, &[])]).await.unwrap();
        assert_eq!(report.results.len(), 2);
        assert_eq!(report.failed, 0);
    }

    #[tokio::test]
    async fn missing_dependency_is_reported_as_failure() {
        let manager = AgentManager::new(Arc::new(MockProvider), PathBuf::from("."), AgentConfig::default(), AgentManagerConfig::default());
        let report = manager.run_report(vec![spec("a", SubagentRole::Explorer, 0, &["missing"])]).await.unwrap();
        assert_eq!(report.results.len(), 0);
        assert_eq!(report.succeeded, 0);
        assert_eq!(report.failed, 1);
    }

    #[tokio::test]
    async fn persisted_successes_are_skipped_on_resume() {
        let dir = std::env::temp_dir().join(format!("x11-swarm-resume-{}", Uuid::new_v4()));
        let path = dir.join("state.json");
        let mut state = SwarmState::new("swarm", vec!["a".into(), "b".into()]);
        state.mark_finished("a", ResultSnapshot { success: true, cancelled: false, output: Some("done".into()), error: None, files_changed: Vec::new() }).unwrap();
        state.save_atomic(&path).await.unwrap();
        let manager = AgentManager::new(Arc::new(MockProvider), PathBuf::from("."), AgentConfig::default(), AgentManagerConfig::default());
        let report = manager.run_report_resumable(vec![spec("a", SubagentRole::Explorer, 0, &[]), spec("b", SubagentRole::Explorer, 0, &[])], &path).await.unwrap();
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 0);
        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn inherited_cancellation_marks_children_cancelled() {
        let (tx, _) = watch::channel(true);
        let manager = AgentManager::new(Arc::new(MockProvider), PathBuf::from("."), AgentConfig::default(), AgentManagerConfig { cancellation: Some(tx), ..Default::default() });
        let report = manager.run_report(vec![spec("a", SubagentRole::Explorer, 0, &[])]).await.unwrap();
        assert_eq!(report.succeeded, 0);
        assert_eq!(report.failed, 1);
    }

    #[tokio::test]
    async fn absolute_workspace_scope_is_rejected() {
        let manager = AgentManager::new(Arc::new(MockProvider), PathBuf::from("."), AgentConfig::default(), AgentManagerConfig::default());
        let mut bad = spec("bad", SubagentRole::Explorer, 0, &[]);
        bad.workspace_scope = Some(std::env::current_dir().unwrap().display().to_string());
        let report = manager.run_report(vec![bad]).await.unwrap();
        assert_eq!(report.succeeded, 0);
        assert_eq!(report.failed, 1);
    }

    #[test]
    fn report_aggregates_counts() {
        let report = SwarmReport { swarm_id: Uuid::nil(), results: Vec::new(), succeeded: 0, failed: 0, conflict_candidates: Vec::new() };
        assert!(report.all_success());
        assert_eq!(report.summary(), "swarm complete: 0 succeeded, 0 failed, 0 conflict candidate(s)");
    }

    #[tokio::test]
    async fn emits_lifecycle_events() {
        let bus = SwarmEventBus::new(64);
        let mut rx = bus.subscribe();
        let id = Uuid::new_v4();
        let manager = AgentManager::new(Arc::new(MockProvider), PathBuf::from("."), AgentConfig::default(), AgentManagerConfig { event_bus: Some(bus.clone()), swarm_id: Some(id), ..Default::default() });
        let _ = manager.run_report(vec![spec("a", SubagentRole::Explorer, 0, &[])]).await.unwrap();
        let mut kinds = Vec::new();
        while let Ok(event) = rx.try_recv() { kinds.push(event.kind); }
        assert!(kinds.contains(&SwarmEventKind::SwarmStarted));
        assert!(kinds.contains(&SwarmEventKind::TaskQueued));
        assert!(kinds.contains(&SwarmEventKind::TaskStarted));
        assert!(kinds.contains(&SwarmEventKind::TaskCompleted));
        assert!(kinds.contains(&SwarmEventKind::SwarmCompleted));
    }
}
