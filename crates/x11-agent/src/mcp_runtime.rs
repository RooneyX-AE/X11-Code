use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::{env, path::{Path, PathBuf}};
use crate::{mcp_tools::connect_mcp_server, AgentRuntime};
use x11_mcp::McpRegistry;
use x11_model::ModelProvider;

impl<P: ModelProvider + 'static> AgentRuntime<P> {
    pub async fn load_mcp_file(&mut self, path: impl AsRef<Path>) -> Result<usize> {
        let path = path.as_ref();
        let value: Value = serde_json::from_str(&tokio::fs::read_to_string(path).await?)
            .with_context(|| format!("invalid MCP config: {}", path.display()))?;
        self.load_mcp_value(value).await
    }

    pub async fn load_mcp_configs(&mut self, workspace: &Path) -> Result<usize> {
        if let Some(path) = env::var_os("X11_MCP_CONFIG").map(PathBuf::from).filter(|p| p.is_file()) {
            return self.load_mcp_file(path).await;
        }

        let mut merged = Map::new();
        if let Some(user_path) = user_mcp_config_path() {
            if user_path.is_file() {
                let value: Value = serde_json::from_str(&tokio::fs::read_to_string(&user_path).await?)
                    .with_context(|| format!("invalid user MCP config: {}", user_path.display()))?;
                merge_servers(&mut merged, &value)?;
            }
        }

        let project_path = workspace.join(".x11").join("mcp.json");
        if project_path.is_file() {
            let value: Value = serde_json::from_str(&tokio::fs::read_to_string(&project_path).await?)
                .with_context(|| format!("invalid project MCP config: {}", project_path.display()))?;
            merge_servers(&mut merged, &value)?;
        }

        if merged.is_empty() { return Ok(0); }
        let mut root = Map::new();
        root.insert("mcpServers".into(), Value::Object(merged));
        self.load_mcp_value(Value::Object(root)).await
    }

    async fn load_mcp_value(&mut self, value: Value) -> Result<usize> {
        let registry = McpRegistry::from_json(value)?;
        let mut added = 0usize;
        for server in registry.enabled() {
            let adapters = connect_mcp_server(server).await
                .with_context(|| format!("failed to load MCP server {}", server.name))?;
            for adapter in adapters {
                self.tools.register(adapter);
                added += 1;
            }
        }
        self.executor = self.executor.with_registry(self.tools.clone());
        Ok(added)
    }
}

fn merge_servers(target: &mut Map<String, Value>, value: &Value) -> Result<()> {
    let Some(servers) = value.get("mcpServers") else { return Ok(()); };
    let object = servers.as_object().context("MCP mcpServers must be an object")?;
    for (name, config) in object { target.insert(name.clone(), config.clone()); }
    Ok(())
}

fn user_mcp_config_path() -> Option<PathBuf> {
    if let Some(root) = env::var_os("X11_CONFIG_HOME") { return Some(PathBuf::from(root).join("mcp.json")); }
    if cfg!(windows) {
        env::var_os("APPDATA").map(PathBuf::from).map(|p| p.join("x11-code").join("mcp.json"))
    } else {
        env::var_os("HOME").map(PathBuf::from).map(|p| p.join(".config").join("x11-code").join("mcp.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_server_overrides_user_server() {
        let mut merged = Map::new();
        merge_servers(&mut merged, &serde_json::json!({"mcpServers":{"github":{"command":"user-github"},"shared":{"command":"user-shared"}}})).unwrap();
        merge_servers(&mut merged, &serde_json::json!({"mcpServers":{"shared":{"command":"project-shared"}}})).unwrap();
        assert_eq!(merged["github"]["command"], "user-github");
        assert_eq!(merged["shared"]["command"], "project-shared");
    }
}
