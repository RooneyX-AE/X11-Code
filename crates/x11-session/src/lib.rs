use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{path::Path, time::{SystemTime, UNIX_EPOCH}};
use tokio::fs;
use uuid::Uuid;
use x11_protocol::AgentEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session { pub id:Uuid, pub goal:String, pub created_at:u64, pub updated_at:u64, pub events:Vec<AgentEvent> }
impl Session {
 pub fn new(goal:impl Into<String>)->Self{let now=now();Self{id:Uuid::new_v4(),goal:goal.into(),created_at:now,updated_at:now,events:Vec::new()}}
 pub fn append(&mut self,event:AgentEvent){self.events.push(event);self.updated_at=now();}
 pub fn save_json(&self)->Result<String>{Ok(serde_json::to_string_pretty(self)?)}
 pub fn load_json(input:&str)->Result<Self>{Ok(serde_json::from_str(input)?)}
 pub async fn save_to(&self,path:impl AsRef<Path>)->Result<()> {if let Some(p)=path.as_ref().parent(){fs::create_dir_all(p).await?;}fs::write(path,self.save_json()?).await?;Ok(())}
 pub async fn load_from(path:impl AsRef<Path>)->Result<Self>{Ok(Self::load_json(&fs::read_to_string(path).await?)?)}
}
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}
