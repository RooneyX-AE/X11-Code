use anyhow::{Context, Result};
use std::{env, fs, path::PathBuf};

pub fn run(purge_config: bool) -> Result<()> {
    let current = env::current_exe().context("locate current executable")?;
    let parent = current.parent().map(PathBuf::from).context("locate executable directory")?;
    println!("installation: {}", current.display());

    if purge_config {
        let config = config_dir();
        if config.exists() {
            fs::remove_dir_all(&config).with_context(|| format!("remove X11 config {}", config.display()))?;
            println!("removed user config: {}", config.display());
        } else {
            println!("user config already absent: {}", config.display());
        }
    } else {
        println!("user data preserved: {}", config_dir().display());
    }

    println!("remove the executable from {}", parent.display());
    if cfg!(windows) {
        let script = parent.join("x11-uninstall.ps1");
        let script_body = format!(
            "$ErrorActionPreference='Stop'\nStart-Sleep -Milliseconds 500\nRemove-Item -Force -LiteralPath '{}'\nRemove-Item -Force -LiteralPath $PSCommandPath\n",
            current.display()
        );
        fs::write(&script, script_body)?;
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-File", script.to_string_lossy().as_ref()])
            .spawn()
            .context("spawn Windows uninstall helper")?;
    } else {
        fs::remove_file(&current).with_context(|| format!("remove {}", current.display()))?;
        println!("removed executable");
    }
    Ok(())
}

fn config_dir() -> PathBuf {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("x11");
    }
    if cfg!(windows) {
        return env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("." )).join("x11");
    }
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".config/x11")
}
