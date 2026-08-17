use anyhow::{Context,Result};
use serde::{Deserialize,Serialize};
use std::process::Stdio;
use tokio::{io::{AsyncBufReadExt,AsyncWriteExt,BufReader},process::{Child,ChildStdin,ChildStdout,Command}};

#[derive(Debug,Clone,Serialize,Deserialize)]pub struct McpServerConfig{pub name:String,pub command:String,#[serde(default)]pub args:Vec<String>}
#[derive(Debug,Clone,Serialize,Deserialize)]pub struct McpTool{pub name:String,pub description:Option<String>,pub input_schema:serde_json::Value}
#[derive(Debug,Serialize)]struct RpcReq<'a>{jsonrpc:&'static str,id:u64,method:&'a str,params:serde_json::Value}
#[derive(Debug,Deserialize)]struct RpcResp{result:Option<serde_json::Value>,error:Option<serde_json::Value>}
pub struct McpClient{child:Child,stdin:ChildStdin,reader:BufReader<ChildStdout>,next_id:u64}
impl McpClient{pub async fn spawn(cfg:&McpServerConfig)->Result<Self>{let mut c=Command::new(&cfg.command);c.args(&cfg.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());let mut child=c.spawn().context("failed to start MCP server")?;let stdin=child.stdin.take().context("MCP stdin missing")?;let stdout=child.stdout.take().context("MCP stdout missing")?;Ok(Self{child,stdin,reader:BufReader::new(stdout),next_id:1})}
 async fn request(&mut self,method:&str,params:serde_json::Value)->Result<serde_json::Value>{let id=self.next_id;self.next_id+=1;let body=serde_json::to_string(&RpcReq{jsonrpc:"2.0",id,method,params})?;self.stdin.write_all(body.as_bytes()).await?;self.stdin.write_all(b"\n").await?;self.stdin.flush().await?;let mut line=String::new();loop{line.clear();let n=self.reader.read_line(&mut line).await?;if n==0{anyhow::bail!("MCP server exited")};let resp:RpcResp=serde_json::from_str(line.trim()).context("invalid MCP JSON-RPC response")?;if let Some(e)=resp.error{anyhow::bail!("MCP error: {e}")}if let Some(r)=resp.result{return Ok(r)}}}
 pub async fn initialize(&mut self)->Result<serde_json::Value>{self.request("initialize",serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"x11-code","version":"0.1.0"}})).await}
 pub async fn list_tools(&mut self)->Result<Vec<McpTool>>{let r=self.request("tools/list",serde_json::json!({})).await?;Ok(serde_json::from_value(r["tools"].clone())?)}
 pub async fn call_tool(&mut self,name:&str,arguments:serde_json::Value)->Result<serde_json::Value>{self.request("tools/call",serde_json::json!({"name":name,"arguments":arguments})).await}
 pub async fn shutdown(mut self)->Result<()>{self.stdin.shutdown().await?;let _=self.child.kill().await;Ok(())}
}
