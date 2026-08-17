use crate::manager::SwarmReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict { Accept, RepairRequired }

#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub verdict: ReviewVerdict,
    pub reasons: Vec<String>,
}

pub struct SwarmReviewer;

impl SwarmReviewer {
    pub fn review(report: &SwarmReport) -> ReviewResult {
        let mut reasons = Vec::new();
        if report.failed > 0 {
            reasons.push(format!("{} subagent(s) failed", report.failed));
        }
        if !report.conflict_candidates.is_empty() {
            reasons.push(format!("{} file conflict candidate(s)", report.conflict_candidates.len()));
        }
        for result in &report.results {
            if !result.success {
                continue;
            }
            if !result.verification.contains("passed") {
                reasons.push(format!("subagent '{}' did not report successful verification", result.id));
            }
        }
        let verdict = if reasons.is_empty() { ReviewVerdict::Accept } else { ReviewVerdict::RepairRequired };
        ReviewResult { verdict, reasons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::{SubagentResult, SwarmReport};
    use x11_core::SubagentRole;

    fn result(id: &str, success: bool, verification: &str) -> SubagentResult {
        SubagentResult { id: id.into(), role: SubagentRole::Reviewer, success, output: String::new(), session_id: uuid::Uuid::new_v4(), iterations: 1, files_changed: Vec::new(), verification: verification.into() }
    }

    #[test]
    fn accepts_clean_swarm() {
        let report = SwarmReport { results: vec![result("a", true, "runtime verification passed")], succeeded: 1, failed: 0, conflict_candidates: Vec::new() };
        assert_eq!(SwarmReviewer::review(&report).verdict, ReviewVerdict::Accept);
    }

    #[test]
    fn rejects_conflicts_and_failures() {
        let report = SwarmReport { results: vec![result("a", false, "runtime verification failed")], succeeded: 0, failed: 1, conflict_candidates: vec!["src/lib.rs <- a, b".into()] };
        let review = SwarmReviewer::review(&report);
        assert_eq!(review.verdict, ReviewVerdict::RepairRequired);
        assert_eq!(review.reasons.len(), 2);
    }
}
