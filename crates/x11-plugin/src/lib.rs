pub mod security;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)] pub description: String,
    #[serde(default)] pub skills: Vec<String>,
    #[serde(default)] pub agents: Vec<String>,
    #[serde(default)] pub commands: Vec<String>,
    #[serde(default)] pub system_prompt: Option<String>,
    #[serde(default)] pub mcp_servers: serde_json::Value,
    #[serde(default)] pub hooks: Vec<PluginHook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHook { pub event: String, pub matcher: Option<String>, pub command: String, #[serde(default)] pub timeout_seconds: Option<u64> }

#[derive(Debug, Clone)]
pub struct Plugin { pub root: PathBuf, pub manifest: PluginManifest }

impl Plugin {
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().canonicalize().context("plugin root does not exist")?;
        let path = root.join("x11.plugin.json");
        let manifest: PluginManifest = serde_json::from_str(&fs::read_to_string(&path)?)?;
        if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() { anyhow::bail!("plugin name and version are required"); }
        for rel in manifest.skills.iter().chain(manifest.agents.iter()).chain(manifest.commands.iter()) {
            validate_relative_path(&root, rel)?;
        }
        Ok(Self { root, manifest })
    }

    pub fn skill_paths(&self) -> impl Iterator<Item=PathBuf> + '_ { self.manifest.skills.iter().map(|p| self.root.join(p)) }
    pub fn agent_paths(&self) -> impl Iterator<Item=PathBuf> + '_ { self.manifest.agents.iter().map(|p| self.root.join(p)) }
    pub fn command_paths(&self) -> impl Iterator<Item=PathBuf> + '_ { self.manifest.commands.iter().map(|p| self.root.join(p)) }
}

fn validate_relative_path(root: &Path, relative: &str) -> Result<()> {
    let candidate = Path::new(relative);
    if candidate.is_absolute() || candidate.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        anyhow::bail!("plugin path must stay relative to plugin root: {relative}");
    }
    let target = root.join(candidate);
    if target.exists() {
        let canonical = target.canonicalize().with_context(|| format!("canonicalize plugin path: {relative}"))?;
        if !canonical.starts_with(root) { anyhow::bail!("plugin path escapes plugin root: {relative}"); }
    } else if !target.starts_with(root) {
        anyhow::bail!("plugin path escapes plugin root: {relative}");
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct PluginRegistry { plugins: Vec<Plugin> }
impl PluginRegistry {
    pub fn discover(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let mut registry = Self::default();
        if !dir.is_dir() { return Ok(registry); }
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() && path.join("x11.plugin.json").is_file() {
                registry.plugins.push(Plugin::load(&path).with_context(|| format!("load plugin {}", path.display()))?);
            }
        }
        registry.plugins.sort_by(|a,b|a.manifest.name.cmp(&b.manifest.name));
        Ok(registry)
    }
    pub fn iter(&self) -> impl Iterator<Item=&Plugin> { self.plugins.iter() }
    pub fn get(&self, name: &str) -> Option<&Plugin> { self.plugins.iter().find(|p| p.manifest.name == name) }
    pub fn is_empty(&self) -> bool { self.plugins.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("x11-plugin-{label}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
    }

    #[test]
    fn loads_and_validates_manifest() {
        let root=temp_root("valid"); fs::create_dir_all(root.join("skills")).unwrap();
        fs::write(root.join("x11.plugin.json"), r#"{"name":"demo","version":"1.0.0","skills":["skills"]}"#).unwrap();
        let plugin=Plugin::load(&root).unwrap(); assert_eq!(plugin.manifest.name,"demo");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_parent_escape() {
        let root=temp_root("escape"); fs::create_dir_all(&root).unwrap();
        fs::write(root.join("x11.plugin.json"), r#"{"name":"demo","version":"1.0.0","skills":["../outside"]}"#).unwrap();
        let err=Plugin::load(&root).unwrap_err(); assert!(err.to_string().contains("must stay relative"));
        fs::remove_dir_all(root).unwrap();
    }
}
