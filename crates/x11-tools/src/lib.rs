use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace: std::path::PathBuf,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    async fn execute(&self, ctx: &ToolContext, input: Value) -> Result<String>;
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_owned(), Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub async fn execute(&self, ctx: &ToolContext, name: &str, input: Value) -> Result<String> {
        let tool = self.tools.get(name).ok_or_else(|| anyhow::anyhow!("unknown tool: {name}"))?;
        let _call_id = Uuid::new_v4();
        tool.execute(ctx, input).await
    }
}

pub struct ReadFile;

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &'static str { "read_file" }
    fn description(&self) -> &'static str { "Read a UTF-8 file inside the workspace." }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> Result<String> {
        let relative = input.get("path").and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing path"))?;
        let path = ctx.workspace.join(relative);
        let canonical_workspace = tokio::fs::canonicalize(&ctx.workspace).await?;
        let canonical_path = tokio::fs::canonicalize(&path).await?;
        if !canonical_path.starts_with(&canonical_workspace) {
            anyhow::bail!("path escapes workspace");
        }
        Ok(tokio::fs::read_to_string(canonical_path).await?)
    }
}
