use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use x11_model::{CompletionRequest, ModelProvider};

use crate::conflict_resolution::{ConflictHunk, ConflictResolutionGate, ResolutionProposal};
use crate::conflict_resolver::ConflictReport;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionRequest {
    pub conflict: ConflictHunk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionResponse {
    pub proposal: Option<ResolutionProposal>,
    pub raw_output: String,
    pub provider: String,
}

#[async_trait]
pub trait ResolutionProvider: Send + Sync {
    async fn propose(&self, request: ResolutionRequest) -> Result<ResolutionResponse>;
}

pub struct ModelResolutionProvider<P> {
    pub provider: P,
    pub model: String,
}

impl<P> ModelResolutionProvider<P> {
    pub fn new(provider: P, model: impl Into<String>) -> Self {
        Self { provider, model: model.into() }
    }
}

#[async_trait]
impl<P: ModelProvider + Send + Sync> ResolutionProvider for ModelResolutionProvider<P> {
    async fn propose(&self, request: ResolutionRequest) -> Result<ResolutionResponse> {
        let prompt = ConflictResolutionGate::build_prompt(&request.conflict);
        let completion = self.provider.complete(CompletionRequest {
            model: self.model.clone(),
            messages: vec![x11_model::Message::user(prompt)],
            tools: Vec::new(),
            temperature: Some(0.0),
            max_tokens: Some(4096),
        }).await?;

        // The provider is intentionally proposal-only. Structured parsing is kept separate
        // so malformed model output can never become an executable patch implicitly.
        Ok(ResolutionResponse {
            proposal: None,
            raw_output: completion.text,
            provider: "model".into(),
        })
    }
}

pub fn proposal_is_scoped(report: &ConflictReport, proposal: &ResolutionProposal) -> bool {
    ConflictResolutionGate::validate(report, proposal).accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict_resolver::{ConflictReport, MergeDecision};

    #[test]
    fn scoped_proposal_uses_existing_gate() {
        let report = ConflictReport {
            decision: MergeDecision::ResolveRequired,
            overlapping_files: vec!["src/lib.rs".into()],
            groups: vec![vec!["a".into(), "b".into()]],
        };
        let proposal = ResolutionProposal {
            path: "src/lib.rs".into(),
            start_line: 10,
            end_line: 12,
            source_agents: vec!["a".into(), "b".into()],
            replacement: "merged".into(),
            rationale: "preserve both changes".into(),
        };
        assert!(proposal_is_scoped(&report, &proposal));
    }
}
