use anyhow::Result;
use std::path::Path;

use crate::conflict_resolution::{ConflictHunk, ResolutionProposal};
use crate::conflict_resolver::ConflictReport;
use crate::resolution_orchestrator::{ResolutionConfig, ResolutionOrchestrator, ResolutionOutcome, ResolutionState};
use crate::resolution_provider::{ResolutionProvider, ResolutionRequest};

pub struct ResolutionRunner;

impl ResolutionRunner {
    pub async fn run<P, F, Fut>(workspace: &Path, report: &ConflictReport, hunk: &ConflictHunk, provider: &P, config: ResolutionConfig, verify: F) -> Result<ResolutionOutcome>
    where
        P: ResolutionProvider + ?Sized,
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<bool>>,
    {
        if config.max_attempts == 0 {
            return Ok(ResolutionOrchestrator::reject(0, vec!["resolution max_attempts must be greater than zero".into()]));
        }
        let mut attempts = 0;
        let mut reasons = Vec::new();
        while ResolutionOrchestrator::can_retry(attempts, config) {
            attempts += 1;
            let response = provider.propose(ResolutionRequest { conflict: hunk.clone() }).await?;
            let proposal: ResolutionProposal = match response.proposal {
                Some(proposal) => proposal,
                None => {
                    reasons.push(format!("attempt {attempts}: provider returned no structured proposal"));
                    continue;
                }
            };
            if let Err(error) = ResolutionOrchestrator::preview(workspace, report, &proposal) {
                reasons.push(format!("attempt {attempts}: proposal rejected: {error}"));
                continue;
            }
            let result = ResolutionOrchestrator::apply_and_verify(workspace, hunk, &proposal, &verify).await?;
            match result.state {
                ResolutionState::Verified => return Ok(ResolutionOutcome { attempts, ..result }),
                ResolutionState::RolledBack => reasons.extend(result.reasons),
                _ => reasons.push(format!("attempt {attempts}: unexpected resolution state {:?}", result.state)),
            }
        }
        Ok(ResolutionOrchestrator::rolled_back(attempts, reasons))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolution_provider::{ResolutionResponse, ResolutionProvider};
    use crate::conflict_resolver::MergeDecision;
    use std::{fs, path::PathBuf};

    struct EmptyProvider;
    #[async_trait::async_trait]
    impl ResolutionProvider for EmptyProvider {
        async fn propose(&self, _request: ResolutionRequest) -> Result<ResolutionResponse> {
            Ok(ResolutionResponse { proposal: None, raw_output: "bad".into(), provider: "test".into() })
        }
    }

    #[tokio::test]
    async fn empty_provider_is_bounded() {
        let workspace = std::env::temp_dir().join(format!("x11-resolution-runner-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&workspace).unwrap();
        let report = ConflictReport { decision: MergeDecision::ResolveRequired, overlapping_files: vec!["x.rs".into()], groups: vec![vec!["a".into(), "b".into()]] };
        let hunk = ConflictHunk { path: "x.rs".into(), start_line: 1, end_line: 1, agent_ids: vec!["a".into(), "b".into()], before: "x".into(), alternatives: vec!["y".into()] };
        let result = ResolutionRunner::run(&workspace, &report, &hunk, &EmptyProvider, ResolutionConfig { max_attempts: 2 }, || async { Ok(true) }).await.unwrap();
        assert_eq!(result.attempts, 2);
        assert_eq!(result.state, ResolutionState::RolledBack);
        let _ = fs::remove_dir_all(workspace);
    }
}
