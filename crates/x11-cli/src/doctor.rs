use std::{env, path::{Path, PathBuf}, process::Command};
use crate::runtime;

#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
}

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
