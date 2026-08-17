use anyhow::{Context, Result};
use std::{env, path::{Path, PathBuf}, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerKind { Npm, Pnpm, Yarn }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveState { Ready, VersionMismatch, Missing }

#[derive(Debug, Clone)]
pub struct ManagerResolution {
    pub kind: ManagerKind,
    pub requested: Option<String>,
    pub detected_version: Option<String>,
    pub program: Option<PathBuf>,
    pub state: ResolveState,
}

impl ManagerKind {
    pub fn name(self) -> &'static str { match self { Self::Npm => "npm", Self::Pnpm => "pnpm", Self::Yarn => "yarn" } }
}

pub fn resolve(workspace: &Path, kind: ManagerKind, requested: Option<&str>) -> Result<ManagerResolution> {
    let program = project_local_binary(workspace, kind).or_else(|| find_program(kind.name()));
    let Some(program) = program else {
        return Ok(ManagerResolution { kind, requested: requested.map(str::to_owned), detected_version: None, program: None, state: ResolveState::Missing });
    };
    let detected_version = version_of(&program)?;
    let state = match requested {
        None => ResolveState::Ready,
        Some(req) if version_matches(&detected_version, req) => ResolveState::Ready,
        Some(_) => ResolveState::VersionMismatch,
    };
    Ok(ManagerResolution { kind, requested: requested.map(str::to_owned), detected_version: Some(detected_version), program: Some(program), state })
}

fn project_local_binary(workspace: &Path, kind: ManagerKind) -> Option<PathBuf> {
    match kind {
        ManagerKind::Yarn => {
            let releases = workspace.join(".yarn/releases");
            let mut entries = std::fs::read_dir(releases).ok()?.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().and_then(|s| s.to_str()) == Some("cjs"));
            entries.next()
        }
        ManagerKind::Pnpm => {
            let local = workspace.join("node_modules/.bin/pnpm");
            local.is_file().then_some(local)
        }
        ManagerKind::Npm => None,
    }
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for root in env::split_paths(&path) {
        let candidate = root.join(name);
        if candidate.is_file() { return Some(candidate); }
        if cfg!(windows) { for suffix in [".cmd", ".exe"] { let candidate = root.join(format!("{name}{suffix}")); if candidate.is_file() { return Some(candidate); } } }
    }
    None
}

fn version_of(program: &Path) -> Result<String> {
    let output = Command::new(program).arg("--version").output().with_context(|| format!("run {} --version", program.display()))?;
    anyhow::ensure!(output.status.success(), "{} --version failed", program.display());
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let version = text.split_whitespace().find(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit())).unwrap_or(text.as_str());
    Ok(version.trim_start_matches('v').to_owned())
}

fn version_matches(actual: &str, requested: &str) -> bool {
    let requested = requested.trim_start_matches('v');
    actual == requested || actual.starts_with(&format!("{}.", requested))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn patchless_request_matches_patch_release() { assert!(version_matches("10.2.3", "10.2")); }
    #[test] fn exact_request_rejects_different_patch() { assert!(!version_matches("10.2.3", "10.2.4")); }
    #[test] fn kind_names_are_stable() { assert_eq!(ManagerKind::Pnpm.name(), "pnpm"); }
}