use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::manager::{SubagentResult, SwarmReport};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmReportSnapshot {
    pub swarm_id: Uuid,
    pub succeeded: usize,
    pub failed: usize,
    pub conflict_candidates: Vec<String>,
    pub agents: Vec<AgentReportSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReportSnapshot {
    pub id: String,
    pub role: String,
    pub success: bool,
    pub cancelled: bool,
    pub session_id: String,
    pub iterations: u32,
    pub files_changed: Vec<String>,
    pub verification: String,
    pub output: String,
}

impl From<&SwarmReport> for SwarmReportSnapshot {
    fn from(report: &SwarmReport) -> Self {
        Self {
            swarm_id: report.swarm_id,
            succeeded: report.succeeded,
            failed: report.failed,
            conflict_candidates: report.conflict_candidates.clone(),
            agents: report.results.iter().map(AgentReportSnapshot::from).collect(),
        }
    }
}

impl From<&SubagentResult> for AgentReportSnapshot {
    fn from(result: &SubagentResult) -> Self {
        Self {
            id: result.id.clone(),
            role: format!("{:?}", result.role),
            success: result.success,
            cancelled: result.cancelled,
            session_id: result.session_id.to_string(),
            iterations: result.iterations,
            files_changed: result.files_changed.clone(),
            verification: result.verification.clone(),
            output: result.output.clone(),
        }
    }
}

impl SwarmReportSnapshot {
    pub fn to_pretty_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips_as_json() {
        let swarm_id = Uuid::new_v4();
        let report = SwarmReport {
            swarm_id,
            results: Vec::new(),
            succeeded: 0,
            failed: 0,
            conflict_candidates: vec!["src/lib.rs <- a, b".into()],
        };
        let snap = SwarmReportSnapshot::from(&report);
        let json = snap.to_pretty_json().unwrap();
        let parsed: SwarmReportSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.swarm_id, swarm_id);
        assert_eq!(parsed.conflict_candidates, snap.conflict_candidates);
    }
}
