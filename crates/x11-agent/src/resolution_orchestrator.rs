use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::conflict_resolution::{ConflictHunk, ConflictResolutionGate, ResolutionProposal};
use crate::conflict_resolver::{ConflictReport, MergeDecision};
use crate::resolution_apply::{ApplyPreview, ResolutionApplier};
use crate::resolution_transaction::{FileSnapshot, ResolutionTransaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionState { Proposed, Validated, Applied, Verified, RolledBack, Rejected }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolutionConfig { pub max_attempts: u32 }
impl Default for ResolutionConfig { fn default() -> Self { Self { max_attempts: 2 } } }

#[derive(Debug, Clone)]
pub struct ResolutionOutcome { pub state: ResolutionState, pub attempts: u32, pub path: Option<PathBuf>, pub reasons: Vec<String> }

pub struct ResolutionOrchestrator;

impl ResolutionOrchestrator {
    pub fn preview(workspace: &Path, report: &ConflictReport, proposal: &ResolutionProposal) -> Result<(ResolutionState, ApplyPreview)> {
        if matches!(report.decision, MergeDecision::AutoMerge) { bail!("resolution requested for a conflict-free report"); }
        let validation = ConflictResolutionGate::validate(report, proposal);
        if !validation.accepted { bail!("invalid resolution proposal: {}", validation.reasons.join("; ")); }
        let (_, preview) = ResolutionApplier::preview(workspace, report, proposal)?;
        let preview = preview.ok_or_else(|| anyhow::anyhow!("resolution preview rejected"))?;
        Ok((ResolutionState::Validated, preview))
    }

    pub async fn apply_once(workspace: &Path, hunk: &ConflictHunk, proposal: &ResolutionProposal) -> Result<ResolutionOutcome> {
        let path = ResolutionApplier::apply(workspace, hunk, proposal).await?;
        Ok(ResolutionOutcome { state: ResolutionState::Applied, attempts: 1, path: Some(path), reasons: Vec::new() })
    }

    pub async fn apply_and_verify<F, Fut>(workspace: &Path, hunk: &ConflictHunk, proposal: &ResolutionProposal, verify: F) -> Result<ResolutionOutcome>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<bool>>,
    {
        let snapshot: FileSnapshot = ResolutionTransaction::snapshot_file(workspace, Path::new(&hunk.path)).await?;
        let path = ResolutionApplier::apply(workspace, hunk, proposal).await?;
        match verify().await {
            Ok(true) => Ok(ResolutionOutcome { state: ResolutionState::Verified, attempts: 1, path: Some(path), reasons: Vec::new() }),
            Ok(false) => {
                ResolutionTransaction::rollback(&snapshot).await?;
                Ok(Self::rolled_back(1, vec!["verification failed; resolution rolled back".into()]))
            }
            Err(error) => {
                ResolutionTransaction::rollback(&snapshot).await?;
                Ok(Self::rolled_back(1, vec![format!("verification errored; resolution rolled back: {error}")]))
            }
        }
    }

    pub async fn verify_rollback_invariant(snapshot: &FileSnapshot) -> Result<bool> {
        ResolutionTransaction::verify_unchanged(snapshot).await
    }

    pub fn reject(attempts: u32, reasons: Vec<String>) -> ResolutionOutcome {
        ResolutionOutcome { state: ResolutionState::Rejected, attempts, path: None, reasons }
    }

    pub fn rolled_back(attempts: u32, reasons: Vec<String>) -> ResolutionOutcome {
        ResolutionOutcome { state: ResolutionState::RolledBack, attempts, path: None, reasons }
    }

    pub fn can_retry(attempts: u32, config: ResolutionConfig) -> bool { attempts < config.max_attempts.max(1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_is_bounded() {
        let cfg = ResolutionConfig { max_attempts: 2 };
        assert!(ResolutionOrchestrator::can_retry(0, cfg));
        assert!(ResolutionOrchestrator::can_retry(1, cfg));
        assert!(!ResolutionOrchestrator::can_retry(2, cfg));
    }
    #[test]
    fn rejection_and_rollback_are_terminal_states() {
        assert_eq!(ResolutionOrchestrator::reject(2, vec!["bad".into()]).state, ResolutionState::Rejected);
        assert_eq!(ResolutionOrchestrator::rolled_back(2, vec!["verification failed".into()]).state, ResolutionState::RolledBack);
    }
}
