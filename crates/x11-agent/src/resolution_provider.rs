use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use x11_model::{CompletionRequest, Message, ModelProvider};

use crate::conflict_resolution::{ConflictHunk, ConflictResolutionGate, ResolutionProposal};
use crate::conflict_resolver::ConflictReport;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionRequest { pub conflict: ConflictHunk }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionResponse { pub proposal: Option<ResolutionProposal>, pub raw_output: String, pub provider: String }

#[async_trait]
pub trait ResolutionProvider: Send + Sync { async fn propose(&self, request: ResolutionRequest) -> Result<ResolutionResponse>; }

pub struct ModelResolutionProvider<P> { pub provider: P, pub model: String }
impl<P> ModelResolutionProvider<P> { pub fn new(provider: P, model: impl Into<String>) -> Self { Self { provider, model: model.into() } } }

#[async_trait]
impl<P: ModelProvider + Send + Sync> ResolutionProvider for ModelResolutionProvider<P> {
    async fn propose(&self, request: ResolutionRequest) -> Result<ResolutionResponse> {
        let prompt = format!("{}\nReturn ONLY a JSON object with keys: path,start_line,end_line,source_agents,replacement,rationale.", ConflictResolutionGate::build_prompt(&request.conflict));
        let completion = self.provider.complete(CompletionRequest { model: self.model.clone(), messages: vec![Message::user(prompt)], tools: Vec::new(), temperature: Some(0.0), max_tokens: Some(4096) }).await?;
        let raw_output = completion.text;
        let proposal = parse_proposal(&raw_output).ok();
        Ok(ResolutionResponse { proposal, raw_output, provider: self.provider.name().into() })
    }
}

pub fn parse_proposal(output: &str) -> Result<ResolutionProposal> {
    let trimmed = output.trim();
    let candidate = trimmed.strip_prefix("```").and_then(|v| v.strip_suffix("```")).map(str::trim).unwrap_or(trimmed);
    let value: serde_json::Value = serde_json::from_str(candidate).context("resolution output is not valid JSON")?;
    let obj = value.as_object().context("resolution output must be a JSON object")?;
    let allowed = ["path", "start_line", "end_line", "source_agents", "replacement", "rationale"];
    if obj.keys().any(|key| !allowed.contains(&key.as_str())) { anyhow::bail!("resolution output contains unknown fields"); }
    let proposal: ResolutionProposal = serde_json::from_value(value).context("resolution JSON does not match proposal schema")?;
    if proposal.path.is_empty() || proposal.start_line == 0 || proposal.end_line < proposal.start_line { anyhow::bail!("resolution proposal has invalid path or line range"); }
    if proposal.source_agents.is_empty() || proposal.replacement.trim().is_empty() { anyhow::bail!("resolution proposal is incomplete"); }
    Ok(proposal)
}

pub fn proposal_is_scoped(report: &ConflictReport, proposal: &ResolutionProposal) -> bool { ConflictResolutionGate::validate(report, proposal).accepted }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict_resolver::{ConflictReport, MergeDecision};
    #[test]
    fn parses_strict_proposal_json() {
        let proposal = parse_proposal(r#"{"path":"src/lib.rs","start_line":10,"end_line":12,"source_agents":["a","b"],"replacement":"merged","rationale":"preserve both"}"#).unwrap();
        assert_eq!(proposal.path, "src/lib.rs");
        assert_eq!(proposal.source_agents.len(), 2);
    }
    #[test]
    fn rejects_unknown_fields_and_freeform_text() {
        assert!(parse_proposal("I fixed it").is_err());
        assert!(parse_proposal(r#"{"path":"src/lib.rs","start_line":1,"end_line":1,"source_agents":["a"],"replacement":"x","rationale":"r","extra":1}"#).is_err());
    }
    #[test]
    fn scoped_proposal_uses_existing_gate() {
        let report = ConflictReport { decision: MergeDecision::ResolveRequired, overlapping_files: vec!["src/lib.rs".into()], groups: vec![vec!["a".into(), "b".into()]] };
        let proposal = ResolutionProposal { path: "src/lib.rs".into(), start_line: 10, end_line: 12, source_agents: vec!["a".into(), "b".into()], replacement: "merged".into(), rationale: "preserve both changes".into() };
        assert!(proposal_is_scoped(&report, &proposal));
    }
}
