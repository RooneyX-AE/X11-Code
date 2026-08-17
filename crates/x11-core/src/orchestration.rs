use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentRole { Explorer, Planner, Implementer, Reviewer, Tester }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentSpec {
    pub id: String,
    pub role: SubagentRole,
    pub goal: String,
    #[serde(default)] pub max_iterations: u32,
    #[serde(default = "default_model")] pub model: String,
    #[serde(default = "default_token_budget")] pub token_budget: u32,
    #[serde(default = "default_tool_budget")] pub tool_budget: u32,
    #[serde(default)] pub allowed_tools: BTreeSet<String>,
    #[serde(default)] pub dependencies: BTreeSet<String>,
    #[serde(default)] pub priority: i32,
    #[serde(default)] pub workspace_scope: Option<String>,
}
fn default_model() -> String { "default".into() }
fn default_token_budget() -> u32 { 16_000 }
fn default_tool_budget() -> u32 { 64 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookEvent { BeforeRun, AfterRun, BeforeTool, AfterTool, OnError }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook { pub name: String, pub event: HookEvent, pub command: String, #[serde(default = "enabled")] pub enabled: bool }
fn enabled() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill { pub name: String, pub description: String, pub instructions: String, #[serde(default)] pub tool_hints: Vec<String> }

#[derive(Debug, Default, Clone)]
pub struct Orchestrator { subagents: BTreeMap<String, SubagentSpec>, skills: BTreeMap<String, Skill>, hooks: Vec<Hook> }
impl Orchestrator {
    pub fn register_subagent(&mut self, spec: SubagentSpec) { self.subagents.insert(spec.id.clone(), spec); }
    pub fn register_skill(&mut self, skill: Skill) { self.skills.insert(skill.name.clone(), skill); }
    pub fn register_hook(&mut self, hook: Hook) { self.hooks.push(hook); }
    pub fn subagents(&self) -> impl Iterator<Item=&SubagentSpec> { self.subagents.values() }
    pub fn skills(&self) -> impl Iterator<Item=&Skill> { self.skills.values() }
    pub fn get_subagent(&self, id: &str) -> Option<&SubagentSpec> { self.subagents.get(id) }
    pub fn hooks(&self, event: HookEvent) -> impl Iterator<Item=&Hook> { self.hooks.iter().filter(move |h| h.enabled && h.event == event) }
    pub fn default_subagents() -> Vec<SubagentSpec> {
        [
            ("explorer", SubagentRole::Explorer, "Map the repository structure and identify relevant files."),
            ("planner", SubagentRole::Planner, "Create a minimal implementation and verification plan."),
            ("implementer", SubagentRole::Implementer, "Implement the requested changes with small edits."),
            ("reviewer", SubagentRole::Reviewer, "Review changes for correctness, regressions, and safety."),
            ("tester", SubagentRole::Tester, "Run targeted tests or checks and report failures."),
        ].into_iter().map(|(id, role, goal)| SubagentSpec { id:id.into(), role, goal:goal.into(), max_iterations:8, model:"default".into(), token_budget:16_000, tool_budget:64, allowed_tools:BTreeSet::new(), dependencies:BTreeSet::new(), priority:0, workspace_scope:None }).collect()
    }
    pub fn install_defaults(&mut self) {
        for spec in Self::default_subagents() { self.register_subagent(spec); }
        self.register_skill(Skill { name:"safe-edit".into(), description:"Prefer narrow, verifiable edits over whole-file rewrites.".into(), instructions:"Read surrounding code first, make the smallest viable change, then run the narrowest relevant check.".into(), tool_hints:vec!["read_file".into(),"edit_file".into(),"shell".into()] });
        self.register_skill(Skill { name:"debug-loop".into(), description:"Iterate from failure to diagnosis to verification.".into(), instructions:"Capture the exact error, inspect the implicated code, patch one cause at a time, and rerun the failing check.".into(), tool_hints:vec!["search".into(),"read_file".into(),"edit_file".into(),"shell".into()] });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn defaults_are_useful(){let mut o=Orchestrator::default();o.install_defaults();let explorer=o.get_subagent("explorer").unwrap();assert_eq!(o.subagents().count(),5);assert_eq!(explorer.token_budget,16_000);assert_eq!(explorer.tool_budget,64);assert!(o.skills().any(|s|s.name=="safe-edit"));}
    #[test] fn hook_filtering_is_deterministic(){let mut o=Orchestrator::default();o.register_hook(Hook{name:"before".into(),event:HookEvent::BeforeRun,command:"true".into(),enabled:true});o.register_hook(Hook{name:"disabled".into(),event:HookEvent::BeforeRun,command:"false".into(),enabled:false});assert_eq!(o.hooks(HookEvent::BeforeRun).count(),1);assert_eq!(o.hooks(HookEvent::AfterRun).count(),0);}
}