pub mod rollback;
pub mod store;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tokio::fs;
use uuid::Uuid;
use x11_protocol::AgentEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint { pub id: Uuid, pub created_at: u64, pub event_count: usize, pub note: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id:Uuid, pub goal:String, pub created_at:u64, pub updated_at:u64, pub events:Vec<AgentEvent>,
    #[serde(default)] pub checkpoints:Vec<Checkpoint>,
}
impl Session {
 pub fn new(goal:impl Into<String>)->Self{let now=now();Self{id:Uuid::new_v4(),goal:goal.into(),created_at:now,updated_at:now,events:Vec::new(),checkpoints:Vec::new()}}
 pub fn append(&mut self,event:AgentEvent){self.events.push(event);self.updated_at=now();}
 pub fn checkpoint(&mut self,note:impl Into<String>)->Uuid{let id=Uuid::new_v4();self.checkpoints.push(Checkpoint{id,created_at:now(),event_count:self.events.len(),note:note.into()});self.updated_at=now();id}
 pub fn latest_checkpoint(&self)->Option<&Checkpoint>{self.checkpoints.last()}
 pub fn fork(&self, goal: impl Into<String>) -> Self { let mut fork=self.clone();fork.id=Uuid::new_v4();fork.goal=goal.into();fork.created_at=now();fork.updated_at=fork.created_at;fork.checkpoints.clear();fork.checkpoint("forked session");fork }
 pub fn save_json(&self)->Result<String>{Ok(serde_json::to_string_pretty(self)?)}
 pub fn load_json(input:&str)->Result<Self>{Ok(serde_json::from_str(input)?)}
 pub async fn save_to(&self,path:impl AsRef<Path>)->Result<()> {let path=path.as_ref();if let Some(p)=path.parent(){fs::create_dir_all(p).await?;}let tmp=temporary_path(path);fs::write(&tmp,self.save_json()?).await?;fs::rename(tmp,path).await?;Ok(())}
 pub async fn load_from(path:impl AsRef<Path>)->Result<Self>{Ok(Self::load_json(&fs::read_to_string(path).await?)?)}
 pub fn default_path(workspace: impl AsRef<Path>) -> PathBuf { workspace.as_ref().join(".x11/session.json") }
}
fn temporary_path(path:&Path)->PathBuf{let mut p=path.to_path_buf();p.set_extension(format!("{}.tmp",Uuid::new_v4()));p}
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}

#[cfg(test)]
mod tests { use super::*; #[test] fn fork_gets_new_identity(){let mut s=Session::new("one");s.append(AgentEvent::Error{message:"x".into()});let f=s.fork("two");assert_ne!(s.id,f.id);assert_eq!(f.goal,"two");assert!(!f.checkpoints.is_empty());assert!(s.latest_checkpoint().is_some());} }
