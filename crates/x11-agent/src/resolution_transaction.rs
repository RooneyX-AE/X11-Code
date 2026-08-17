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
        let path = workspace.join(relative);
        let existed = fs::try_exists(&path).await.context("check resolution target")?;
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
    #[test]
    fn rejects_parent_escape() {
        assert!(validate_relative(Path::new("../secret")).is_err());
        assert!(validate_relative(Path::new("src/main.rs")).is_ok());
    }
}
