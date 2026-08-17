use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::{Path, PathBuf}};
use x11_permissions::{Decision, Operation, Policy, Rule};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub permission: PermissionConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub read: Option<Decision>,
    pub shell: Option<Decision>,
    pub filesystem_write: Option<Decision>,
    pub network: Option<Decision>,
    pub git_write: Option<Decision>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

pub fn user_config_path() -> Option<PathBuf> {
    if let Some(root) = env::var_os("X11_CONFIG_HOME") { return Some(PathBuf::from(root).join("config.toml")); }
    if cfg!(windows) { env::var_os("APPDATA").map(PathBuf::from).map(|p| p.join("x11-code").join("config.toml")) }
    else { env::var_os("HOME").map(PathBuf::from).map(|p| p.join(".config").join("x11-code").join("config.toml")) }
}

pub fn project_config_path(workspace: &Path) -> PathBuf { workspace.join(".x11").join("config.toml") }

fn read_config(path: &Path) -> Result<Config> {
    if !path.is_file() { return Ok(Config::default()); }
    let text = fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse config {}", path.display()))
}

pub fn load(workspace: &Path) -> Result<Config> {
    let user = user_config_path().map(|p| read_config(&p)).transpose()?.unwrap_or_default();
    let project = read_config(&project_config_path(workspace))?;
    Ok(merge(user, project))
}

fn merge(user: Config, project: Config) -> Config {
    let mut permission = PermissionConfig {
        read: project.permission.read.or(user.permission.read),
        shell: project.permission.shell.or(user.permission.shell),
        filesystem_write: project.permission.filesystem_write.or(user.permission.filesystem_write),
        network: project.permission.network.or(user.permission.network),
        git_write: project.permission.git_write.or(user.permission.git_write),
        rules: user.permission.rules,
    };
    permission.rules.extend(project.permission.rules);
    Config { permission }
}

pub fn policy(workspace: &Path) -> Result<Policy> {
    let config = load(workspace)?;
    let mut policy = Policy::default();
    policy.read = config.permission.read.unwrap_or(policy.read);
    policy.shell = config.permission.shell.unwrap_or(policy.shell);
    policy.filesystem_write = config.permission.filesystem_write.unwrap_or(policy.filesystem_write);
    policy.network = config.permission.network.unwrap_or(policy.network);
    policy.git_write = config.permission.git_write.unwrap_or(policy.git_write);
    policy.rules = config.permission.rules;
    Ok(policy)
}

pub fn describe(workspace: &Path) -> Result<Vec<(String, String)>> {
    let mut rows = Vec::new();
    if let Some(path) = user_config_path() { rows.push(("user".into(), path.display().to_string())); }
    rows.push(("project".into(), project_config_path(workspace).display().to_string()));
    let p = policy(workspace)?;
    rows.push(("read".into(), format!("{:?}", p.read)));
    rows.push(("shell".into(), format!("{:?}", p.shell)));
    rows.push(("filesystem_write".into(), format!("{:?}", p.filesystem_write)));
    rows.push(("git_write".into(), format!("{:?}", p.git_write)));
    rows.push(("network".into(), format!("{:?}", p.network)));
    rows.push(("rules".into(), p.rules.len().to_string()));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_overrides_user_scalar() {
        let user = Config { permission: PermissionConfig { shell: Some(Decision::Ask), ..Default::default() } };
        let project = Config { permission: PermissionConfig { shell: Some(Decision::Deny), ..Default::default() } };
        let merged = merge(user, project);
        assert_eq!(merged.permission.shell, Some(Decision::Deny));
    }
    #[test]
    fn project_rules_append_after_user_rules() {
        let user = Config { permission: PermissionConfig { rules: vec![Rule { decision: Decision::Allow, operation: Some(Operation::Shell), pattern: Some("git *".into()) }], ..Default::default() } };
        let project = Config { permission: PermissionConfig { rules: vec![Rule { decision: Decision::Deny, operation: Some(Operation::Shell), pattern: Some("git push*".into()) }], ..Default::default() } };
        let merged = merge(user, project);
        assert_eq!(merged.permission.rules.len(), 2);
    }
}
