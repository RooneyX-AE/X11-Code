use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationKind { Command, Test, Build, GitDiff, Custom }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStep {
    pub kind: VerificationKind,
    pub description: String,
    pub command: Option<String>,
    #[serde(default)] pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationResult {
    pub passed: bool,
    pub summary: String,
    pub steps_run: usize,
    #[serde(default)] pub failed_steps: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct VerificationPlan { pub steps: Vec<VerificationStep> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationPlanError { EmptyDescription, MissingCommand(String), EmptyCommand(String), DuplicateDescription(String) }
impl std::fmt::Display for VerificationPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDescription => write!(f, "verification step description cannot be empty"),
            Self::MissingCommand(step) => write!(f, "verification step has no command: {step}"),
            Self::EmptyCommand(step) => write!(f, "verification command is empty: {step}"),
            Self::DuplicateDescription(step) => write!(f, "duplicate verification step: {step}"),
        }
    }
}
impl std::error::Error for VerificationPlanError {}

impl VerificationPlan {
    pub fn push(&mut self, step: VerificationStep) { self.steps.push(step); }
    pub fn required_steps(&self) -> impl Iterator<Item = &VerificationStep> { self.steps.iter().filter(|s| s.required) }
    pub fn is_empty(&self) -> bool { self.steps.is_empty() }
    pub fn validate(&self) -> Result<(), VerificationPlanError> {
        let mut seen = std::collections::BTreeSet::new();
        for step in &self.steps {
            let description = step.description.trim();
            if description.is_empty() { return Err(VerificationPlanError::EmptyDescription); }
            if !seen.insert(description.to_owned()) { return Err(VerificationPlanError::DuplicateDescription(description.to_owned())); }
            match step.command.as_deref().map(str::trim) {
                None if step.required => return Err(VerificationPlanError::MissingCommand(description.to_owned())),
                Some("") => return Err(VerificationPlanError::EmptyCommand(description.to_owned())),
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_required_commands_and_duplicates() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Build, description: "build".into(), command: Some("cargo check".into()), required: true });
        assert!(plan.validate().is_ok());
        plan.push(VerificationStep { kind: VerificationKind::Build, description: "build".into(), command: Some("cargo check".into()), required: false });
        assert!(matches!(plan.validate(), Err(VerificationPlanError::DuplicateDescription(_))));
    }
    #[test]
    fn optional_missing_command_is_allowed() {
        let mut plan = VerificationPlan::default();
        plan.push(VerificationStep { kind: VerificationKind::Custom, description: "note".into(), command: None, required: false });
        assert!(plan.validate().is_ok());
    }
}
