use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{env, fs, path::{Path, PathBuf}};

const REPO: &str = "RooneyX-AE/X11-Code";

#[derive(Debug, Deserialize)]
struct Release { tag_name: String, assets: Vec<Asset> }

#[derive(Debug, Deserialize)]
struct Asset { name: String, browser_download_url: String }

pub async fn run(check_only: bool) -> Result<()> {
    let client = Client::builder().user_agent("x11-code-updater").build()?;
    let release = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .send().await.context("request latest X11 release")?
        .error_for_status().context("latest X11 release request failed")?
        .json::<Release>().await.context("decode latest X11 release")?;

    let current = env!("CARGO_PKG_VERSION");
    let latest = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
    println!("current: v{current}");
    println!("latest:  {}", release.tag_name);
    if latest == current {
        println!("X11 Code is already up to date.");
        return Ok(());
    }
    if check_only { return Ok(()); }

    let asset = asset_for_current_platform(&release.assets)
        .context("no release asset for this operating system/architecture")?;
    let sums = release.assets.iter().find(|a| a.name == "SHA256SUMS")
        .context("release is missing SHA256SUMS")?;

    let temp_root = env::temp_dir().join(format!("x11-update-{}", std::process::id()));
    if temp_root.exists() { fs::remove_dir_all(&temp_root).ok(); }
    fs::create_dir_all(&temp_root)?;

    let archive = temp_root.join(&asset.name);
    let sums_path = temp_root.join("SHA256SUMS");
    download(&client, &asset.browser_download_url, &archive).await?;
    download(&client, &sums.browser_download_url, &sums_path).await?;

    verify_sha256(&archive, &sums_path, &asset.name)?;
    let binary = extract_binary(&archive, &temp_root)?;
    let current_exe = env::current_exe().context("locate current executable")?;
    println!("verified {}; replacing {}", asset.name, current_exe.display());

    replace_current_binary(&binary, &current_exe).context("replace current executable")?;
    fs::remove_dir_all(&temp_root).ok();
    println!("updated to {}", release.tag_name);
    Ok(())
}

async fn download(client: &Client, url: &str, path: &Path) -> Result<()> {
    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    fs::write(path, bytes)?;
    Ok(())
}

fn asset_for_current_platform<'a>(assets: &'a [Asset]) -> Option<&'a Asset> {
    let suffix = match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu.tar.gz",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu.tar.gz",
        ("macos", "x86_64") => "x86_64-apple-darwin.tar.gz",
        ("macos", "aarch64") => "aarch64-apple-darwin.tar.gz",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc.zip",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc.zip",
        _ => return None,
    };
    assets.iter().find(|a| a.name.ends_with(suffix))
}

fn verify_sha256(path: &Path, sums_path: &Path, asset_name: &str) -> Result<()> {
    let expected = fs::read_to_string(sums_path)?
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?.trim_start_matches("*");
            (name == asset_name).then_some(hash.to_ascii_lowercase())
        })
        .context("release SHA256SUMS does not contain selected asset")?;

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let actual = format!("{:x}", hasher.finalize());
    anyhow::ensure!(actual == expected, "SHA-256 mismatch: expected {expected}, got {actual}");
    Ok(())
}

fn extract_binary(archive: &Path, root: &Path) -> Result<PathBuf> {
    if archive.extension().and_then(|x| x.to_str()) == Some("zip") {
        #[cfg(windows)] {
            let status = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force", archive.display(), root.display())])
                .status().context("spawn PowerShell")?;
            anyhow::ensure!(status.success(), "failed to extract release archive");
            return Ok(root.join("x11.exe"));
        }
        anyhow::bail!("zip release cannot be extracted on this platform")
    }
    let status = std::process::Command::new("tar")
        .args(["-xzf", archive.to_string_lossy().as_ref(), "-C", root.to_string_lossy().as_ref()])
        .status().context("spawn tar")?;
    anyhow::ensure!(status.success(), "failed to extract release archive");
    Ok(root.join("x11"))
}

fn replace_current_binary(new_binary: &Path, current: &Path) -> Result<()> {
    let replacement = fs::canonicalize(new_binary).context("resolve downloaded binary")?;
    self_replace::self_replace(&replacement).context("self replacement failed")?;
    let _ = current;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_asset_names_are_unambiguous() {
        let assets = vec![
            Asset { name: "x11-x86_64-unknown-linux-gnu.tar.gz".into(), browser_download_url: "".into() },
            Asset { name: "x11-aarch64-unknown-linux-gnu.tar.gz".into(), browser_download_url: "".into() },
        ];
        let selected = match (env::consts::OS, env::consts::ARCH) {
            ("linux", "x86_64") => asset_for_current_platform(&assets).unwrap().name.clone(),
            _ => "".into(),
        };
        if !selected.is_empty() { assert!(selected.contains("x86_64-unknown-linux-gnu")); }
    }
}
