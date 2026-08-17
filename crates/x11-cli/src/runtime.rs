use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{env, fs, io::Write, path::{Path, PathBuf}, process::Command};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedManifest {
    pub runtime: String,
    pub version: String,
    pub source: String,
    pub asset: String,
    pub sha256: String,
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
    let version = command_version(&exe, "--version")?;
    if requested.is_some_and(|r| !version_matches(version.trim_start_matches('v'), r)) { return None; }
    Some((exe, version))
}

fn find_managed(kind: RuntimeKind, requested: Option<&str>) -> Option<(PathBuf, String)> {
    let root = runtime_root();
    let dir = root.join(match kind { RuntimeKind::Node => "node", RuntimeKind::Python => "python" });
    if !dir.is_dir() { return None; }
    let requested = requested.unwrap_or("");
    let mut candidates = fs::read_dir(&dir).ok()?.filter_map(Result::ok).filter(|e| e.path().is_dir()).map(|e| e.path()).collect::<Vec<_>>();
    candidates.sort();
    candidates.reverse();
    for path in candidates {
        let manifest = read_manifest(&path);
        let name = path.file_name()?.to_string_lossy();
        if let Some(m) = &manifest { if m.runtime != runtime_name(kind) { continue; } }
        if !requested.is_empty() && !version_matches(&name, requested) { continue; }
        let exe = path.join(if cfg!(windows) { if kind == RuntimeKind::Node { "node.exe" } else { "python.exe" } } else { if kind == RuntimeKind::Node { "bin/node" } else { "bin/python3" } });
        if exe.is_file() { if let Some(version) = command_version(&exe, "--version") { return Some((exe, version)); } }
    }
    None
}

