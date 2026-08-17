use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::{time::Duration, sync::atomic::{AtomicU64, Ordering}};

const MAX_MODEL_NAME: usize = 256;
const MAX_TOOL_CALL_ARGS: usize = 256_000;
const MAX_RESPONSE_TEXT: usize = 2_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireFunction { pub name: String, pub arguments: String }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireToolCall { pub id: String, #[serde(rename = "type")] pub kind: String, pub function: WireFunction }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub tool_calls: Vec<WireToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub tool_call_id: Option<String>,
}
impl Message {
    pub fn system(content: impl Into<String>) -> Self { Self { role:"system".into(), content:content.into(), tool_calls:Vec::new(), tool_call_id:None } }
    pub fn user(content: impl Into<String>) -> Self { Self { role:"user".into(), content:content.into(), tool_calls:Vec::new(), tool_call_id:None } }
    pub fn assistant(content: impl Into<String>) -> Self { Self { role:"assistant".into(), content:content.into(), tool_calls:Vec::new(), tool_call_id:None } }
    pub fn assistant_with_tools(content: impl Into<String>, calls: &[ToolCall]) -> Self { Self { role:"assistant".into(), content:content.into(), tool_calls:calls.iter().map(WireToolCall::from_tool_call).collect(), tool_call_id:None } }
    pub fn tool(call_id: impl Into<String>, content: impl Into<String>) -> Self { Self { role:"tool".into(), content:content.into(), tool_calls:Vec::new(), tool_call_id:Some(call_id.into()) } }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall { pub id:String, pub name:String, pub arguments:serde_json::Value }
impl WireToolCall { fn from_tool_call(call:&ToolCall)->Self{Self{id:call.id.clone(),kind:"function".into(),function:WireFunction{name:call.name.clone(),arguments:serde_json::to_string(&call.arguments).unwrap_or_else(|_|"{}".into())}}} }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest { pub model:String, pub messages:Vec<Message>, #[serde(default)] pub tools:Vec<serde_json::Value>, pub temperature:Option<f32>, pub max_tokens:Option<u32> }
impl CompletionRequest {
    pub fn validate(&self)->Result<()> {
        if self.model.trim().is_empty(){anyhow::bail!("model name cannot be empty")}
        if self.model.len()>MAX_MODEL_NAME{anyhow::bail!("model name is too long")}
        if self.messages.is_empty(){anyhow::bail!("completion request requires at least one message")}
        for message in &self.messages{if message.role.trim().is_empty(){anyhow::bail!("message role cannot be empty")}}
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage { pub input_tokens:u32, pub output_tokens:u32 }
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionResponse { pub text:String, pub tool_calls:Vec<ToolCall>, pub finish_reason:Option<String>, pub usage:Usage }
#[async_trait]
pub trait ModelProvider:Send+Sync { fn name(&self)->&'static str; async fn complete(&self,request:CompletionRequest)->Result<CompletionResponse>; }
pub struct MockProvider;
#[async_trait]
impl ModelProvider for MockProvider { fn name(&self)->&'static str{"mock"} async fn complete(&self,request:CompletionRequest)->Result<CompletionResponse>{request.validate()?;let goal=request.messages.iter().rev().find(|m|m.role=="user").map(|m|m.content.clone()).unwrap_or_default();Ok(CompletionResponse{text:format!("Mock provider received: {goal}"),finish_reason:Some("stop".into()),..Default::default()})} }
#[derive(Clone)]
pub struct OpenAiCompatible { pub base_url:String, pub api_key:String, pub client:Client, pub max_retries:u32, request_seq:std::sync::Arc<AtomicU64> }
impl OpenAiCompatible {
    pub fn new(base_url:impl Into<String>,api_key:impl Into<String>)->Self{let base=base_url.into();let client=Client::builder().timeout(Duration::from_secs(120)).connect_timeout(Duration::from_secs(15)).build().unwrap_or_else(|_|Client::new());Self{base_url:base,api_key:api_key.into(),client,max_retries:2,request_seq:std::sync::Arc::new(AtomicU64::new(1))}}
    fn retryable(status:StatusCode)->bool{matches!(status,StatusCode::TOO_MANY_REQUESTS|StatusCode::BAD_GATEWAY|StatusCode::SERVICE_UNAVAILABLE|StatusCode::GATEWAY_TIMEOUT)}
    fn endpoint(&self)->Result<String>{let base=self.base_url.trim();if base.is_empty(){anyhow::bail!("model base URL cannot be empty")}let url=reqwest::Url::parse(base).context("invalid model base URL")?;if !matches!(url.scheme(),"http"|"https"){anyhow::bail!("model base URL must use http or https")};Ok(format!("{}/chat/completions",base.trim_end_matches('/')))}
}
#[derive(Debug,Deserialize)] struct ChatEnvelope{choices:Vec<ChatChoice>,#[serde(default)]usage:Option<UsageEnvelope>}
#[derive(Debug,Deserialize)] struct ChatChoice{message:ChatMessage,finish_reason:Option<String>}
#[derive(Debug,Deserialize)] struct ChatMessage{#[serde(default)]content:Option<String>,#[serde(default)]tool_calls:Vec<RawToolCall>}
#[derive(Debug,Deserialize)] struct RawToolCall{id:Option<String>,function:RawFunction}
#[derive(Debug,Deserialize)] struct RawFunction{name:Option<String>,arguments:Option<String>}
#[derive(Debug,Deserialize)] struct UsageEnvelope{prompt_tokens:Option<u64>,completion_tokens:Option<u64>}
#[async_trait]
impl ModelProvider for OpenAiCompatible {
    fn name(&self)->&'static str{"openai-compatible"}
    async fn complete(&self,r:CompletionRequest)->Result<CompletionResponse>{
        r.validate()?;let url=self.endpoint()?;let tools=if r.tools.is_empty(){None}else{Some(r.tools.iter().map(|t|serde_json::json!({"type":"function","function":{"name":t["name"],"description":t["description"],"parameters":t["input_schema"]}})).collect::<Vec<_>>())};let body=serde_json::json!({"model":r.model,"messages":r.messages,"temperature":r.temperature,"max_tokens":r.max_tokens,"tools":tools});
        let mut attempt=0u32;let response=loop{let request_id=self.request_seq.fetch_add(1,Ordering::Relaxed);let response=self.client.post(&url).header("x-x11-request-id",request_id.to_string()).bearer_auth(&self.api_key).json(&body).send().await.context("model request failed")?;if response.status().is_success(){break response;}let status=response.status();if !Self::retryable(status)||attempt>=self.max_retries{let body=response.text().await.unwrap_or_default();let body=body.chars().take(4000).collect::<String>();anyhow::bail!("model API returned {}: {}",status,body)}attempt+=1;let backoff=250u64.saturating_mul(1u64<<attempt.min(4));tokio::time::sleep(Duration::from_millis(backoff)).await;};
        let envelope=response.json::<ChatEnvelope>().await.context("invalid model response JSON")?;let choice=envelope.choices.first().context("model response contained no choices")?;let mut calls=Vec::new();for c in &choice.message.tool_calls{let id=c.id.clone().unwrap_or_default();let name=c.function.name.clone().unwrap_or_default();if id.is_empty()||name.is_empty(){anyhow::bail!("model returned malformed tool call")}let raw=c.function.arguments.as_deref().unwrap_or("{}");if raw.len()>MAX_TOOL_CALL_ARGS{anyhow::bail!("tool arguments exceed safety limit")};let arguments=serde_json::from_str(raw).with_context(||format!("invalid JSON tool arguments for {name}"))?;if !arguments.is_object(){anyhow::bail!("tool arguments for {name} must be a JSON object")};calls.push(ToolCall{id,name,arguments});}
        let text=choice.message.content.clone().unwrap_or_default();if text.len()>MAX_RESPONSE_TEXT{anyhow::bail!("model response text exceeds safety limit")};let usage=envelope.usage.unwrap_or(UsageEnvelope{prompt_tokens:None,completion_tokens:None});Ok(CompletionResponse{text,tool_calls:calls,finish_reason:choice.finish_reason.clone(),usage:Usage{input_tokens:usage.prompt_tokens.unwrap_or(0).min(u32::MAX as u64) as u32,output_tokens:usage.completion_tokens.unwrap_or(0).min(u32::MAX as u64) as u32}})
    }
}
#[cfg(test)]
mod tests{use super::*;
#[tokio::test]async fn mock_provider_is_deterministic(){let p=MockProvider;let r=p.complete(CompletionRequest{model:"m".into(),messages:vec![Message::user("hello")],tools:vec![],temperature:None,max_tokens:None}).await.unwrap();assert_eq!(r.text,"Mock provider received: hello");}
#[test]fn request_validation_rejects_empty_message_list(){let r=CompletionRequest{model:"m".into(),messages:vec![],tools:vec![],temperature:None,max_tokens:None};assert!(r.validate().is_err());}
#[test]fn assistant_tool_call_serializes_as_wire_format(){let call=ToolCall{id:"call_1".into(),name:"read_file".into(),arguments:serde_json::json!({"path":"src/main.rs"})};let msg=Message::assistant_with_tools("",&[call]);assert_eq!(msg.tool_calls[0].kind,"function");}
#[test]fn tool_result_preserves_call_id(){let msg=Message::tool("call_1","result");assert_eq!(msg.tool_call_id.as_deref(),Some("call_1"));}
#[test]fn retry_policy_is_narrow(){assert!(OpenAiCompatible::retryable(StatusCode::TOO_MANY_REQUESTS));assert!(OpenAiCompatible::retryable(StatusCode::BAD_GATEWAY));assert!(OpenAiCompatible::retryable(StatusCode::SERVICE_UNAVAILABLE));assert!(OpenAiCompatible::retryable(StatusCode::GATEWAY_TIMEOUT));assert!(!OpenAiCompatible::retryable(StatusCode::BAD_REQUEST));assert!(!OpenAiCompatible::retryable(StatusCode::INTERNAL_SERVER_ERROR));}
#[test]fn endpoint_rejects_invalid_scheme(){let provider=OpenAiCompatible::new("file:///tmp/model","k");assert!(provider.endpoint().is_err());}
}