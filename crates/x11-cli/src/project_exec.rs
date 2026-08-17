use anyhow::{Context, Result};
use std::{env, path::{Path, PathBuf}, process::Stdio};
use tokio::{process::Command, time::{timeout, Duration}};
use crate::runtime::{self, RuntimeKind, RuntimeStatus, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind { Node, Python, Rust, Unknown }

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
        (ProjectKind::Node, "install") => node_tool(&cwd, vec!["install"])? ,
        (ProjectKind::Node, "test") => node_tool(&cwd, vec!["test"])? ,
        (ProjectKind::Node, "build") => node_tool(&cwd, vec!["run", "build"])? ,
        (ProjectKind::Python, "install") => python_tool(&cwd, vec!["-m", "pip", "install", "-r", "requirements.txt"])? ,
        (ProjectKind::Python, "test") => python_tool(&cwd, vec!["-m", "pytest"])? ,
        (ProjectKind::Rust, "test") => (find_program("cargo")?.into(), vec!["test".into()], None),
        (ProjectKind::Rust, "build") => (find_program("cargo")?.into(), vec!["build".into()], None),
        (_, other) => anyhow::bail!("unsupported project action: {other}"),
    };
    Ok(ExecutionPlan { kind, cwd, program, args, runtime })
}

fn node_tool(workspace: &Path, args: Vec<&str>) -> Result<(PathBuf, Vec<String>, Option<RuntimeStatus>)> {
    let status = runtime_status(workspace, RuntimeKind::Node)?;
    anyhow::ensure!(status.source != Source::Missing, "Node.js runtime is missing; run `x11 runtime install node <version>`");
    let node = status.executable.clone().context("resolved Node.js executable missing")?;
    let tool = if cfg!(windows) { node.parent().context("Node runtime has no parent")?.join("npm.cmd") } else { node.parent().context("Node runtime has no parent")?.join("npm") };
    anyhow::ensure!(tool.is_file(), "npm executable not found beside resolved Node.js runtime");
    Ok((tool, args.into_iter().map(str::to_owned).collect(), Some(status)))
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
    #[test] fn unknown_project_is_rejected() { assert!(plan(Path::new("."), ProjectKind::Unknown, "test").is_err()); }
    #[test] fn runtime_path_uses_executable_parent() {
        let status = RuntimeStatus { kind: RuntimeKind::Node, source: Source::System, executable: Some(PathBuf::from("/opt/node/bin/node")), version: Some("22.0.0".into()), requested: None, project_reason: None };
        assert_eq!(runtime_path(&status), Some(PathBuf::from("/opt/node/bin")));
    }
}
