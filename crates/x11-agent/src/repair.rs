use crate::manager::SwarmReport;
use crate::swarm_reviewer::{ReviewResult, ReviewVerdict};
use std::collections::BTreeSet;
use x11_core::{SubagentRole, SubagentSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairConfig {
    pub max_rounds: u32,
    pub priority: i32,
    pub token_budget: u32,
    pub tool_budget: u32,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self { max_rounds: 2, priority: 100, token_budget: 24_000, tool_budget: 96 }
    }
}

#[derive(Debug, Clone)]
pub struct RepairPlan {
    pub round: u32,
    pub tasks: Vec<SubagentSpec>,
    pub reasons: Vec<String>,
}

impl RepairPlan {
    pub fn empty(round: u32) -> Self {
        Self { round, tasks: Vec::new(), reasons: Vec::new() }
    }

    pub fn from_review(report: &SwarmReport, review: &ReviewResult, round: u32, config: RepairConfig) -> Self {
        if matches!(review.verdict, ReviewVerdict::Accept) || round >= config.max_rounds {
            return Self::empty(round);
        }

        let mut tasks = Vec::new();
        let mut reasons = review.reasons.clone();
        let mut seen = BTreeSet::new();

        for result in &report.results {
            if result.success { continue; }
            let id = format!("repair-{}-r{}", result.id, round);
            if !seen.insert(id.clone()) { continue; }
            tasks.push(SubagentSpec {
                id,
                role: SubagentRole::Implementer,
                goal: format!(
                    "Repair failed subagent '{}'. Failure evidence: {}. Verification evidence: {}. Changed files: {}",
                    result.id,
                    result.output,
                    result.verification,
                    if result.files_changed.is_empty() { "none".into() } else { result.files_changed.join(", ") },
                ),
                max_iterations: 6,
                model: "default".into(),
                token_budget: config.token_budget,
                tool_budget: config.tool_budget,
                allowed_tools: ["read_file", "search", "edit_file", "write_file", "shell", "git_status", "git_diff"]
                    .into_iter().map(String::from).collect(),
                dependencies: BTreeSet::new(),
                priority: config.priority,
                workspace_scope: None,
            });
        }

        for conflict in &report.conflict_candidates {
            let id = format!("repair-conflict-r{}-{}", round, tasks.len());
            if seen.insert(id.clone()) {
                tasks.push(SubagentSpec {
                    id,
                    role: SubagentRole::Reviewer,
                    goal: format!(
                        "Resolve workspace conflict candidate: {}. Inspect the current diff, preserve correct work, and leave the tree in a consistent state.",
                        conflict,
                    ),
                    max_iterations: 5,
                    model: "default".into(),
                    token_budget: config.token_budget,
                    tool_budget: config.tool_budget,
                    allowed_tools: ["read_file", "search", "git_status", "git_diff", "shell"]
                        .into_iter().map(String::from).collect(),
                    dependencies: BTreeSet::new(),
                    priority: config.priority,
                    workspace_scope: None,
                });
            }
        }

        if tasks.is_empty() && !review.reasons.is_empty() {
            tasks.push(SubagentSpec {
                id: format!("repair-general-r{}", round),
                role: SubagentRole::Implementer,
                goal: format!("Repair swarm according to review findings: {}", review.reasons.join("; ")),
                max_iterations: 6,
                model: "default".into(),
                token_budget: config.token_budget,
                tool_budget: config.tool_budget,
                allowed_tools: ["read_file", "search", "edit_file", "write_file", "shell", "git_status", "git_diff"]
                    .into_iter().map(String::from).collect(),
                dependencies: BTreeSet::new(),
                priority: config.priority,
                workspace_scope: None,
            });
        }

        reasons.push(format!("repair round {} created {} task(s)", round, tasks.len()));
        Self { round, tasks, reasons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::{SubagentResult, SwarmReport};
    use crate::swarm_reviewer::{ReviewResult, ReviewVerdict};

    #[test]
    fn failed_child_generates_bounded_repair_task() {
        let report = SwarmReport {
            results: vec![SubagentResult {
                id: "coder".into(), role: SubagentRole::Implementer, success: false,
                output: "test failed".into(), session_id: uuid::Uuid::new_v4(), iterations: 2,
                files_changed: vec!["src/lib.rs".into()], verification: "runtime verification failed".into(),
            }],
            succeeded: 0, failed: 1, conflict_candidates: Vec::new(),
        };
        let review = ReviewResult { verdict: ReviewVerdict::RepairRequired, reasons: vec!["failed".into()] };
        let plan = RepairPlan::from_review(&report, &review, 1, RepairConfig::default());
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].role, SubagentRole::Implementer);
    }

    #[test]
    fn max_round_prevents_unbounded_repair() {
        let report = SwarmReport { results: Vec::new(), succeeded: 0, failed: 1, conflict_candidates: vec!["src/lib.rs <- a, b".into()] };
        let review = ReviewResult { verdict: ReviewVerdict::RepairRequired, reasons: vec!["conflict".into()] };
        let plan = RepairPlan::from_review(&report, &review, 2, RepairConfig { max_rounds: 2, ..Default::default() });
        assert!(plan.tasks.is_empty());
    }
}
