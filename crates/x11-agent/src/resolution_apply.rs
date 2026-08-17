use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

use crate::conflict_resolution::{ConflictHunk, ResolutionProposal, ResolutionValidation, ConflictResolutionGate};
use crate::conflict_resolver::ConflictReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPreview {
    pub path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub replacement: String,
}

pub struct ResolutionApplier;

impl ResolutionApplier {
    pub fn preview(workspace: &Path, report: &ConflictReport, proposal: &ResolutionProposal) -> Result<(ResolutionValidation, Option<ApplyPreview>)> {
        let validation = ConflictResolutionGate::validate(report, proposal);
        if !validation.accepted {
            return Ok((validation, None));
        }
        let relative = Path::new(&proposal.path);
        if relative.is_absolute() || relative.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Ok((ResolutionValidation { accepted: false, reasons: vec!["proposal escapes workspace".into()] }, None));
        }
        let target = workspace.join(relative);
        Ok((validation, Some(ApplyPreview { path: target, start_line: proposal.start_line, end_line: proposal.end_line, replacement: proposal.replacement.clone() })))
    }

    pub async fn apply(workspace: &Path, hunk: &ConflictHunk, proposal: &ResolutionProposal) -> Result<PathBuf> {
        if proposal.path != hunk.path || proposal.start_line != hunk.start_line || proposal.end_line != hunk.end_line {
            anyhow::bail!("proposal does not match conflict hunk");
        }
        let relative = Path::new(&proposal.path);
        if relative.is_absolute() || relative.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            anyhow::bail!("proposal escapes workspace");
        }
        let path = workspace.join(relative);
        let canonical_parent = path.parent().context("proposal has no parent")?.canonicalize().context("canonicalize proposal parent")?;
        let workspace_root = workspace.canonicalize().context("canonicalize workspace")?;
        if !canonical_parent.starts_with(&workspace_root) {
            anyhow::bail!("proposal parent escapes workspace");
        }
        let content = tokio::fs::read_to_string(&path).await.context("read conflict target")?;
        let lines: Vec<&str> = content.lines().collect();
        if proposal.start_line == 0 || proposal.end_line > lines.len() || proposal.start_line > proposal.end_line {
            anyhow::bail!("proposal line range is outside target file");
        }
        let mut out = Vec::new();
        out.extend(lines[..proposal.start_line - 1].iter().copied());
        out.extend(proposal.replacement.lines());
        out.extend(lines[proposal.end_line..].iter().copied());
        let mut updated = out.join("\n");
        if content.ends_with('\n') { updated.push('\n'); }
        tokio::fs::write(&path, updated).await.context("write resolved conflict")?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict_resolver::{ConflictReport, MergeDecision};
    use tempfile::tempdir;

    #[tokio::test]
    async fn applies_only_inside_workspace() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("x.rs");
        tokio::fs::write(&path, "a\nb\nc\n").await.unwrap();
        let report = ConflictReport { decision: MergeDecision::ResolveRequired, overlapping_files: vec!["x.rs".into()], groups: vec![vec!["a".into(), "b".into()]] };
        let hunk = ConflictHunk { path: "x.rs".into(), start_line: 2, end_line: 2, agent_ids: vec!["a".into(), "b".into()], before: "b".into(), alternatives: vec!["B".into()] };
        let proposal = ResolutionProposal { path: "x.rs".into(), start_line: 2, end_line: 2, source_agents: vec!["a".into(), "b".into()], replacement: "B".into(), rationale: "merge".into() };
        let result = ResolutionApplier::apply(dir.path(), &hunk, &proposal).await.unwrap();
        assert_eq!(result, path);
        assert_eq!(tokio::fs::read_to_string(path).await.unwrap(), "a\nB\nc\n");
        let _ = report;
    }
}
