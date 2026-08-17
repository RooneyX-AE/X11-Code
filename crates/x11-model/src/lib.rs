pub mod router;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall { pub id:String, pub name:String, pub arguments:serde_json::Value }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest { pub model:String, pub messages:Vec<Message>, #[serde(default)] pub tools:Vec<serde_json::Value>, pub temperature:Option<f32>, pub max_tokens:Option<u32> }
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage { pub input_tokens:u32, pub output_tokens:u32 }
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionResponse { pub text:String, pub tool_calls:Vec<ToolCall>, pub finish_reason:Option<String>, pub usage:Usage }

#[async_trait]
pub trait ModelProvider:Send+Sync { fn name(&self)->&'static str; async fn complete(&self,request:CompletionRequest)->Result<CompletionResponse>; }

pub struct MockProvider;
#[async_trait]
impl ModelProvider for MockProvider {
    fn name(&self)->&'static str{"mock"}
    async fn complete(&self,request:CompletionRequest)->Result<CompletionResponse>{let goal=request.messages.iter().rev().find(|m|m.role=="user").map(|m|m.content.clone()).unwrap_or_default();Ok(CompletionResponse{text:format!("Mock provider received: {goal}"),finish_reason:Some("stop".into()),..Default::default()})}
}

#[derive(Clone)]
pub struct OpenAiCompatible { pub base_url:String, pub api_key:String, pub client:Client, pub max_retries:u32 }
impl OpenAiCompatible {
    pub fn new(base_url:impl Into<String>,api_key:impl Into<String>)->Self{let client=Client::builder().timeout(Duration::from_secs(120)).connect_timeout(Duration::from_secs(15)).build().unwrap_or_else(|_|Client::new());Self{base_url:base_url.into(),api_key:api_key.into(),client,max_retries:2}}
    fn retryable(status:StatusCode)->bool { status==StatusCode::TOO_MANY_REQUESTS || status.is_server_error() }
}

#[derive(Debug, Deserialize)] struct ChatEnvelope { choices:Vec<ChatChoice>, #[serde(default)] usage:Option<UsageEnvelope> }
#[derive(Debug, Deserialize)] struct ChatChoice { message:ChatMessage, finish_reason:Option<String> }
#[derive(Debug, Deserialize)] struct ChatMessage { #[serde(default)] content:Option<String>, #[serde(default)] tool_calls:Vec<RawToolCall> }
#[derive(Debug, Deserialize)] struct RawToolCall { id:Option<String>, function:RawFunction }
#[derive(Debug, Deserialize)] struct RawFunction { name:Option<String>, arguments:Option<String> }
#[derive(Debug, Deserialize)] struct UsageEnvelope { prompt_tokens:Option<u64>, completion_tokens:Option<u64> }

#[async_trait]
impl ModelProvider for OpenAiCompatible {
    fn name(&self)->&'static str{"openai-compatible"}
    async fn complete(&self,r:CompletionRequest)->Result<CompletionResponse>{
        let tools=if r.tools.is_empty(){None}else{Some(r.tools.iter().map(|t|serde_json::json!({"type":"function","function":{"name":t["name"],"description":t["description"],"parameters":t["input_schema"]}})).collect::<Vec<_>>())};
        let body=serde_json::json!({"model":r.model,"messages":r.messages,"temperature":r.temperature,"max_tokens":r.max_tokens,"tools":tools});
        let url=format!("{}/chat/completions",self.base_url.trim_end_matches('/'));let mut attempt=0u32;
        let response=loop{let response=self.client.post(&url).bearer_auth(&self.api_key).json(&body).send().await.context("model request failed")?;if response.status().is_success(){break response;}let status=response.status();if !Self::retryable(status)||attempt>=self.max_retries{let body=response.text().await.unwrap_or_default();anyhow::bail!("model API returned {}: {}",status,body.chars().take(4000).collect::<String>())}attempt+=1;tokio::time::sleep(Duration::from_millis(250u64.saturating_mul(1u64<<attempt.min(4)))).await};
        let envelope=response.json::<ChatEnvelope>().await.context("invalid model response JSON")?;let choice=envelope.choices.first().context("model response contained no choices")?;let mut calls=Vec::new();
        for c in &choice.message.tool_calls{let id=c.id.clone().unwrap_or_default();let name=c.function.name.clone().unwrap_or_default();if id.is_empty()||name.is_empty(){anyhow::bail!("model returned malformed tool call")}let raw=c.function.arguments.as_deref().unwrap_or("{}");let arguments=serde_json::from_str(raw).with_context(||format!("invalid JSON tool arguments for {name}"))?;calls.push(ToolCall{id,name,arguments});}
        let usage=envelope.usage.unwrap_or(UsageEnvelope{prompt_tokens:None,completion_tokens:None});Ok(CompletionResponse{text:choice.message.content.clone().unwrap_or_default(),tool_calls:calls,finish_reason:choice.finish_reason.clone(),usage:Usage{input_tokens:usage.prompt_tokens.unwrap_or(0).min(u32::MAX as u64) as u32,output_tokens:usage.completion_tokens.unwrap_or(0).min(u32::MAX as u64) as u32}})
    }
}

#[cfg(test)]
mod tests{use super::*;#[tokio::test]async fn mock_provider_is_deterministic(){let p=MockProvider;let r=p.complete(CompletionRequest{model:"m".into(),messages:vec![Message{role:"user".into(),content:"hello".into(),..Default::default()}],tools:vec![],temperature:None,max_tokens:None}).await.unwrap();assert_eq!(r.text,"Mock provider received: hello");assert!(r.tool_calls.is_empty());}#[test]fn tool_message_roundtrips(){let message=Message{role:"assistant".into(),content:String::new(),tool_calls:vec![ToolCall{id:"1".into(),name:"read_file".into(),arguments:serde_json::json!({"path":"a"})}],tool_call_id:None};let json=serde_json::to_string(&message).unwrap();let decoded:Message=serde_json::from_str(&json).unwrap();assert_eq!(decoded.tool_calls[0].id,"1");}#[test]fn retry_policy_covers_transient_failures(){assert!(OpenAiCompatible::retryable(StatusCode::TOO_MANY_REQUESTS));assert!(OpenAiCompatible::retryable(StatusCode::BAD_GATEWAY));assert!(!OpenAiCompatible::retryable(StatusCode::BAD_REQUEST));}}
