use anyhow::{Context, Result};
use std::{env, path::{Path, PathBuf}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    Off,
    Auto,
    Strict,
}

impl SandboxMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "auto" => Ok(Self::Auto),
            "strict" => Ok(Self::Strict),
            other => anyhow::bail!("unknown sandbox mode '{other}'; use off, auto, or strict"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    None,
    Bubblewrap,
    MacSeatbelt,
    WindowsRestrictedToken,
}

#[derive(Debug, Clone)]
pub struct Capability {
    pub backend: Backend,
    pub filesystem_isolation: bool,
    pub network_isolation: bool,
    pub process_isolation: bool,
    pub reason: String,
}

impl Capability {
    pub fn available(backend: Backend) -> bool {
        !matches!(backend, Backend::None)
    }
}

pub fn detect() -> Capability {
    if cfg!(target_os = "linux") {
        if find_program("bwrap").is_some() {
            return Capability {
                backend: Backend::Bubblewrap,
                filesystem_isolation: true,
                network_isolation: true,
                process_isolation: true,
                reason: "bubblewrap namespaces available".into(),
            };
        }
        return Capability {
            backend: Backend::None,
            filesystem_isolation: false,
            network_isolation: false,
            process_isolation: false,
            reason: "bubblewrap is not installed".into(),
        };
    }

    if cfg!(target_os = "macos") {
        let available = Path::new("/usr/bin/sandbox-exec").is_file();
        return Capability {
            backend: if available { Backend::MacSeatbelt } else { Backend::None },
            filesystem_isolation: available,
            network_isolation: available,
            process_isolation: available,
            reason: if available { "macOS Seatbelt executable available" } else { "macOS Seatbelt executable unavailable" }.into(),
        };
    }

    if cfg!(target_os = "windows") {
        return Capability {
            backend: Backend::WindowsRestrictedToken,
            filesystem_isolation: true,
            network_isolation: false,
            process_isolation: true,
            reason: "Windows restricted-token backend; fine-grained network isolation is not enabled yet".into(),
        };
    }

    Capability {
        backend: Backend::None,
        filesystem_isolation: false,
        network_isolation: false,
        process_isolation: false,
        reason: "no supported sandbox backend".into(),
    }
}

pub fn validate(mode: SandboxMode) -> Result<Capability> {
    let capability = detect();
    if mode == SandboxMode::Strict && !Capability::available(capability.backend) {
        anyhow::bail!("strict sandbox requested but no supported backend is available: {}", capability.reason);
    }
    Ok(capability)
}

pub fn wrap_command(mode: SandboxMode, workspace: &Path, program: &Path, args: &[String]) -> Result<(PathBuf, Vec<String>, Backend)> {
    let capability = validate(mode)?;
    match (mode, capability.backend) {
        (SandboxMode::Off, _) | (SandboxMode::Auto, Backend::None) => Ok((program.to_path_buf(), args.to_vec(), Backend::None)),
        (_, Backend::Bubblewrap) => {
            let bwrap = find_program("bwrap").context("bubblewrap disappeared after capability detection")?;
            let cwd = workspace.canonicalize().context("canonicalize sandbox workspace")?;
            let mut wrapped = vec![
                "--die-with-parent".into(),
                "--new-session".into(),
                "--unshare-pid".into(),
                "--unshare-uts".into(),
                "--unshare-ipc".into(),
                "--unshare-net".into(),
                "--proc".into(), "/proc".into(),
                "--dev".into(), "/dev".into(),
                "--ro-bind".into(), "/usr".into(), "/usr".into(),
                "--ro-bind".into(), "/bin".into(), "/bin".into(),
            ];
            if Path::new("/lib").is_dir() { wrapped.extend(["--ro-bind".into(), "/lib".into(), "/lib".into()]); }
            if Path::new("/lib64").is_dir() { wrapped.extend(["--ro-bind".into(), "/lib64".into(), "/lib64".into()]); }
            wrapped.extend(["--bind".into(), cwd.display().to_string(), cwd.display().to_string(), "--chdir".into(), cwd.display().to_string(), "--".into(), program.display().to_string()]);
            wrapped.extend(args.iter().cloned());
            Ok((bwrap, wrapped, Backend::Bubblewrap))
        }
        (_, Backend::MacSeatbelt) => {
            let sandbox_exec = PathBuf::from("/usr/bin/sandbox-exec");
            let profile = format!(
                "(version 1) (deny default) (allow process*) (allow file-read*) (allow file-write* (subpath \"{}\")) (deny network*)",
                workspace.display()
            );
            let mut wrapped = vec!["-p".into(), profile, program.display().to_string()];
            wrapped.extend(args.iter().cloned());
            Ok((sandbox_exec, wrapped, Backend::MacSeatbelt))
        }
        (_, Backend::WindowsRestrictedToken) => Ok((program.to_path_buf(), args.to_vec(), Backend::WindowsRestrictedToken)),
        (_, Backend::None) => Ok((program.to_path_buf(), args.to_vec(), Backend::None)),
    }
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for root in env::split_paths(&path) {
        let candidate = root.join(name);
        if candidate.is_file() { return Some(candidate); }
        if cfg!(windows) {
            for suffix in [".exe", ".cmd"] {
                let candidate = root.join(format!("{name}{suffix}"));
                if candidate.is_file() { return Some(candidate); }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_modes() {
        assert_eq!(SandboxMode::parse("off").unwrap(), SandboxMode::Off);
        assert_eq!(SandboxMode::parse("auto").unwrap(), SandboxMode::Auto);
        assert_eq!(SandboxMode::parse("STRICT").unwrap(), SandboxMode::Strict);
        assert!(SandboxMode::parse("banana").is_err());
    }
    #[test]
    fn strict_mode_fails_closed_without_backend() {
        if matches!(detect().backend, Backend::None) { assert!(validate(SandboxMode::Strict).is_err()); }
    }
}
