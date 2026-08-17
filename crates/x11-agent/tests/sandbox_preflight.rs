use std::{process::Command};

#[test]
fn bubblewrap_preflight_if_available() {
    if !cfg!(target_os = "linux") { return; }

    let Some(bwrap) = find_in_path("bwrap") else {
        eprintln!("sandbox preflight: bwrap not installed; skipping");
        return;
    };

    let output = Command::new(&bwrap)
        .args([
            "--die-with-parent", "--new-session", "--unshare-pid",
            "--unshare-uts", "--unshare-ipc", "--unshare-net",
            "--proc", "/proc", "--dev", "/dev",
            "--ro-bind", "/usr", "/usr",
            "--ro-bind", "/bin", "/bin",
            "--", "/bin/true",
        ])
        .output()
        .expect("failed to spawn bubblewrap preflight");

    assert!(
        output.status.success(),
        "bubblewrap is present but preflight failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn find_in_path(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then(|| candidate.to_string_lossy().into_owned())
    })
}
