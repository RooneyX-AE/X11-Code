use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentRole {
    Explorer,
    Planner,
    Implementer,
    Reviewer,
    Tester,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentSpec {
    pub id: String,
    pub role: SubagentRole,
    pub goal: String,
    #[serde(default)]
    pub max_iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    BeforeRun,
    AfterRun,
    BeforeTool,
    AfterTool,
    OnError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub name: String,
    pub event: HookEvent,
    pub command: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    #[serde(default)]
    pub tool_hints: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Orchestrator {
    subagents: BTreeMap<String, SubagentSpec>,
    skills: BTreeMap<String, Skill>,
    hooks: Vec<Hook>,
}

impl Orchestrator {
    pub fn register_subagent(&mut self, spec: SubagentSpec) {
        self.subagents.insert(spec.id.clone(), spec);
    }

    pub fn register_skill(&mut self, skill: Skill) {
        self.skills.insert(skill.name.clone(), skill);
    }

    pub fn register_hook(&mut self, hook: Hook) {
        self.hooks.push(hook);
    }

    pub fn subagents(&self) -> impl Iterator<Item = &SubagentSpec> { self.subagents.values() }
    pub fn skills(&self) -> impl Iterator<Item = &Skill> { self.skills.values() }
    pub fn hooks(&self, event: HookEvent) -> impl Iterator<Item = &Hook> {
        self.hooks.iter().filter(move |h| h.enabled && h.event == event)
    }

    pub fn default_subagents() -> Vec<SubagentSpec> {
        [
            ("explorer", SubagentRole::Explorer, "Map the repository structure and identify relevant files."),
            ("planner", SubagentRole::Planner, "Create a minimal implementation and verification plan."),
            ("implementer", SubagentRole::Implementer, "Implement the requested changes with small edits."),
            ("reviewer", SubagentRole::Reviewer, "Review changes for correctness, regressions, and safety."),
            ("tester", SubagentRole::Tester, "Run targeted tests or checks and report failures."),
        ].into_iter().map(|(id, role, goal)| SubagentSpec {
            id: id.into(), role, goal: goal.into(), max_iterations: 8,
        }).collect()
    }

    pub fn install_defaults(&mut self) {
        for spec in Self::default_subagents() { self.register_subagent(spec); }
        self.register_skill(Skill {
            name: "safe-edit".into(),
            description: "Prefer narrow, verifiable edits over whole-file rewrites.".into(),
            instructions: "Read surrounding code first, make the smallest viable change, then run the narrowest relevant check.".into(),
            tool_hints: vec!["read_file".into(), "edit_file".into(), "shell".into()],
        });
        self.register_skill(Skill {
            name: "debug-loop".into(),
            description: "Iterate from failure to diagnosis to verification.".into(),
            instructions: "Capture the exact error, inspect the implicated code, patch one cause at a time, and rerun the failing check.".into(),
            tool_hints: vec!["search".into(), "read_file".into(), "edit_file".into(), "shell".into()],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_useful() {
        let mut o = Orchestrator::default();
        o.install_defaults();
        assert_eq!(o.subagents().count(), 5);
        assert!(o.skills().any(|s| s.name == "safe-edit"));
    }
}
