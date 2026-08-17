use std::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};

#[path = "conflict_resolution.rs"]
pub mod resolution;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChange {
    pub agent_id: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MergeDecision { AutoMerge, ResolveRequired }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictReport {
    pub decision: MergeDecision,
    pub overlapping_files: Vec<String>,
    pub groups: Vec<Vec<String>>,
}

pub struct ConflictResolver;

impl ConflictResolver {
    pub fn analyze(changes: &[FileChange]) -> ConflictReport {
        let mut by_file: BTreeMap<String, Vec<&FileChange>> = BTreeMap::new();
        for change in changes { by_file.entry(change.path.clone()).or_default().push(change); }
        let mut overlapping_files = Vec::new();
        let mut groups = Vec::new();
        for (path, entries) in by_file {
            if entries.len() < 2 { continue; }
            let mut agents = BTreeSet::new();
            let mut overlap = false;
            for i in 0..entries.len() {
                agents.insert(entries[i].agent_id.clone());
                for j in (i + 1)..entries.len() {
                    agents.insert(entries[j].agent_id.clone());
                    if entries[i].start_line <= entries[j].end_line && entries[j].start_line <= entries[i].end_line { overlap = true; }
                }
            }
            if overlap {
                overlapping_files.push(path);
                groups.push(agents.into_iter().collect());
            }
        }
        let decision = if overlapping_files.is_empty() { MergeDecision::AutoMerge } else { MergeDecision::ResolveRequired };
        ConflictReport { decision, overlapping_files, groups }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_overlapping_changes_are_safe_to_merge() {
        let changes = vec![
            FileChange { agent_id: "a".into(), path: "src/lib.rs".into(), start_line: 10, end_line: 20 },
            FileChange { agent_id: "b".into(), path: "src/lib.rs".into(), start_line: 30, end_line: 40 },
        ];
        let report = ConflictResolver::analyze(&changes);
        assert_eq!(report.decision, MergeDecision::AutoMerge);
    }
    #[test]
    fn overlapping_changes_require_resolution() {
        let changes = vec![
            FileChange { agent_id: "a".into(), path: "src/lib.rs".into(), start_line: 10, end_line: 20 },
            FileChange { agent_id: "b".into(), path: "src/lib.rs".into(), start_line: 18, end_line: 26 },
        ];
        let report = ConflictResolver::analyze(&changes);
        assert_eq!(report.decision, MergeDecision::ResolveRequired);
        assert_eq!(report.overlapping_files, vec!["src/lib.rs"]);
        assert_eq!(report.groups, vec![vec!["a".to_string(), "b".to_string()]]);
    }
}
