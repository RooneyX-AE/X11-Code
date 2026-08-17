use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::{CompletionRequest, CompletionResponse, ModelProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind { Planning, Coding, Review, Testing, Subagent, General }

#[derive(Debug, Clone)]
pub struct RoutePolicy {
    pub planning: String,
    pub coding: String,
    pub review: String,
    pub testing: String,
    pub subagent: String,
    pub general: String,
}

impl Default for RoutePolicy {
    fn default() -> Self {
        Self {
            planning: "default".into(), coding: "default".into(), review: "default".into(),
            testing: "default".into(), subagent: "default".into(), general: "default".into(),
        }
    }
}

impl RoutePolicy {
    pub fn model_for(&self, task: TaskKind) -> &str {
        match task {
            TaskKind::Planning => &self.planning,
            TaskKind::Coding => &self.coding,
            TaskKind::Review => &self.review,
            TaskKind::Testing => &self.testing,
            TaskKind::Subagent => &self.subagent,
            TaskKind::General => &self.general,
        }
    }
}

#[derive(Clone)]
pub struct ModelRouter<P: ModelProvider> {
    provider: Arc<P>,
    policy: RoutePolicy,
}

impl<P: ModelProvider> ModelRouter<P> {
    pub fn new(provider: Arc<P>, policy: RoutePolicy) -> Self { Self { provider, policy } }
    pub fn provider(&self) -> Arc<P> { self.provider.clone() }
    pub fn policy(&self) -> &RoutePolicy { &self.policy }

    pub async fn complete(&self, task: TaskKind, mut request: CompletionRequest) -> Result<CompletionResponse> {
        request.model = self.policy.model_for(task).to_owned();
        self.provider.complete(request).await
    }
}

#[async_trait]
pub trait RoutedModel: Send + Sync {
    async fn complete_for(&self, task: TaskKind, request: CompletionRequest) -> Result<CompletionResponse>;
}

#[async_trait]
impl<P: ModelProvider + 'static> RoutedModel for ModelRouter<P> {
    async fn complete_for(&self, task: TaskKind, request: CompletionRequest) -> Result<CompletionResponse> {
        self.complete(task, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, MockProvider};

    #[test]
    fn route_policy_selects_task_model() {
        let policy = RoutePolicy { coding: "coder-fast".into(), review: "reviewer-strong".into(), ..Default::default() };
        assert_eq!(policy.model_for(TaskKind::Coding), "coder-fast");
        assert_eq!(policy.model_for(TaskKind::Review), "reviewer-strong");
    }

    #[tokio::test]
    async fn router_overrides_request_model() {
        let policy = RoutePolicy { general: "routed".into(), ..Default::default() };
        let router = ModelRouter::new(Arc::new(MockProvider), policy);
        let result = router.complete(TaskKind::General, CompletionRequest { model: "ignored".into(), messages: vec![Message { role: "user".into(), content: "hello".into(), ..Default::default() }], tools: vec![], temperature: None, max_tokens: None }).await.unwrap();
        assert_eq!(result.text, "Mock provider received: hello");
    }
}
