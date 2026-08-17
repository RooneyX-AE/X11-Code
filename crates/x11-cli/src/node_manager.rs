use anyhow::{Context, Result};
use serde_json::Value;
use std::{env, fs, path::{Path, PathBuf}, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerKind { Npm, Pnpm, Yarn }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub fn resolve_project(workspace: &Path) -> Result<ManagerResolution> {
    let (kind, requested) = project_spec(workspace)?;
    resolve(workspace, kind, requested.as_deref())
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

fn project_spec(workspace: &Path) -> Result<(ManagerKind, Option<String>)> {
    let package_json = workspace.join("package.json");
    if package_json.is_file() {
        let value: Value = serde_json::from_str(&fs::read_to_string(&package_json).context("read package.json")?).context("parse package.json")?;
        if let Some(field) = value.get("packageManager").and_then(Value::as_str) {
            let (name, version) = field.rsplit_once('@').unwrap_or((field, ""));
            let kind = match name.trim().to_ascii_lowercase().as_str() {
                "npm" => ManagerKind::Npm,
                "pnpm" => ManagerKind::Pnpm,
                "yarn" => ManagerKind::Yarn,
                other => anyhow::bail!("unsupported packageManager '{other}' in package.json"),
            };
            return Ok((kind, (!version.trim().is_empty()).then(|| version.trim().to_owned())));
        }
    }
    if workspace.join("pnpm-lock.yaml").is_file() { return Ok((ManagerKind::Pnpm, None)); }
    if workspace.join("yarn.lock").is_file() { return Ok((ManagerKind::Yarn, None)); }
    Ok((ManagerKind::Npm, None))
}

fn project_local_binary(workspace: &Path, kind: ManagerKind) -> Option<PathBuf> {
    match kind {
        ManagerKind::Yarn => {
            if let Some(path) = yarn_path(workspace) { return Some(path); }
            let releases = workspace.join(".yarn/releases");
            let mut entries = fs::read_dir(releases).ok()?.filter_map(Result::ok).map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("cjs"));
            entries.next()
        }
        ManagerKind::Pnpm => {
            let local = workspace.join("node_modules/.bin/pnpm");
            local.is_file().then_some(local)
        }
        ManagerKind::Npm => None,
    }
}

fn yarn_path(workspace: &Path) -> Option<PathBuf> {
    let path = workspace.join(".yarnrc.yml");
    let text = fs::read_to_string(path).ok()?;
    let line = text.lines().find(|line| line.trim_start().starts_with("yarnPath:"))?;
    let raw = line.split_once(':')?.1.trim().trim_matches(['"', '\''].as_ref());
    if raw.is_empty() { return None; }
    let resolved = workspace.join(raw);
    resolved.is_file().then_some(resolved)
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for root in env::split_paths(&path) {
        let candidate = root.join(name);
        if candidate.is_file() { return Some(candidate); }
        if cfg!(windows) {
            for suffix in [".cmd", ".exe"] {
                let candidate = root.join(format!("{name}{suffix}"));
                if candidate.is_file() { return Some(candidate); }
            }
        }
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
    let actual = actual.trim_start_matches('v');
    let requested = requested.trim().trim_start_matches('v');
    if requested.is_empty() { return true; }
    actual == requested || (requested.matches('.').count() < 2 && actual.starts_with(&format!("{requested}.")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn patchless_request_matches_patch_release() { assert!(version_matches("10.2.3", "10.2")); }
    #[test] fn exact_request_rejects_different_patch() { assert!(!version_matches("10.2.3", "10.2.4")); }
    #[test] fn kind_names_are_stable() { assert_eq!(ManagerKind::Pnpm.name(), "pnpm"); }
    #[test] fn project_manager_field_wins_over_lockfile() {
        let root = env::temp_dir().join(format!("x11-node-manager-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("package.json"), r#"{"packageManager":"pnpm@10.2.0"}"#).unwrap();
        fs::write(root.join("yarn.lock"), "").unwrap();
        let (kind, version) = project_spec(&root).unwrap();
        assert_eq!(kind, ManagerKind::Pnpm);
        assert_eq!(version.as_deref(), Some("10.2.0"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test] fn yarn_path_is_resolved_from_project_config() {
        let root = env::temp_dir().join(format!("x11-yarn-manager-{}", std::process::id()));
        fs::create_dir_all(root.join(".yarn/releases")).unwrap();
        fs::write(root.join(".yarn/releases/yarn-4.0.0.cjs"), "// test").unwrap();
        fs::write(root.join(".yarnrc.yml"), "yarnPath: .yarn/releases/yarn-4.0.0.cjs\n").unwrap();
        assert_eq!(yarn_path(&root), Some(root.join(".yarn/releases/yarn-4.0.0.cjs")));
        fs::remove_dir_all(root).unwrap();
    }
}
