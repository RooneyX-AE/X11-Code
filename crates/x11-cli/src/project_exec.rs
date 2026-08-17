use anyhow::{Context, Result};
use std::{env, fs, path::{Path, PathBuf}, process::Stdio};
use tokio::{process::Command, time::{timeout, Duration}};
use crate::{node_manager, project_env, sandbox};
use crate::runtime::{self, RuntimeKind, RuntimeStatus, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind { Node, Python, Rust, Unknown }

#[derive(Debug, Clone)]
pub struct ExecutionPlan { pub kind: ProjectKind, pub cwd: PathBuf, pub program: PathBuf, pub args: Vec<String>, pub runtime: Option<RuntimeStatus> }

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
        (ProjectKind::Python, "install") => python_tool(&cwd, PythonAction::Install)?,
        (ProjectKind::Python, "test") => python_tool(&cwd, PythonAction::Test)?,
        (ProjectKind::Rust, "test") => (find_program("cargo")?, vec!["test".into()], None),
        (ProjectKind::Rust, "build") => (find_program("cargo")?, vec!["build".into()], None),
        (_, other) => anyhow::bail!("unsupported project action: {other}"),
    };
    Ok(ExecutionPlan { kind, cwd, program, args, runtime })
}

enum NodeAction { Install, Test, Build }

fn node_tool(workspace: &Path, action: NodeAction) -> Result<(PathBuf, Vec<String>, Option<RuntimeStatus>)> {
    let status = runtime_status(workspace, RuntimeKind::Node)?;
    anyhow::ensure!(status.source != Source::Missing, "Node.js runtime is missing; run `x11 runtime install node <version>`");
    let manager = node_manager::resolve_project(workspace)?;
    anyhow::ensure!(manager.state == node_manager::ResolveState::Ready,
        "package manager '{}' is not ready (requested: {}, detected: {})",
        manager.kind.name(), manager.requested.as_deref().unwrap_or("any"), manager.detected_version.as_deref().unwrap_or("missing"));
    let manager_program = manager.program.context("resolved package manager executable missing")?;
    let base_args = match (manager.kind, action) {
        (node_manager::ManagerKind::Npm, NodeAction::Install)
            if workspace.join("package-lock.json").is_file() || workspace.join("npm-shrinkwrap.json").is_file() => vec!["ci"],
        (node_manager::ManagerKind::Npm, NodeAction::Install) => vec!["install"],
        (node_manager::ManagerKind::Pnpm, NodeAction::Install) => vec!["install", "--frozen-lockfile"],
        (node_manager::ManagerKind::Yarn, NodeAction::Install) => vec!["install", "--immutable"],
        (_, NodeAction::Test) => vec!["test"],
        (_, NodeAction::Build) => vec!["run", "build"],
    };
    let (program, args) = wrap_project_local_yarn(&manager, manager_program, base_args, &status)?;
    Ok((program, args, Some(status)))
}

fn wrap_project_local_yarn(
    manager: &node_manager::ManagerResolution,
    manager_program: PathBuf,
    args: Vec<&str>,
    node: &RuntimeStatus,
) -> Result<(PathBuf, Vec<String>)> {
    let is_cjs = manager.kind == node_manager::ManagerKind::Yarn
        && manager_program.extension().and_then(|ext| ext.to_str()) == Some("cjs");
    if !is_cjs { return Ok((manager_program, args.into_iter().map(str::to_owned).collect())); }
    let node_program = node.executable.clone().context("managed Node executable missing for project-local Yarn")?;
    let mut wrapped = Vec::with_capacity(args.len() + 1);
    wrapped.push(manager_program.to_string_lossy().into_owned());
    wrapped.extend(args.into_iter().map(str::to_owned));
    Ok((node_program, wrapped))
}

