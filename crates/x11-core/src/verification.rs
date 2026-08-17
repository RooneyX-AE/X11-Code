use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationKind {
    Command,
    Test,
    Build,
    GitDiff,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStep {
    pub kind: VerificationKind,
    pub description: String,
    pub command: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub passed: bool,
    pub summary: String,
    pub steps_run: usize,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct VerificationPlan {
    pub steps: Vec<VerificationStep>,
}

impl VerificationPlan {
    pub fn push(&mut self, step: VerificationStep) { self.steps.push(step); }
    pub fn required_steps(&self) -> impl Iterator<Item = &VerificationStep> { self.steps.iter().filter(|s| s.required) }
    pub fn is_empty(&self) -> bool { self.steps.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_steps_are_filtered() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Build, description: "build".into(), command: Some("cargo check".into()), required: true });
        plan.push(VerificationStep { kind: VerificationKind::GitDiff, description: "inspect diff".into(), command: None, required: false });
        assert_eq!(plan.required_steps().count(), 1);
    }
}
