use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use crate::{mcp_tools::connect_mcp_server, AgentRuntime};
use x11_mcp::McpRegistry;
use x11_model::ModelProvider;

impl<P: ModelProvider + 'static> AgentRuntime<P> {
    pub async fn load_mcp_file(&mut self, path: impl AsRef<Path>) -> Result<usize> {
        let path = path.as_ref();
        let value: Value = serde_json::from_str(&tokio::fs::read_to_string(path).await?)
            .with_context(|| format!("invalid MCP config: {}", path.display()))?;
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
        self.executor = crate::tool_executor::ToolExecutor::new(self.tools.clone(), self.config.workspace.clone());
        Ok(added)
    }
}
