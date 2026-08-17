use std::{env, fs, path::{Path, PathBuf}, process::Command};
use crate::runtime;
use x11_agent::tool_executor::sandbox;

#[derive(Debug, Clone)]
pub struct Check { pub name: &'static str, pub status: Status, pub detail: String }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status { Ok, Warn, Fail }

pub fn collect() -> Vec<Check> {
    let mut checks = vec![
        check_command("git", true),
        check_command("rg", false),
        check_shell(),
        check_workspace(),
        check_model_env(),
    ];
    let workspace = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    checks.push(check_project_environment(&workspace));
    checks.push(check_mcp_config(&workspace));
    checks.push(check_sandbox());
    for status in runtime::inspect(&workspace) {
        let name = match status.kind { runtime::RuntimeKind::Node => "node-runtime", runtime::RuntimeKind::Python => "python-runtime" };
        let detail = match (&status.source, &status.executable, &status.version, &status.requested) {
            (runtime::Source::System, Some(exe), Some(version), Some(req)) => format!("system {version} at {} (requested {req})", exe.display()),
            (runtime::Source::System, Some(exe), Some(version), None) => format!("system {version} at {}", exe.display()),
            (runtime::Source::Managed, Some(exe), Some(version), Some(req)) => format!("managed {version} at {} (requested {req})", exe.display()),
            (runtime::Source::Managed, Some(exe), Some(version), None) => format!("managed {version} at {}", exe.display()),
            (runtime::Source::Missing, _, _, Some(req)) => format!("missing (requested {req})"),
            (runtime::Source::Missing, _, _, None) => "missing".into(),
            _ => "unavailable".into(),
        };
        checks.push(Check { name, status: if matches!(status.source, runtime::Source::Missing) { Status::Warn } else { Status::Ok }, detail });
    }
    checks
}

fn check_command(name: &'static str, required: bool) -> Check {
    let command = if cfg!(windows) { "where" } else { "which" };
    match Command::new(command).arg(name).output() {
        Ok(output) if output.status.success() => {
            let version = Command::new(name).arg("--version").output().ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "installed".into());
            Check { name, status: Status::Ok, detail: version }
        }
        _ if required => Check { name, status: Status::Fail, detail: "missing".into() },
        _ => Check { name, status: Status::Warn, detail: "missing (optional)".into() },
    }
}

fn check_project_environment(workspace: &Path) -> Check {
    let package = workspace.join("package.json");
    let pyproject = workspace.join("pyproject.toml");
    let requirements = workspace.join("requirements.txt");
    let cargo = workspace.join("Cargo.toml");
    if package.is_file() {
        let manager = fs::read_to_string(&package).ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|v| v.get("packageManager").and_then(|v| v.as_str()).map(str::to_owned))
            .or_else(|| {
                if workspace.join("pnpm-lock.yaml").is_file() { Some("pnpm (lockfile)".into()) }
                else if workspace.join("yarn.lock").is_file() { Some("yarn (lockfile)".into()) }
                else if workspace.join("package-lock.json").is_file() { Some("npm (lockfile)".into()) }
                else { Some("npm (default)".into()) }
            }).unwrap_or_else(|| "unknown".into());
        let lock = if workspace.join("pnpm-lock.yaml").is_file() { "pnpm-lock.yaml" }
            else if workspace.join("yarn.lock").is_file() { "yarn.lock" }
            else if workspace.join("package-lock.json").is_file() { "package-lock.json" }
            else { "no lockfile" };
        let status = if lock == "no lockfile" { Status::Warn } else { Status::Ok };
        return Check { name: "project-env", status, detail: format!("Node package manager: {manager}; lock: {lock}") };
    }
    if pyproject.is_file() || requirements.is_file() {
        let venv = workspace.join(".venv");
        let python = if cfg!(windows) { venv.join("Scripts/python.exe") } else { venv.join("bin/python") };
        let lock = if workspace.join("uv.lock").is_file() { "uv.lock" } else if requirements.is_file() { "requirements.txt" } else { "no lockfile" };
        let status = if python.is_file() { Status::Ok } else { Status::Warn };
        let detail = if python.is_file() { format!("Python .venv ready; source: {lock}") } else { format!("Python .venv missing; run `x11 project env python`; source: {lock}") };
        return Check { name: "project-env", status, detail };
    }
    if cargo.is_file() { return Check { name: "project-env", status: Status::Ok, detail: "Rust Cargo project detected".into() }; }
    Check { name: "project-env", status: Status::Warn, detail: "no recognized project manifest".into() }
}

