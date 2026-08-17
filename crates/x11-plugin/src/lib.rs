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
            let p = root.join(rel);
            if !p.starts_with(&root) { anyhow::bail!("plugin path escapes plugin root: {rel}"); }
        }
        Ok(Self { root, manifest })
    }

    pub fn skill_paths(&self) -> impl Iterator<Item=PathBuf> + '_ { self.manifest.skills.iter().map(|p| self.root.join(p)) }
    pub fn agent_paths(&self) -> impl Iterator<Item=PathBuf> + '_ { self.manifest.agents.iter().map(|p| self.root.join(p)) }
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
                if let Ok(plugin) = Plugin::load(&path) { registry.plugins.push(plugin); }
            }
        }
        registry.plugins.sort_by(|a,b| a.manifest.name.cmp(&b.manifest.name));
        Ok(registry)
    }
    pub fn iter(&self) -> impl Iterator<Item=&Plugin> { self.plugins.iter() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[test]
    fn loads_and_validates_manifest() {
        let root = std::env::temp_dir().join(format!("x11-plugin-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("x11.plugin.json"), r#"{"name":"demo","version":"1.0.0","skills":["skills"]}"#).unwrap();
        fs::create_dir(root.join("skills")).unwrap();
        let plugin = Plugin::load(&root).unwrap();
        assert_eq!(plugin.manifest.name, "demo");
        fs::remove_dir_all(root).unwrap();
    }
}
