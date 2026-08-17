use anyhow::{Context, Result};
use serde_json::Value;
use std::{env, fs, path::{Path, PathBuf}, process::Stdio};
use tokio::{process::Command, time::{timeout, Duration}};
use crate::runtime::{self, RuntimeKind, RuntimeStatus, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind { Node, Python, Rust, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeManager { Npm, Pnpm, Yarn }

impl NodeManager {
    fn name(self) -> &'static str { match self { Self::Npm => "npm", Self::Pnpm => "pnpm", Self::Yarn => "yarn" } }
}

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub kind: ProjectKind,
    pub cwd: PathBuf,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub runtime: Option<RuntimeStatus>,
}

pub fn detect(workspace: &Path) -> ProjectKind {
    if workspace.join("package.json").is_file() { ProjectKind::Node }
    else if workspace.join("pyproject.toml").is_file() || workspace.join("requirements.txt").is_file() { ProjectKind::Python }
    else if workspace.join("Cargo.toml").is_file() { ProjectKind::Rust }
    else { ProjectKind::Unknown }
}

pub fn plan(workspace: &Path, kind: ProjectKind, action: &str) -> Result<ExecutionPlan> {
    let cwd = workspace.canonicalize().context("resolve workspace")?;
    let (program, args, runtime) = match (kind, action) {
        (ProjectKind::Node, "install") => node_tool(&cwd, NodeAction::Install)?,
        (ProjectKind::Node, "test") => node_tool(&cwd, NodeAction::Test)?,
        (ProjectKind::Node, "build") => node_tool(&cwd, NodeAction::Build)?,
        (ProjectKind::Python, "install") => python_tool(&cwd, vec!["-m", "pip", "install", "-r", "requirements.txt"])? ,
        (ProjectKind::Python, "test") => python_tool(&cwd, vec!["-m", "pytest"])? ,
        (ProjectKind::Rust, "test") => (find_program("cargo")?.into(), vec!["test".into()], None),
        (ProjectKind::Rust, "build") => (find_program("cargo")?.into(), vec!["build".into()], None),
        (_, other) => anyhow::bail!("unsupported project action: {other}"),
    };
    Ok(ExecutionPlan { kind, cwd, program, args, runtime })
}

enum NodeAction { Install, Test, Build }

fn node_tool(workspace: &Path, action: NodeAction) -> Result<(PathBuf, Vec<String>, Option<RuntimeStatus>)> {
    let status = runtime_status(workspace, RuntimeKind::Node)?;
    anyhow::ensure!(status.source != Source::Missing, "Node.js runtime is missing; run `x11 runtime install node <version>`");
    let manager = detect_node_manager(workspace)?;
    let args = match (manager, action) {
        (NodeManager::Npm, NodeAction::Install) if workspace.join("package-lock.json").is_file() || workspace.join("npm-shrinkwrap.json").is_file() => vec!["ci"],
        (NodeManager::Npm, NodeAction::Install) => vec!["install"],
        (NodeManager::Pnpm, NodeAction::Install) => vec!["install", "--frozen-lockfile"],
        (NodeManager::Yarn, NodeAction::Install) => vec!["install", "--immutable"],
        (NodeManager::Npm, NodeAction::Test) => vec!["test"],
        (NodeManager::Pnpm, NodeAction::Test) => vec!["test"],
        (NodeManager::Yarn, NodeAction::Test) => vec!["test"],
        (NodeManager::Npm, NodeAction::Build) => vec!["run", "build"],
        (NodeManager::Pnpm, NodeAction::Build) => vec!["run", "build"],
        (NodeManager::Yarn, NodeAction::Build) => vec!["run", "build"],
    };
    let runtime_bin = runtime_path(&status);
    let tool = find_manager_program(manager, runtime_bin.as_deref())?;
    Ok((tool, args.into_iter().map(str::to_owned).collect(), Some(status)))
}

fn detect_node_manager(workspace: &Path) -> Result<NodeManager> {
    if let Some(manager) = package_manager_field(workspace)? { return Ok(manager); }
    if workspace.join("pnpm-lock.yaml").is_file() { return Ok(NodeManager::Pnpm); }
    if workspace.join("yarn.lock").is_file() { return Ok(NodeManager::Yarn); }
    Ok(NodeManager::Npm)
}

fn package_manager_field(workspace: &Path) -> Result<Option<NodeManager>> {
    let path = workspace.join("package.json");
    if !path.is_file() { return Ok(None); }
    let text = fs::read_to_string(path).context("read package.json")?;
    let value: Value = serde_json::from_str(&text).context("parse package.json")?;
    let Some(field) = value.get("packageManager").and_then(Value::as_str) else { return Ok(None); };
    let name = field.split_once('@').map(|(n, _)| n).unwrap_or(field).to_ascii_lowercase();
    let manager = match name.as_str() {
        "npm" => NodeManager::Npm,
        "pnpm" => NodeManager::Pnpm,
        "yarn" => NodeManager::Yarn,
        other => anyhow::bail!("unsupported packageManager '{other}' in package.json"),
    };
    Ok(Some(manager))
}

fn find_manager_program(manager: NodeManager, runtime_bin: Option<&Path>) -> Result<PathBuf> {
    let name = manager.name();
    if let Some(bin) = runtime_bin {
        let direct = bin.join(if cfg!(windows) { format!("{name}.cmd") } else { name.to_owned() });
        if direct.is_file() { return Ok(direct); }
    }
    find_program(name)
}

fn python_tool(workspace: &Path, args: Vec<&str>) -> Result<(PathBuf, Vec<String>, Option<RuntimeStatus>)> {
    let status = runtime_status(workspace, RuntimeKind::Python)?;
    anyhow::ensure!(status.source != Source::Missing, "Python runtime is missing; run `x11 runtime install python <version>`");
    let python = status.executable.clone().context("resolved Python executable missing")?;
    Ok((python, args.into_iter().map(str::to_owned).collect(), Some(status)))
}

fn runtime_status(workspace: &Path, kind: RuntimeKind) -> Result<RuntimeStatus> {
    runtime::inspect(workspace).into_iter().find(|s| s.kind == kind).context("requested runtime was not detected for this workspace")
}

fn find_program(name: &str) -> Result<PathBuf> {
    let path = env::var_os("PATH").context("PATH is unavailable")?;
    for root in env::split_paths(&path) {
        let candidate = root.join(name);
        if candidate.is_file() { return Ok(candidate); }
        if cfg!(windows) {
            for suffix in [".exe", ".cmd"] {
                let candidate = root.join(format!("{name}{suffix}"));
                if candidate.is_file() { return Ok(candidate); }
            }
        }
    }
    anyhow::bail!("required executable '{name}' is not available on PATH")
}

fn runtime_path(status: &RuntimeStatus) -> Option<PathBuf> {
    status.executable.as_ref()?.parent().map(Path::to_path_buf)
}

pub async fn execute(plan: &ExecutionPlan, timeout_ms: u64, dry_run: bool) -> Result<i32> {
    println!("$ {} {}", plan.program.display(), plan.args.join(" "));
    println!("cwd: {}", plan.cwd.display());
    if dry_run { return Ok(0); }
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args).current_dir(&plan.cwd).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    if let Some(status) = &plan.runtime {
        if let Some(bin) = runtime_path(status) {
            let mut paths = vec![bin];
            if let Some(existing) = env::var_os("PATH") { paths.extend(env::split_paths(&existing)); }
            let joined = env::join_paths(paths).context("build runtime PATH")?;
            cmd.env("PATH", joined);
        }
    }
    let status = timeout(Duration::from_millis(timeout_ms.clamp(1000, 1_800_000)), cmd.status()).await.context("project command timed out")??;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn detects_common_projects() {
        let root = std::env::temp_dir().join(format!("x11-exec-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("package.json"), "{}").unwrap();
        assert_eq!(detect(&root), ProjectKind::Node);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn package_manager_field_has_priority() {
        let root = std::env::temp_dir().join(format!("x11-pm-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("package.json"), r#"{"packageManager":"pnpm@10.0.0"}"#).unwrap();
        std::fs::write(root.join("package-lock.json"), "{}").unwrap();
        assert_eq!(detect_node_manager(&root).unwrap(), NodeManager::Pnpm);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn lockfiles_select_install_policy() {
        let root = std::env::temp_dir().join(format!("x11-lock-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("package.json"), "{}").unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9'\n").unwrap();
        assert_eq!(detect_node_manager(&root).unwrap(), NodeManager::Pnpm);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn unknown_project_is_rejected() { assert!(plan(Path::new("."), ProjectKind::Unknown, "test").is_err()); }
    #[test] fn runtime_path_uses_executable_parent() {
        let status = RuntimeStatus { kind: RuntimeKind::Node, source: Source::System, executable: Some(PathBuf::from("/opt/node/bin/node")), version: Some("22.0.0".into()), requested: None, project_reason: None };
        assert_eq!(runtime_path(&status), Some(PathBuf::from("/opt/node/bin")));
    }
}
