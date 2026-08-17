use anyhow::{Context, Result};
use std::{path::{Path, PathBuf}, process::Stdio};
use tokio::{process::Command, time::{timeout, Duration}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind { Node, Python, Rust, Unknown }

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub kind: ProjectKind,
    pub cwd: PathBuf,
    pub program: String,
    pub args: Vec<String>,
}

pub fn detect(workspace: &Path) -> ProjectKind {
    if workspace.join("package.json").is_file() { ProjectKind::Node }
    else if workspace.join("pyproject.toml").is_file() || workspace.join("requirements.txt").is_file() { ProjectKind::Python }
    else if workspace.join("Cargo.toml").is_file() { ProjectKind::Rust }
    else { ProjectKind::Unknown }
}

pub fn plan(workspace: &Path, kind: ProjectKind, action: &str) -> Result<ExecutionPlan> {
    let cwd = workspace.canonicalize().context("resolve workspace")?;
    let (program, args) = match (kind, action) {
        (ProjectKind::Node, "install") => ("npm".into(), vec!["install".into()]),
        (ProjectKind::Node, "test") => ("npm".into(), vec!["test".into()]),
        (ProjectKind::Node, "build") => ("npm".into(), vec!["run".into(), "build".into()]),
        (ProjectKind::Python, "install") => ("python".into(), vec!["-m".into(), "pip".into(), "install".into(), "-r".into(), "requirements.txt".into()]),
        (ProjectKind::Python, "test") => ("python".into(), vec!["-m".into(), "pytest".into()]),
        (ProjectKind::Rust, "test") => ("cargo".into(), vec!["test".into()]),
        (ProjectKind::Rust, "build") => ("cargo".into(), vec!["build".into()]),
        (_, other) => anyhow::bail!("unsupported project action: {other}"),
    };
    Ok(ExecutionPlan { kind, cwd, program, args })
}

pub async fn execute(plan: &ExecutionPlan, timeout_ms: u64, dry_run: bool) -> Result<i32> {
    println!("$ {} {}", plan.program, plan.args.join(" "));
    println!("cwd: {}", plan.cwd.display());
    if dry_run { return Ok(0); }
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args).current_dir(&plan.cwd).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
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
    #[test] fn node_test_plan_is_deterministic() {
        let p = plan(Path::new("."), ProjectKind::Node, "test").unwrap();
        assert_eq!(p.program, "npm");
        assert_eq!(p.args, vec!["test"]);
    }
}