enum PythonAction { Install, Test }
fn python_tool(workspace: &Path, action: PythonAction) -> Result<(PathBuf, Vec<String>, Option<RuntimeStatus>)> {
    let status = runtime_status(workspace, RuntimeKind::Python)?;
    anyhow::ensure!(status.source != Source::Missing, "Python runtime is missing; run `x11 runtime install python <version>`");
    anyhow::ensure!(project_env::python_env_ready(workspace), "Python project environment is missing at `.venv`; create it with `x11 project env python`");
    let python = project_env::python_path(workspace);
    let (program, args) = match action {
        PythonAction::Test => (python.clone(), vec!["-m".into(), "pytest".into()]),
        PythonAction::Install if workspace.join("uv.lock").is_file() && workspace.join("pyproject.toml").is_file() => {
            let uv = find_program("uv")?;
            (uv, vec!["sync".into(), "--locked".into()])
        }
        PythonAction::Install => (python.clone(), vec!["-m".into(), "pip".into(), "install".into(), "-r".into(), "requirements.txt".into()]),
    };
    Ok((program, args, Some(status)))
}

fn runtime_status(workspace: &Path, kind: RuntimeKind) -> Result<RuntimeStatus> {
    runtime::inspect(workspace).into_iter().find(|s| s.kind == kind).context("requested runtime was not detected for this workspace")
}

fn find_program(name: &str) -> Result<PathBuf> {
    let path = env::var_os("PATH").context("PATH is unavailable")?;
    for root in env::split_paths(&path) {
        let candidate = root.join(name); if candidate.is_file() { return Ok(candidate); }
        if cfg!(windows) { for suffix in [".exe", ".cmd"] { let candidate = root.join(format!("{name}{suffix}")); if candidate.is_file() { return Ok(candidate); } } }
    }
    anyhow::bail!("required executable '{name}' is not available on PATH")
}

fn runtime_path(status: &RuntimeStatus) -> Option<PathBuf> { status.executable.as_ref()?.parent().map(Path::to_path_buf) }

pub async fn execute(plan: &ExecutionPlan, timeout_ms: u64, dry_run: bool, sandbox_mode: sandbox::SandboxMode) -> Result<i32> {
    let (program, args, backend) = sandbox::wrap_command(sandbox_mode, &plan.cwd, &plan.program, &plan.args)?;
    println!("$ {} {}", program.display(), args.join(" "));
    println!("cwd: {}", plan.cwd.display());
    println!("sandbox: {:?}", backend);
    if dry_run { return Ok(0); }
    let mut cmd = Command::new(&program);
    cmd.args(&args).current_dir(&plan.cwd).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    if let Some(status) = &plan.runtime {
        if let Some(bin) = runtime_path(status) {
            let mut paths = vec![bin];
            if let Some(existing) = env::var_os("PATH") { paths.extend(env::split_paths(&existing)); }
            cmd.env("PATH", env::join_paths(paths).context("build runtime PATH")?);
        }
    }
    let status = timeout(Duration::from_millis(timeout_ms.clamp(1000, 1_800_000)), cmd.status()).await.context("project command timed out")??;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_projects() {
        let root = std::env::temp_dir().join(format!("x11-exec-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        assert_eq!(detect(&root), ProjectKind::Node);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_project_is_rejected() { assert!(plan(Path::new("."), ProjectKind::Unknown, "test").is_err()); }

    #[test]
    fn local_yarn_cjs_is_wrapped_by_node() {
        let manager = node_manager::ManagerResolution {
            kind: node_manager::ManagerKind::Yarn,
            requested: Some("4.0.0".into()), detected_version: Some("4.0.0".into()),
            program: Some(PathBuf::from(".yarn/releases/yarn-4.0.0.cjs")), state: node_manager::ResolveState::Ready,
        };
        let node = RuntimeStatus { kind: RuntimeKind::Node, source: Source::Managed, executable: Some(PathBuf::from("/x11/node/bin/node")), version: Some("24.0.0".into()), requested: None };
        let (program, args) = wrap_project_local_yarn(&manager, manager.program.clone().unwrap(), vec!["test"], &node).unwrap();
        assert_eq!(program, PathBuf::from("/x11/node/bin/node"));
        assert_eq!(args, vec![".yarn/releases/yarn-4.0.0.cjs", "test"]);
    }
}
