use anyhow::{Context, Result};
use std::time::Duration;
use tokio::{process::Command, time::timeout};
use crate::verification::{VerificationPlan, VerificationResult};

#[derive(Debug, Clone)]
pub struct VerificationEngine {
    pub workspace: std::path::PathBuf,
    pub timeout: Duration,
}

impl VerificationEngine {
    pub fn new(workspace: impl Into<std::path::PathBuf>, timeout: Duration) -> Self {
        Self { workspace: workspace.into(), timeout }
    }

    pub async fn run(&self, plan: &VerificationPlan) -> Result<VerificationResult> {
        let mut passed = true;
        let mut steps_run = 0usize;
        let mut failures = Vec::new();
        for step in plan.required_steps() {
            let Some(command) = step.command.as_deref() else {
                if step.required {
                    passed = false;
                    failures.push(format!("{}: no command configured", step.description));
                }
                continue;
            };
            steps_run += 1;
            let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
            cmd.args(if cfg!(windows) { vec!["/C", command] } else { vec!["-lc", command] })
                .current_dir(&self.workspace);
            match timeout(self.timeout, cmd.output()).await {
                Ok(Ok(output)) if output.status.success() => {}
                Ok(Ok(output)) => {
                    passed = false;
                    failures.push(format!(
                        "{}: exit={} stderr={}",
                        step.description,
                        output.status.code().unwrap_or(-1),
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
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
            if !passed && step.required {
                break;
            }
        }
        Ok(VerificationResult {
            passed,
            summary: if failures.is_empty() { format!("verification passed: {steps_run} step(s)") } else { format!("verification failed: {}", failures.join(" | ")) },
            steps_run,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verification::{VerificationKind, VerificationStep};

    #[tokio::test]
    async fn successful_command_passes() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "true".into(), command: Some(if cfg!(windows) { "exit 0" } else { "true" }.into()), required: true });
        let result = VerificationEngine::new(std::env::current_dir().unwrap(), Duration::from_secs(2)).run(&plan).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.steps_run, 1);
    }

    #[tokio::test]
    async fn failing_required_command_stops_plan() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "fail".into(), command: Some(if cfg!(windows) { "exit 1" } else { "false" }.into()), required: true });
        plan.push(VerificationStep { kind: VerificationKind::Command, description: "should not run".into(), command: Some(if cfg!(windows) { "exit 0" } else { "true" }.into()), required: true });
        let result = VerificationEngine::new(std::env::current_dir().unwrap(), Duration::from_secs(2)).run(&plan).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.steps_run, 1);
        assert!(result.summary.contains("fail"));
    }
}
