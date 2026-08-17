use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

use crate::{ToolContext, ToolRegistry};

#[derive(Debug, Clone)]
pub struct ToolExecutionConfig {
    pub timeout: Duration,
    pub max_attempts: u32,
}

impl Default for ToolExecutionConfig {
    fn default() -> Self { Self { timeout: Duration::from_secs(120), max_attempts: 1 } }
}

#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub output: String,
    pub attempts: u32,
}

#[derive(Clone)]
pub struct ToolExecutor {
    registry: Arc<ToolRegistry>,
    config: ToolExecutionConfig,
}

impl ToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>, config: ToolExecutionConfig) -> Self {
        Self { registry, config }
    }

    pub async fn execute(&self, context: &ToolContext, name: &str, input: Value) -> Result<ToolExecutionResult> {
        let attempts = self.config.max_attempts.max(1);
        let mut last_error = None;
        for attempt in 1..=attempts {
            match timeout(self.config.timeout, self.registry.execute(context, name, input.clone())).await {
                Ok(Ok(output)) => return Ok(ToolExecutionResult { output, attempts: attempt }),
                Ok(Err(error)) => last_error = Some(error),
                Err(_) => last_error = Some(anyhow::anyhow!("tool '{}' timed out after {:?}", name, self.config.timeout)),
            }
            if attempt < attempts {
                tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("tool '{}' failed", name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_conservative() {
        let cfg = ToolExecutionConfig::default();
        assert_eq!(cfg.max_attempts, 1);
        assert!(cfg.timeout >= Duration::from_secs(1));
    }
}
