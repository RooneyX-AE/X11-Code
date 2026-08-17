use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)] pub struct Message { pub role:String, pub content:String }
#[derive(Debug, Clone, Serialize, Deserialize)] pub struct ToolCall { pub id:String, pub name:String, pub arguments:serde_json::Value }
#[derive(Debug, Clone, Serialize, Deserialize)] pub struct CompletionRequest { pub model:String, pub messages:Vec<Message>, pub tools:Vec<serde_json::Value>, pub temperature:Option<f32>, pub max_tokens:Option<u32> }
#[derive(Debug, Clone, Serialize, Deserialize, Default)] pub struct Usage { pub input_tokens:u32, pub output_tokens:u32 }
#[derive(Debug, Clone, Serialize, Deserialize, Default)] pub struct CompletionResponse { pub text:String, pub tool_calls:Vec<ToolCall>, pub finish_reason:Option<String>, pub usage:Usage }
#[async_trait] pub trait ModelProvider:Send+Sync { fn name(&self)->&'static str; async fn complete(&self,request:CompletionRequest)->Result<CompletionResponse>; }

pub struct MockProvider;
#[async_trait] impl ModelProvider for MockProvider{fn name(&self)->&'static str{"mock"}async fn complete(&self,request:CompletionRequest)->Result<CompletionResponse>{let goal=request.messages.last().map(|m|m.content.clone()).unwrap_or_default();Ok(CompletionResponse{text:format!("Mock provider received: {goal}"),..Default::default()})}}

pub struct OpenAiCompatible { pub base_url:String, pub api_key:String, pub client:reqwest::Client }
impl OpenAiCompatible { pub fn new(base_url:impl Into<String>,api_key:impl Into<String>)->Self{Self{base_url:base_url.into(),api_key:api_key.into(),client:reqwest::Client::new()}} }
#[async_trait] impl ModelProvider for OpenAiCompatible{
 fn name(&self)->&'static str{"openai-compatible"}
 async fn complete(&self,r:CompletionRequest)->Result<CompletionResponse>{
  let tools=if r.tools.is_empty(){None}else{Some(r.tools.iter().map(|t|serde_json::json!({"type":"function","function":{"name":t["name"],"description":t["description"],"parameters":t["input_schema"]}})).collect::<Vec<_>>())};
  let body=serde_json::json!({"model":r.model,"messages":r.messages,"temperature":r.temperature,"max_tokens":r.max_tokens,"tools":tools});
  let resp=self.client.post(format!("{}/chat/completions",self.base_url.trim_end_matches('/')).as_str()).bearer_auth(&self.api_key).json(&body).send().await?.error_for_status()?.json::<serde_json::Value>().await?;
  let choice=&resp["choices"][0]; let msg=&choice["message"]; let text=msg["content"].as_str().unwrap_or("").to_owned(); let mut calls=Vec::new();
  if let Some(arr)=msg["tool_calls"].as_array(){for c in arr{let id=c["id"].as_str().unwrap_or_default().to_owned();let name=c["function"]["name"].as_str().unwrap_or_default().to_owned();let raw=c["function"]["arguments"].as_str().unwrap_or("{}");let arguments=serde_json::from_str(raw).unwrap_or_else(|_|serde_json::json!({"_raw":raw}));calls.push(ToolCall{id,name,arguments});}}
  Ok(CompletionResponse{text,tool_calls:calls,finish_reason:choice["finish_reason"].as_str().map(str::to_owned),usage:Usage{input_tokens:resp["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,output_tokens:resp["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32}})
 }
}
