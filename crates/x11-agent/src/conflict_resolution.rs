use serde::{Deserialize, Serialize};

use crate::conflict_resolver::{ConflictReport, MergeDecision};

#[path = "resolution_apply.rs"]
pub mod apply;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictHunk {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub agent_ids: Vec<String>,
    pub before: String,
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionProposal {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub source_agents: Vec<String>,
    pub replacement: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionValidation {
    pub accepted: bool,
    pub reasons: Vec<String>,
}

pub struct ConflictResolutionGate;

impl ConflictResolutionGate {
    pub fn validate(report: &ConflictReport, proposal: &ResolutionProposal) -> ResolutionValidation {
        let mut reasons = Vec::new();
        if matches!(report.decision, MergeDecision::AutoMerge) { reasons.push("no conflict requires a model resolution".into()); }
        if !report.overlapping_files.iter().any(|p| p == &proposal.path) { reasons.push(format!("proposal path '{}' is not an overlapping file", proposal.path)); }
        if proposal.start_line == 0 || proposal.end_line < proposal.start_line { reasons.push("invalid proposal line range".into()); }
        if proposal.source_agents.is_empty() { reasons.push("proposal has no source agents".into()); }
        for agent in &proposal.source_agents { if !report.groups.iter().any(|group| group.iter().any(|id| id == agent)) { reasons.push(format!("unknown source agent '{agent}'")); } }
        if proposal.replacement.trim().is_empty() { reasons.push("empty replacement is not a valid resolution".into()); }
        ResolutionValidation { accepted: reasons.is_empty(), reasons }
    }

    pub fn build_prompt(hunk: &ConflictHunk) -> String {
        format!("Resolve exactly this coding conflict. Do not modify unrelated files.\nPath: {}\nLines: {}-{}\nAgents: {}\n\nCurrent content:\n```\n{}\n```\n\nCandidate alternatives:\n{}\n\nReturn a replacement for only the specified line range and a concise rationale.", hunk.path, hunk.start_line, hunk.end_line, hunk.agent_ids.join(", "), hunk.before, hunk.alternatives.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a)).collect::<Vec<_>>().join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict_resolver::{ConflictReport, MergeDecision};
    fn report() -> ConflictReport { ConflictReport { decision: MergeDecision::ResolveRequired, overlapping_files: vec!["src/lib.rs".into()], groups: vec![vec!["a".into(), "b".into()]] } }
    #[test] fn rejects_untrusted_proposal() { let proposal = ResolutionProposal { path: "src/main.rs".into(), start_line: 2, end_line: 3, source_agents: vec!["a".into()], replacement: "x".into(), rationale: "test".into() }; assert!(!ConflictResolutionGate::validate(&report(), &proposal).accepted); }
    #[test] fn accepts_scoped_proposal() { let proposal = ResolutionProposal { path: "src/lib.rs".into(), start_line: 10, end_line: 20, source_agents: vec!["a".into(), "b".into()], replacement: "merged".into(), rationale: "preserve both behaviours".into() }; assert!(ConflictResolutionGate::validate(&report(), &proposal).accepted); }
}
