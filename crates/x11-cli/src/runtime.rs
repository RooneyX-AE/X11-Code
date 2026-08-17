use std::{env, fs, path::{Path, PathBuf}, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind { Node, Python }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source { System, Managed, Missing }

#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub kind: RuntimeKind,
    pub source: Source,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub requested: Option<String>,
    pub project_reason: Option<String>,
}

pub fn detect_project(workspace: &Path) -> Vec<RuntimeKind> {
    let mut out = Vec::new();
    if workspace.join("package.json").is_file() || workspace.join("package-lock.json").is_file() || workspace.join("pnpm-lock.yaml").is_file() || workspace.join("yarn.lock").is_file() { out.push(RuntimeKind::Node); }
    if workspace.join("pyproject.toml").is_file() || workspace.join("requirements.txt").is_file() || workspace.join("uv.lock").is_file() || workspace.join(".python-version").is_file() { out.push(RuntimeKind::Python); }
    out
}

pub fn inspect(workspace: &Path) -> Vec<RuntimeStatus> {
    detect_project(workspace).into_iter().map(|kind| inspect_one(kind, workspace)).collect()
}

fn inspect_one(kind: RuntimeKind, workspace: &Path) -> RuntimeStatus {
    let requested = match kind {
        RuntimeKind::Node => read_version_file(workspace, ".nvmrc"),
        RuntimeKind::Python => read_version_file(workspace, ".python-version"),
    };
    if let Some((exe, version)) = find_system(kind, requested.as_deref()) {
        return RuntimeStatus { kind, source: Source::System, executable: Some(exe), version: Some(version), requested, project_reason: reason(kind, workspace) };
    }
    if let Some((exe, version)) = find_managed(kind, requested.as_deref()) {
        return RuntimeStatus { kind, source: Source::Managed, executable: Some(exe), version: Some(version), requested, project_reason: reason(kind, workspace) };
    }
    RuntimeStatus { kind, source: Source::Missing, executable: None, version: None, requested, project_reason: reason(kind, workspace) }
}

fn reason(kind: RuntimeKind, workspace: &Path) -> Option<String> {
    match kind {
        RuntimeKind::Node if workspace.join("package.json").is_file() => Some("package.json".into()),
        RuntimeKind::Python if workspace.join("pyproject.toml").is_file() => Some("pyproject.toml".into()),
        RuntimeKind::Python if workspace.join("requirements.txt").is_file() => Some("requirements.txt".into()),
        _ => None,
    }
}

fn read_version_file(workspace: &Path, name: &str) -> Option<String> {
    fs::read_to_string(workspace.join(name)).ok().map(|s| s.trim().trim_start_matches('v').to_owned()).filter(|s| !s.is_empty())
}

fn find_system(kind: RuntimeKind, requested: Option<&str>) -> Option<(PathBuf, String)> {
    let command = match kind { RuntimeKind::Node => "node", RuntimeKind::Python => if cfg!(windows) { "python" } else { "python3" } };
    let exe = find_on_path(command)?;
    let version = command_version(&exe, if kind == RuntimeKind::Node { "--version" } else { "--version" })?;
    if requested.is_some_and(|r| !version_matches(version.trim_start_matches('v'), r)) { return None; }
    Some((exe, version))
}

fn find_managed(kind: RuntimeKind, requested: Option<&str>) -> Option<(PathBuf, String)> {
    let root = env::var_os("X11_RUNTIME_HOME").map(PathBuf::from).unwrap_or_else(|| config_root().join("runtimes"));
    let dir = root.join(match kind { RuntimeKind::Node => "node", RuntimeKind::Python => "python" });
    if !dir.is_dir() { return None; }
    let requested = requested.unwrap_or("");
    let mut candidates = fs::read_dir(&dir).ok()?.filter_map(Result::ok).filter(|e| e.path().is_dir()).map(|e| e.path()).collect::<Vec<_>>();
    candidates.sort();
    candidates.reverse();
    for path in candidates {
        let name = path.file_name()?.to_string_lossy();
        if !requested.is_empty() && !version_matches(&name, requested) { continue; }
        let exe = path.join(if cfg!(windows) { if kind == RuntimeKind::Node { "node.exe" } else { "python.exe" } } else { if kind == RuntimeKind::Node { "bin/node" } else { "bin/python3" } });
        if exe.is_file() { if let Some(version) = command_version(&exe, "--version") { return Some((exe, version)); } }
    }
    None
}

fn config_root() -> PathBuf {
    env::var_os("X11_CONFIG_HOME").map(PathBuf::from).or_else(|| env::var_os("XDG_DATA_HOME").map(|p| PathBuf::from(p).join("x11"))).or_else(|| env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("x11"))).unwrap_or_else(|| PathBuf::from(".x11"))
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for root in env::split_paths(&path) { let candidate = root.join(name); if candidate.is_file() { return Some(candidate); } if cfg!(windows) { let exe = root.join(format!("{name}.exe")); if exe.is_file() { return Some(exe); } } }
    None
}

fn command_version(exe: &Path, arg: &str) -> Option<String> {
    Command::new(exe).arg(arg).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).filter(|s| !s.is_empty())
}

fn version_matches(actual: &str, requested: &str) -> bool {
    let a = actual.trim_start_matches('v');
    let r = requested.trim_start_matches('v');
    if r.contains('.') { a == r || a.starts_with(&format!("{r}.")) } else { a.split('.').next() == Some(r) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn project_detection_is_file_based(){let p=std::env::temp_dir().join(format!("x11-runtime-{}",std::process::id()));fs::create_dir_all(&p).unwrap();fs::write(p.join("package.json"),"{}").unwrap();assert_eq!(detect_project(&p),vec![RuntimeKind::Node]);let _=fs::remove_dir_all(p);}
    #[test] fn version_matching_supports_major_minor(){assert!(version_matches("22.19.0","22"));assert!(version_matches("22.19.0","22.19"));assert!(!version_matches("20.10.0","22"));}
}
