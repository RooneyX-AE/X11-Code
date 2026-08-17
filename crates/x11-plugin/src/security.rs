use anyhow::Result;
use std::path::Path;

use crate::Plugin;

/// Plugin metadata never grants execution permission. The host policy must
/// authorize each hook before the command is run.
pub fn validate_hook_command(plugin: &Plugin, command: &str) -> Result<()> {
    if command.trim().is_empty() {
        anyhow::bail!("plugin hook command cannot be empty");
    }
    if command.len() > 16_000 {
        anyhow::bail!("plugin hook command is too long");
    }
    let root = Path::new(&plugin.root);
    if !root.is_dir() {
        anyhow::bail!("plugin root no longer exists");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Plugin, PluginManifest};
    use std::{fs, path::PathBuf};

    #[test]
    fn hook_metadata_does_not_execute() {
        let root = std::env::temp_dir().join(format!("x11-plugin-security-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let plugin = Plugin { root: PathBuf::from(&root), manifest: PluginManifest { name: "demo".into(), version: "1".into(), ..Default::default() } };
        assert!(validate_hook_command(&plugin, "printf test").is_ok());
        assert!(validate_hook_command(&plugin, "").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
