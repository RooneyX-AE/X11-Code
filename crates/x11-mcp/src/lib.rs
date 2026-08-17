use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, process::Stdio, time::Duration};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader}, process::{Child, ChildStdin, ChildStdout, Command}, time::timeout};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig { pub name:String,pub command:String,#[serde(default)]pub args:Vec<String>,#[serde(default)]pub env:BTreeMap<String,String>,#[serde(default)]pub cwd:Option<String>,#[serde(default)]pub enabled:bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool { pub name:String,pub description:Option<String>,pub input_schema:serde_json::Value }
impl McpTool{pub fn qualified_name(&self,server:&str)->String{format!("mcp__{server}__{}",self.name)}}

#[derive(Debug, Serialize)]struct RpcReq<'a>{jsonrpc:&'static str,id:u64,method:&'a str,params:serde_json::Value}
#[derive(Debug, Deserialize)]struct RpcResp{id:Option<u64>,result:Option<serde_json::Value>,error:Option<serde_json::Value>}

pub struct McpClient{child:Child,stdin:ChildStdin,reader:BufReader<ChildStdout>,next_id:u64,request_timeout:Duration}
impl McpClient{
 pub async fn spawn(cfg:&McpServerConfig)->Result<Self>{let mut c=Command::new(&cfg.command);c.args(&cfg.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());for(k,v)in&cfg.env{c.env(k,v);}if let Some(cwd)=&cfg.cwd{c.current_dir(cwd);}let mut child=c.spawn().context("failed to start MCP server")?;let stdin=child.stdin.take().context("MCP stdin missing")?;let stdout=child.stdout.take().context("MCP stdout missing")?;Ok(Self{child,stdin,reader:BufReader::new(stdout),next_id:1,request_timeout:DEFAULT_REQUEST_TIMEOUT})}
 pub fn with_request_timeout(mut self,timeout_duration:Duration)->Self{self.request_timeout=timeout_duration;self}
 async fn request(&mut self,method:&str,params:serde_json::Value)->Result<serde_json::Value>{let id=self.next_id;self.next_id+=1;let body=serde_json::to_string(&RpcReq{jsonrpc:"2.0",id,method,params})?;self.stdin.write_all(body.as_bytes()).await?;self.stdin.write_all(b"\n").await?;self.stdin.flush().await?;let read=async{loop{let mut line=String::new();let n=self.reader.read_line(&mut line).await?;if n==0{anyhow::bail!("MCP server exited")}if line.trim().is_empty(){continue}let resp:RpcResp=serde_json::from_str(line.trim()).context("invalid MCP JSON-RPC response")?;match resp.id{Some(response_id) if response_id==id=>{if let Some(e)=resp.error{anyhow::bail!("MCP error: {e}")}return Ok(resp.result.unwrap_or(serde_json::json!({})));},Some(_)=>continue,None=>continue}}};timeout(self.request_timeout,read).await.context("MCP request timed out")??}
 pub async fn initialize(&mut self)->Result<serde_json::Value>{self.request("initialize",serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"x11-code","version":"0.1.0"}})).await}
 pub async fn list_tools(&mut self)->Result<Vec<McpTool>>{let r=self.request("tools/list",serde_json::json!({})).await?;Ok(serde_json::from_value(r["tools"].clone())?)}
 pub async fn call_tool(&mut self,name:&str,arguments:serde_json::Value)->Result<serde_json::Value>{self.request("tools/call",serde_json::json!({"name":name,"arguments":arguments})).await}
 pub async fn shutdown(mut self)->Result<()>{self.stdin.shutdown().await?;let _=self.child.kill().await;Ok(())}
}

pub struct McpRegistry{servers:BTreeMap<String,McpServerConfig>}
impl Default for McpRegistry{fn default()->Self{Self{servers:BTreeMap::new()}}}
impl McpRegistry{pub fn register(&mut self,config:McpServerConfig){self.servers.insert(config.name.clone(),config);}pub fn get(&self,name:&str)->Option<&McpServerConfig>{self.servers.get(name)}pub fn iter(&self)->impl Iterator<Item=&McpServerConfig>{self.servers.values()}pub fn enabled(&self)->impl Iterator<Item=&McpServerConfig>{self.servers.values().filter(|s|s.enabled)}pub fn from_json(value:serde_json::Value)->Result<Self>{let map=value.get("mcpServers").cloned().unwrap_or_else(||serde_json::json!({}));let raw:BTreeMap<String,serde_json::Value>=serde_json::from_value(map)?;let mut registry=Self::default();for(name,v)in raw{let command=v.get("command").and_then(|x|x.as_str()).context("MCP server command missing")?.to_owned();registry.register(McpServerConfig{name,command,args:v.get("args").and_then(|x|serde_json::from_value(x.clone()).ok()).unwrap_or_default(),env:v.get("env").and_then(|x|serde_json::from_value(x.clone()).ok()).unwrap_or_default(),cwd:v.get("cwd").and_then(|x|x.as_str()).map(str::to_owned),enabled:v.get("enabled").and_then(|x|x.as_bool()).unwrap_or(true)});}Ok(registry)}}

#[cfg(test)]
mod tests{use super::*;#[test]fn qualifies_mcp_tool_names(){let tool=McpTool{name:"create_issue".into(),description:None,input_schema:serde_json::json!({})};assert_eq!(tool.qualified_name("github"),"mcp__github__create_issue");}#[test]fn registry_disables_servers(){let registry=McpRegistry::from_json(serde_json::json!({"mcpServers":{"demo":{"command":"demo","enabled":false}}})).unwrap();assert_eq!(registry.enabled().count(),0);}}
