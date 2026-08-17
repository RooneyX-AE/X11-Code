use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{env, fs, path::{Path, PathBuf}, process::Command};

const API: &str = "https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest";
const SOURCE: &str = "python-build-standalone";

#[derive(Debug, Deserialize)]
struct Release { tag_name: String, assets: Vec<Asset> }
#[derive(Debug, Deserialize)]
struct Asset { name: String, browser_download_url: String, digest: Option<String> }

pub async fn install(request: &str) -> Result<()> {
    let client = Client::builder().user_agent("x11-code-runtime").build()?;
    let release = client.get(API).send().await?.error_for_status()?.json::<Release>().await?;
    let version = normalize_request(request)?;
    let asset = select_asset(&release.assets, &version).context("no compatible managed Python asset for this platform/version")?;
    let expected = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).context("Python release asset has no SHA-256 digest")?;

    let root = runtime_root().join("python").join(&version);
    let staging = runtime_root().join(format!(".python-{}-{}", version, std::process::id()));
    if staging.exists() { fs::remove_dir_all(&staging).ok(); }
    fs::create_dir_all(&staging)?;
    let archive = staging.join(&asset.name);
    let bytes = client.get(&asset.browser_download_url).send().await?.error_for_status()?.bytes().await?;
    fs::write(&archive, &bytes)?;
    verify_bytes(&archive, expected)?;

    let status = Command::new("tar").args(["-xzf", archive.to_string_lossy().as_ref(), "-C", staging.to_string_lossy().as_ref()]).status().context("extract Python archive with tar")?;
    anyhow::ensure!(status.success(), "failed to extract managed Python archive");
    let extracted = find_python_root(&staging).context("managed Python archive did not contain a python interpreter")?;

    if root.exists() { fs::remove_dir_all(&root)?; }
    if let Some(parent) = root.parent() { fs::create_dir_all(parent)?; }
    fs::rename(&extracted, &root).or_else(|_| { copy_dir(&extracted, &root)?; fs::remove_dir_all(&extracted) })?;
    let manifest = root.join(".x11-runtime.json");
    fs::write(&manifest, serde_json::json!({
        "runtime":"python", "version":version, "source":SOURCE,
        "release":release.tag_name, "asset":asset.name, "sha256":expected
    }).to_string())?;
    fs::remove_dir_all(&staging).ok();
    println!("installed managed Python {} at {}", version, root.display());
    Ok(())
}

fn normalize_request(request: &str) -> Result<String> {
    let trimmed = request.trim().trim_start_matches('v');
    anyhow::ensure!(!trimmed.is_empty(), "Python version is required");
    anyhow::ensure!(trimmed.split('.').all(|p| p.chars().all(|c| c.is_ascii_digit())), "Python version must be numeric, e.g. 3.13 or 3.13.14");
    Ok(trimmed.to_owned())
}

fn select_asset<'a>(assets: &'a [Asset], requested: &str) -> Option<&'a Asset> {
    let platform = match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        _ => return None,
    };
    let exact = requested.split('.').count() >= 3;
    assets.iter().find(|a| {
        if !a.name.contains(platform) || !a.name.contains("install_only_stripped") || !a.name.ends_with(".tar.gz") { return false; }
        let prefix = a.name.strip_prefix("cpython-")?;
        let actual = prefix.split('+').next()?;
        if exact { actual == requested } else { actual.starts_with(&format!("{}.", requested)) }
    })
}

fn verify_bytes(path: &Path, expected: &str) -> Result<()> {
    let mut file = fs::File::open(path)?; let mut h = Sha256::new(); std::io::copy(&mut file, &mut h)?;
    let actual = format!("{:x}", h.finalize());
    anyhow::ensure!(actual.eq_ignore_ascii_case(expected), "Python SHA-256 mismatch: expected {expected}, got {actual}");
    Ok(())
}

fn find_python_root(root: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let candidate = if cfg!(windows) { path.join("python.exe") } else { path.join("bin/python3") };
        if candidate.is_file() { return Some(path); }
        if let Some(nested) = find_python_root(&path) { return Some(nested); }
    }
    None
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?; let from = entry.path(); let to = dst.join(entry.file_name());
        if from.is_dir() { copy_dir(&from, &to)?; } else { fs::copy(&from, &to)?; }
    }
    Ok(())
}

fn runtime_root() -> PathBuf {
    env::var_os("X11_RUNTIME_HOME").map(PathBuf::from).unwrap_or_else(|| {
        env::var_os("X11_CONFIG_HOME").map(PathBuf::from)
            .or_else(|| env::var_os("XDG_DATA_HOME").map(|p| PathBuf::from(p).join("x11")))
            .or_else(|| env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("x11")))
            .unwrap_or_else(|| PathBuf::from(".x11"))
    }).join("runtimes")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn version_rejects_non_numeric(){ assert!(normalize_request("latest").is_err()); }
    #[test] fn version_normalizes_v_prefix(){ assert_eq!(normalize_request("v3.13").unwrap(), "3.13"); }
    #[test] fn exact_patch_does_not_match_other_patch(){
        let a = Asset { name: "cpython-3.13.14+20260718-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz".into(), browser_download_url: String::new(), digest: Some("sha256:abc".into()) };
        assert!(select_asset(&[a], "3.13.13").is_none());
    }
}