fn config_candidates(workspace: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(root) = env::var_os("X11_MCP_CONFIG") { paths.push(PathBuf::from(root)); }
    if let Some(root) = env::var_os("X11_CONFIG_HOME") { paths.push(PathBuf::from(root).join("mcp.json")); }
    if cfg!(windows) {
        if let Some(root) = env::var_os("APPDATA") { paths.push(PathBuf::from(root).join("x11-code").join("mcp.json")); }
    } else if let Some(root) = env::var_os("HOME") {
        paths.push(PathBuf::from(root).join(".config").join("x11-code").join("mcp.json"));
    }
    paths.push(workspace.join(".x11").join("mcp.json"));
    paths
}

fn check_mcp_config(workspace: &Path) -> Check {
    let files = config_candidates(workspace).into_iter().filter(|p| p.is_file()).collect::<Vec<_>>();
    if files.is_empty() {
        return Check { name: "mcp-config", status: Status::Ok, detail: "no MCP configuration (optional)".into() };
    }
    let mut servers = 0usize;
    for path in &files {
        let text = match fs::read_to_string(path) {
            Ok(v) => v,
            Err(error) => return Check { name: "mcp-config", status: Status::Fail, detail: format!("{}: {error}", path.display()) },
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(error) => return Check { name: "mcp-config", status: Status::Fail, detail: format!("invalid JSON at {}: {error}", path.display()) },
        };
        match value.get("mcpServers").and_then(|v| v.as_object()) {
            Some(map) => servers += map.len(),
            None => return Check { name: "mcp-config", status: Status::Fail, detail: format!("{}: `mcpServers` must be an object", path.display()) },
        }
    }
    Check { name: "mcp-config", status: Status::Ok, detail: format!("{} config file(s), {servers} server definition(s); parse-only check", files.len()) }
}

fn check_sandbox() -> Check {
    let capability = sandbox::detect();
    let detail = if capability.backend == sandbox::Backend::None {
        format!("backend=none; fs=false; net=false; proc=false; {}", capability.reason)
    } else {
        format!("backend={:?}; capability detected; runtime preflight not verified; fs={} net={} proc={}; {}", capability.backend, capability.filesystem_isolation, capability.network_isolation, capability.process_isolation, capability.reason)
    };
    Check { name: "sandbox", status: Status::Warn, detail }
}

fn check_shell() -> Check {
    let shell = if cfg!(windows) {
        if Path::new(r"C:\Program Files\Git\bin\bash.exe").exists() { "Git Bash" }
        else if env::var_os("ComSpec").is_some() { "cmd.exe" } else { "unknown" }
    } else if env::var("SHELL").is_ok() { "configured shell" } else { "unknown" };
    Check { name: "shell", status: if shell == "unknown" { Status::Warn } else { Status::Ok }, detail: shell.into() }
}

fn check_workspace() -> Check {
    let path = env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let is_git = path.join(".git").exists();
    Check { name: "workspace", status: if is_git { Status::Ok } else { Status::Warn }, detail: if is_git { format!("git repository: {}", path.display()) } else { format!("not a git root: {}", path.display()) } }
}

fn check_model_env() -> Check {
    match (env::var("X11_API_KEY"), env::var("X11_BASE_URL")) {
        (Ok(_), Ok(base)) if !base.trim().is_empty() => Check { name: "model", status: Status::Ok, detail: format!("configured: {base}") },
        _ => Check { name: "model", status: Status::Warn, detail: "X11_API_KEY/X11_BASE_URL not configured".into() },
    }
}

pub fn print(quiet: bool) -> bool {
    let checks = collect();
    if !quiet {
        println!("X11 Code Doctor\n");
        for check in &checks {
            let marker = match check.status { Status::Ok => '✓', Status::Warn => '!', Status::Fail => '✗' };
            println!("{marker} {:<14} {}", check.name, check.detail);
        }
        println!();
    }
    !checks.iter().any(|c| c.status == Status::Fail)
}
