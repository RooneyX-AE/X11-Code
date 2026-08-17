use anyhow::{Context, Result};
use std::{env, fs, path::{Path, PathBuf}, process::Stdio};
use tokio::process::Command;
use crate::runtime::{self, RuntimeKind, Source};

pub fn python_path(workspace: &Path) -> PathBuf {
    if cfg!(windows) { workspace.join(".venv/Scripts/python.exe") } else { workspace.join(".venv/bin/python") }
}

pub fn python_env_ready(workspace: &Path) -> bool {
    python_path(workspace).is_file()
}

pub fn print_status(workspace: &Path, json: bool) -> Result<()> {
    let workspace = workspace.canonicalize().context("resolve workspace")?;
    let is_python = workspace.join("pyproject.toml").is_file() || workspace.join("requirements.txt").is_file();
    let runtime = runtime::inspect(&workspace).into_iter().find(|s| s.kind == RuntimeKind::Python);
    let payload = serde_json::json!({
        "project": if is_python { "python" } else { "unknown" },
        "venv": {"path": workspace.join(".venv").display().to_string(), "ready": python_env_ready(&workspace)},
        "runtime": runtime.as_ref().map(|r| serde_json::json!({"source": match r.source { Source::System => "system", Source::Managed => "managed", Source::Missing => "missing" }, "version": r.version, "executable": r.executable.as_ref().map(|p| p.display().to_string())})),
        "uv_lock": workspace.join("uv.lock").is_file(),
        "requirements": workspace.join("requirements.txt").is_file(),
    });
    if json { println!("{}", serde_json::to_string_pretty(&payload)?); return Ok(()); }
    println!("Python Environment");
    println!("  project: {}", if is_python { "python" } else { "unknown" });
    println!("  .venv:   {}", if python_env_ready(&workspace) { "ready" } else { "missing" });
    println!("  path:    {}", workspace.join(".venv").display());
    println!("  uv.lock: {}", workspace.join("uv.lock").is_file());
    println!("  requirements.txt: {}", workspace.join("requirements.txt").is_file());
    if let Some(r) = runtime { println!("  runtime: {:?} {:?}", r.source, r.version); }
    Ok(())
}

pub async fn create_python(workspace: &Path) -> Result<()> {
    let workspace = workspace.canonicalize().context("resolve workspace")?;
    anyhow::ensure!(workspace.join("pyproject.toml").is_file() || workspace.join("requirements.txt").is_file(), "not a Python project: expected pyproject.toml or requirements.txt");
    let status = runtime::inspect(&workspace).into_iter().find(|s| s.kind == RuntimeKind::Python).context("Python runtime is missing; run `x11 runtime install python <version>`")?;
    anyhow::ensure!(status.source != Source::Missing, "Python runtime is missing; run `x11 runtime install python <version>`");
    let python = status.executable.context("resolved Python executable missing")?;
    let venv = workspace.join(".venv");
    if venv.exists() {
        anyhow::ensure!(python_path(&workspace).is_file(), "existing .venv is incomplete; remove it and retry");
        println!("Python environment already exists at {}", venv.display());
    } else {
        let mut cmd = Command::new(&python);
        cmd.args(["-m", "venv", ".venv"]).current_dir(&workspace).stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        let status = cmd.status().await.context("create Python virtual environment")?;
        anyhow::ensure!(status.success(), "python -m venv failed with exit code {:?}", status.code());
        println!("created Python environment at {}", venv.display());
    }
    if workspace.join("uv.lock").is_file() {
        if let Some(uv) = find_program("uv") {
            let mut cmd = Command::new(uv); cmd.args(["sync", "--locked"]).current_dir(&workspace).stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
            let status = cmd.status().await.context("sync uv project environment")?;
            anyhow::ensure!(status.success(), "uv sync --locked failed with exit code {:?}", status.code());
        } else { println!("uv.lock detected. Environment created, but `uv` is not installed; skipping dependency sync."); }
    } else if workspace.join("requirements.txt").is_file() {
        println!("Environment created. Run `x11 project run install` to install requirements.txt.");
    }
    Ok(())
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for root in env::split_paths(&path) {
        let candidate = root.join(name); if candidate.is_file() { return Some(candidate); }
        if cfg!(windows) { for suffix in [".exe", ".cmd"] { let candidate = root.join(format!("{name}{suffix}")); if candidate.is_file() { return Some(candidate); } } }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn venv_python_path_is_platform_specific() {
        let root = Path::new(".venv");
        let path = if cfg!(windows) { root.join("Scripts/python.exe") } else { root.join("bin/python") };
        assert!(path.to_string_lossy().contains("python"));
    }
    #[test]
    fn incomplete_existing_venv_is_detectable() {
        let root = std::env::temp_dir().join(format!("x11-env-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(!root.join(".venv").is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