pub fn runtime_root() -> PathBuf {
    env::var_os("X11_RUNTIME_HOME").map(PathBuf::from).unwrap_or_else(|| config_root().join("runtimes"))
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

fn runtime_name(kind: RuntimeKind) -> &'static str { match kind { RuntimeKind::Node => "node", RuntimeKind::Python => "python" } }

fn read_manifest(root: &Path) -> Option<ManagedManifest> {
    let text = fs::read_to_string(root.join("manifest.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub async fn install_node(version: &str) -> Result<PathBuf> {
    let version = version.trim().trim_start_matches('v');
    anyhow::ensure!(!version.is_empty(), "Node version must not be empty");
    anyhow::ensure!(version.bytes().all(|b| b.is_ascii_digit() || b == b'.'), "Node version must be numeric like 24, 24.19, or 24.19.0");

    let (target, archive_kind, binary_name) = match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => ("linux-x64", "tar.xz", "node"),
        ("linux", "aarch64") => ("linux-arm64", "tar.xz", "node"),
        ("macos", "x86_64") => ("darwin-x64", "tar.xz", "node"),
        ("macos", "aarch64") => ("darwin-arm64", "tar.xz", "node"),
        ("windows", "x86_64") => ("win-x64", "zip", "node.exe"),
        ("windows", "aarch64") => ("win-arm64", "zip", "node.exe"),
        _ => anyhow::bail!("unsupported Node platform: {}/{}", env::consts::OS, env::consts::ARCH),
    };
    let root = runtime_root().join("node").join(version);
    if root.join(binary_name).is_file() || root.join("bin/node").is_file() {
        if let Some(manifest) = read_manifest(&root) {
            if manifest.sha256.is_empty() { anyhow::bail!("managed Node manifest is invalid: empty sha256"); }
            println!("Node.js {version} already installed at {}", root.display());
            return Ok(root);
        }
    }
    let temp = env::temp_dir().join(format!("x11-node-{}-{}", version, std::process::id()));
    if temp.exists() { fs::remove_dir_all(&temp).ok(); }
    fs::create_dir_all(&temp)?;
    let archive_name = format!("node-v{version}-{target}.{archive_kind}");
    let base = format!("https://nodejs.org/download/release/v{version}/");
    let archive_url = format!("{base}{archive_name}");
    let sums_url = format!("{base}SHASUMS256.txt");
    let client = Client::builder().user_agent("x11-code-runtime").build()?;
    let archive = temp.join(&archive_name);
    let sums = temp.join("SHASUMS256.txt");
    download(&client, &archive_url, &archive).await.context("download Node.js archive")?;
    download(&client, &sums_url, &sums).await.context("download Node.js SHA256 manifest")?;
    let expected = parse_sha256(&sums, &archive_name)?;
    let actual = sha256_file(&archive)?;
    anyhow::ensure!(expected.eq_ignore_ascii_case(&actual), "Node.js checksum mismatch: expected {expected}, got {actual}");

    let extracted = temp.join("extract");
    fs::create_dir_all(&extracted)?;
    if archive_kind == "zip" {
        let status = Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force", archive.display(), extracted.display())]).status().context("spawn PowerShell")?;
        anyhow::ensure!(status.success(), "failed to extract Node.js zip archive");
    } else {
        let status = Command::new("tar").args(["-xJf", archive.to_string_lossy().as_ref(), "-C", extracted.to_string_lossy().as_ref()]).status().context("spawn tar")?;
        anyhow::ensure!(status.success(), "failed to extract Node.js tar.xz archive");
    }
    let payload = fs::read_dir(&extracted)?.filter_map(Result::ok).find(|e| e.path().is_dir()).map(|e| e.path()).context("Node archive did not contain a directory")?;
    fs::create_dir_all(root.parent().unwrap())?;
    let staging = root.with_extension("staging");
    if staging.exists() { fs::remove_dir_all(&staging).ok(); }
    copy_dir(&payload, &staging)?;
    let manifest = ManagedManifest { runtime: "node".into(), version: version.into(), source: "nodejs.org".into(), asset: archive_name, sha256: actual };
    let manifest_text = serde_json::to_string_pretty(&manifest)?;
    let mut f = fs::File::create(staging.join("manifest.json"))?;
    f.write_all(manifest_text.as_bytes())?;
    if root.exists() { fs::remove_dir_all(&root)?; }
    fs::rename(&staging, &root)?;
    fs::remove_dir_all(&temp).ok();
    println!("installed Node.js {version} at {}", root.display());
    Ok(root)
}

async fn download(client: &Client, url: &str, path: &Path) -> Result<()> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut file = fs::File::create(path)?;
    while let Some(chunk) = response.chunk().await? { file.write_all(&chunk)?; }
    Ok(())
}

fn parse_sha256(path: &Path, asset: &str) -> Result<String> {
    fs::read_to_string(path)?.lines().find_map(|line| {
        let mut p = line.split_whitespace();
        let hash = p.next()?; let name = p.next()?;
        (name == asset).then(|| hash.to_owned())
    }).context("Node.js SHA256 manifest does not contain selected asset")
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(unix)]
fn copy_dir(source: &Path, dest: &Path) -> Result<()> {
    use std::os::unix::fs::{symlink, PermissionsExt};
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?; let from = entry.path(); let to = dest.join(entry.file_name());
        let meta = fs::symlink_metadata(&from)?;
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&from)?;
            symlink(target, &to)?;
        } else if meta.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
            fs::set_permissions(&to, fs::Permissions::from_mode(meta.permissions().mode()))?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn copy_dir(source: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?; let from = entry.path(); let to = dest.join(entry.file_name());
        let meta = fs::symlink_metadata(&from)?;
        if meta.is_dir() { copy_dir(&from, &to)?; } else { fs::copy(&from, &to)?; }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn project_detection_is_file_based(){let p=std::env::temp_dir().join(format!("x11-runtime-{}",std::process::id()));fs::create_dir_all(&p).unwrap();fs::write(p.join("package.json"),"{}").unwrap();assert_eq!(detect_project(&p),vec![RuntimeKind::Node]);let _=fs::remove_dir_all(p);}
    #[test] fn version_matching_supports_major_minor(){assert!(version_matches("22.19.0","22"));assert!(version_matches("22.19.0","22.19"));assert!(!version_matches("20.10.0","22"));}
    #[test] fn parses_node_checksum(){let p=std::env::temp_dir().join(format!("x11-sums-{}",std::process::id()));fs::write(&p,"abc  node-v24.19.0-linux-x64.tar.xz\ndef  other\n").unwrap();assert_eq!(parse_sha256(&p,"node-v24.19.0-linux-x64.tar.xz").unwrap(),"abc");let _=fs::remove_file(p);}
}
