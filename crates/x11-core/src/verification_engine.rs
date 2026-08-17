use anyhow::{Context, Result};
use std::{path::PathBuf, time::Duration};
use tokio::{process::Command, time::timeout};
use crate::verification::{VerificationPlan, VerificationResult};

const MAX_OUTPUT: usize = 32_000;
const MAX_COMMAND_LEN: usize = 16_000;

#[derive(Debug, Clone)]
pub struct VerificationEngine {
    pub workspace: PathBuf,
    pub timeout: Duration,
}

impl VerificationEngine {
    pub fn new(workspace: impl Into<PathBuf>, timeout: Duration) -> Self { Self { workspace: workspace.into(), timeout } }

    pub async fn run(&self, plan: &VerificationPlan) -> Result<VerificationResult> {
        plan.validate().context("invalid verification plan")?;
        if plan.is_empty() { return Ok(VerificationResult { passed: true, summary: "verification plan is empty".into(), steps_run: 0, failed_steps: Vec::new() }); }
        let mut passed = true;
        let mut steps_run = 0usize;
        let mut failures = Vec::new();
        for step in &plan.steps {
            let Some(command) = step.command.as_deref() else { continue; };
            let command = command.trim();
            if command.len() > MAX_COMMAND_LEN {
                passed = false;
                failures.push(format!("{}: command exceeds {} bytes", step.description, MAX_COMMAND_LEN));
                if step.required { break; }
                continue;
            }
            steps_run += 1;
            let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
            cmd.args(if cfg!(windows) { vec!["/C", command] } else { vec!["-lc", command] }).current_dir(&self.workspace);
            match timeout(self.timeout, cmd.output()).await {
                Ok(Ok(output)) if output.status.success() => {}
                Ok(Ok(output)) => {
                    passed = false;
                    let stderr = truncate(String::from_utf8_lossy(&output.stderr));
                    failures.push(format!("{}: exit={} stderr={stderr}", step.description, output.status.code().unwrap_or(-1)));
                }
                Ok(Err(error)) => {
                    passed = false;
                    failures.push(format!("{}: spawn failed: {error}", step.description));
                }
                Err(_) => {
                    passed = false;
                    failures.push(format!("{}: timed out", step.description));
                }
            }
            if !passed && step.required { break; }
        }
        Ok(VerificationResult {
            passed,
            summary: if failures.is_empty() { format!("verification passed: {steps_run} step(s)") } else { format!("verification failed: {}", failures.join(" | ")) },
            steps_run,
            failed_steps: failures,
        })
    }
}

fn truncate(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if text.len() > MAX_OUTPUT { text.truncate(MAX_OUTPUT); text.push_str("\n...[verification output truncated]..."); }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verification::{VerificationKind, VerificationStep};

    fn command_for(ok: bool) -> String { if cfg!(windows) { if ok { "exit 0" } else { "exit 1" } } else { if ok { "true" } else { "false" } }.into() }

    #[tokio::test]
    async fn successful_command_passes() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "true".into(), command: Some(command_for(true)), required: true });
        let result = VerificationEngine::new(std::env::current_dir().unwrap(), Duration::from_secs(2)).run(&plan).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.steps_run, 1);
    }

    #[tokio::test]
    async fn failing_required_command_stops_plan_but_optional_failure_continues() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "optional".into(), command: Some(command_for(false)), required: false });
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "required".into(), command: Some(command_for(true)), required: true });
        let result = VerificationEngine::new(std::env::current_dir().unwrap(), Duration::from_secs(2)).run(&plan).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.steps_run, 2);
        assert!(result.summary.contains("optional"));
    }

    #[tokio::test]
    async fn required_failure_stops_following_steps() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "fail".into(), command: Some(command_for(false)), required: true });
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "should not run".into(), command: Some(command_for(true)), required: true });
        let result = VerificationEngine::new(std::env::current_dir().unwrap(), Duration::from_secs(2)).run(&plan).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.steps_run, 1);
    }
}
