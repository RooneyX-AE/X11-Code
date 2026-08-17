use anyhow::{Context, Result};
use std::{fs, path::{Path, PathBuf}};
use x11_core::orchestration::Skill;
use x11_model::ModelProvider;
use x11_plugin::PluginRegistry;
use crate::AgentRuntime;

impl<P: ModelProvider + 'static> AgentRuntime<P> {
    pub fn load_plugins_from(&mut self, dir: impl AsRef<Path>) -> Result<usize> {
        let registry = PluginRegistry::discover(dir.as_ref())?;
        let mut loaded = 0usize;
        for plugin in registry.iter() {
            for path in plugin.skill_paths() {
                if !path.is_file() { continue; }
                self.orchestration.register_skill(parse_plugin_skill(&path)?);
                loaded += 1;
            }
        }
        Ok(loaded)
    }
}

fn parse_plugin_skill(path: &PathBuf) -> Result<Skill> {
    let text = fs::read_to_string(path).with_context(|| format!("read plugin skill {}", path.display()))?;
    let lines = text.lines().collect::<Vec<_>>();
    let mut name = path.file_stem().and_then(|v| v.to_str()).unwrap_or("skill").to_owned();
    let mut description = String::new();
    let mut body_start = 0usize;
    if lines.first().map(|v| v.trim()) == Some("---") {
        for (idx, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" { body_start = idx + 1; break; }
            if let Some((k,v)) = line.split_once(':') {
                let value=v.trim().trim_matches(['"','\'']);
                match k.trim() { "name" if !value.is_empty()=>name=value.to_owned(), "description"=>description=value.to_owned(), _=>{} }
            }
        }
    }
    let instructions=lines.get(body_start..).unwrap_or(&[]).join("\n").trim().to_owned();
    Ok(Skill{name,description,instructions,tool_hints:Vec::new()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_plugin_skill() {
        let root=std::env::temp_dir().join(format!("x11-plugin-skill-{}",uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();let path=root.join("SKILL.md");
        fs::write(&path,"---\nname: plugin-review\ndescription: Review\n---\nInspect regressions.").unwrap();
        let skill=parse_plugin_skill(&path).unwrap();assert_eq!(skill.name,"plugin-review");assert!(skill.instructions.contains("regressions"));
        let _=fs::remove_dir_all(root);
    }
}
