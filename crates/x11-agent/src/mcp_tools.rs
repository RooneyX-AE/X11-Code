use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use x11_mcp::{McpClient, McpServerConfig, McpTool};
use x11_tools::{Tool, ToolContext, ToolKind};

pub struct McpToolAdapter {
    qualified_name: String,
    description: String,
    input_schema: Value,
    client: Arc<Mutex<McpClient>>,
    remote_name: String,
}

impl McpToolAdapter {
    pub fn new(server: &str, tool: &McpTool, client: Arc<Mutex<McpClient>>) -> Self {
        Self {
            qualified_name: tool.qualified_name(server),
            description: tool.description.clone().unwrap_or_else(|| format!("MCP tool {}", tool.name)),
            input_schema: tool.input_schema.clone(),
            client,
            remote_name: tool.name.clone(),
        }
    }

    pub fn qualified_name(&self) -> &str { &self.qualified_name }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn name(&self) -> &str { &self.qualified_name }
    fn description(&self) -> &str { &self.description }
    fn kind(&self) -> ToolKind { ToolKind::Network }
    fn input_schema(&self) -> Value { self.input_schema.clone() }

    async fn execute(&self, _ctx: &ToolContext, input: Value) -> Result<String> {
        let mut client = self.client.lock().await;
        let result = client.call_tool(&self.remote_name, input).await
            .with_context(|| format!("MCP tool {} failed", self.qualified_name))?;
        Ok(result.to_string())
    }
}

pub async fn connect_mcp_server(config: &McpServerConfig) -> Result<Vec<McpToolAdapter>> {
    let mut client = McpClient::spawn(config).await?;
    client.initialize().await.context("MCP initialize failed")?;
    let tools = client.list_tools().await.context("MCP tools/list failed")?;
    let shared = Arc::new(Mutex::new(client));
    Ok(tools.iter().map(|tool| McpToolAdapter::new(&config.name, tool, shared.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_kind_is_network() {
        let tool = McpTool { name: "create_issue".into(), description: Some("Create issue".into()), input_schema: serde_json::json!({"type":"object"}) };
        assert_eq!(tool.qualified_name("github"), "mcp__github__create_issue");
    }
}
