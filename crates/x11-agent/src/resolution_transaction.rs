use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub existed: bool,
    pub content: Vec<u8>,
}

pub struct ResolutionTransaction;

impl ResolutionTransaction {
    pub async fn snapshot_file(workspace: &Path, relative: &Path) -> Result<FileSnapshot> {
        validate_relative(relative)?;
        let root = fs::canonicalize(workspace).await.context("canonicalize resolution workspace")?;
        let candidate = root.join(relative);
        let existed = fs::try_exists(&candidate).await.context("check resolution target")?;
        let path = if existed {
            let canonical = fs::canonicalize(&candidate).await.context("canonicalize resolution target")?;
            if !canonical.starts_with(&root) { anyhow::bail!("resolution path escapes workspace via symlink"); }
            canonical
        } else {
            let parent = candidate.parent().context("resolution target has no parent")?;
            let canonical_parent = fs::canonicalize(parent).await.context("canonicalize resolution parent")?;
            if !canonical_parent.starts_with(&root) { anyhow::bail!("resolution parent escapes workspace via symlink"); }
            canonical_parent.join(candidate.file_name().context("resolution target has no filename")?)
        };
        let content = if existed { fs::read(&path).await.context("read resolution target")? } else { Vec::new() };
        Ok(FileSnapshot { path, existed, content })
    }

    pub async fn rollback(snapshot: &FileSnapshot) -> Result<()> {
        if snapshot.existed {
            if let Some(parent) = snapshot.path.parent() { fs::create_dir_all(parent).await.context("restore resolution parent")?; }
            fs::write(&snapshot.path, &snapshot.content).await.context("restore resolution file")?;
        } else if fs::try_exists(&snapshot.path).await.context("check rollback target")? {
            fs::remove_file(&snapshot.path).await.context("remove newly created resolution file")?;
        }
        Ok(())
    }

    pub async fn verify_unchanged(snapshot: &FileSnapshot) -> Result<bool> {
        let exists = fs::try_exists(&snapshot.path).await.context("check snapshot target")?;
        if exists != snapshot.existed { return Ok(false); }
        if !exists { return Ok(true); }
        Ok(fs::read(&snapshot.path).await.context("read snapshot target")? == snapshot.content)
    }
}

fn validate_relative(path: &Path) -> Result<()> {
    if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) { anyhow::bail!("resolution path escapes workspace"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::Permissions;

    #[test]
    fn rejects_parent_escape() {
        assert!(validate_relative(Path::new("../secret")).is_err());
        assert!(validate_relative(Path::new("src/main.rs")).is_ok());
    }

    #[tokio::test]
    async fn rejects_symlink_escape_for_existing_target() {
        let root = std::env::temp_dir().join(format!("x11-tx-{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!("x11-out-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).await.unwrap();
        fs::create_dir_all(&outside).await.unwrap();
        fs::write(outside.join("secret.txt"), "secret").await.unwrap();
        #[cfg(unix)] std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        #[cfg(unix)] assert!(ResolutionTransaction::snapshot_file(&root, Path::new("link/secret.txt")).await.is_err());
        let _ = fs::remove_dir_all(&root).await;
        let _ = fs::remove_dir_all(&outside).await;
    }
}
