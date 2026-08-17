pub mod rollback;
pub mod store;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tokio::fs;
use uuid::Uuid;
use x11_protocol::AgentEvent;

const SESSION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint { pub id: Uuid, pub created_at: u64, pub event_count: usize, pub note: String, #[serde(default)] pub git_head: Option<String>, #[serde(default)] pub git_diff_hash: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session { #[serde(default = "default_schema_version")] pub schema_version:u32, pub id:Uuid, pub goal:String, pub created_at:u64, pub updated_at:u64, pub events:Vec<AgentEvent>, #[serde(default)] pub checkpoints:Vec<Checkpoint>, #[serde(default)] pub integrity:Option<String> }

impl Session {
    pub fn new(goal: impl Into<String>) -> Self { let now=now(); let mut session=Self{schema_version:SESSION_SCHEMA_VERSION,id:Uuid::new_v4(),goal:goal.into(),created_at:now,updated_at:now,events:Vec::new(),checkpoints:Vec::new(),integrity:None};session.refresh_integrity();session }
    pub fn append(&mut self,event:AgentEvent){self.events.push(event);self.updated_at=now();self.refresh_integrity();}
    pub fn checkpoint(&mut self,note:impl Into<String>)->Uuid{self.checkpoint_with_git(note,None,None)}
    pub fn checkpoint_with_git(&mut self,note:impl Into<String>,git_head:Option<String>,git_diff_hash:Option<String>)->Uuid{let id=Uuid::new_v4();self.checkpoints.push(Checkpoint{id,created_at:now(),event_count:self.events.len(),note:note.into(),git_head,git_diff_hash});self.updated_at=now();self.refresh_integrity();id}
    pub fn latest_checkpoint(&self)->Option<&Checkpoint>{self.checkpoints.last()}
    pub fn checkpoint_by_id(&self,id:Uuid)->Option<&Checkpoint>{self.checkpoints.iter().find(|c|c.id==id)}
    pub fn fork(&self,goal:impl Into<String>)->Self{let mut fork=Self::new(goal);fork.events=self.events.clone();fork.checkpoints=self.checkpoints.clone();fork.refresh_integrity();fork}
    pub fn save_json(&self)->Result<String>{let mut normalized=self.clone();normalized.integrity=None;let payload=serde_json::to_vec(&normalized)?;normalized.integrity=Some(checksum(&payload));Ok(serde_json::to_string_pretty(&normalized)?)}
    pub fn load_json(input:&str)->Result<Self>{let session:Self=serde_json::from_str(input).context("invalid X11 session JSON")?;session.validate()?;Ok(session)}
    pub fn validate(&self)->Result<()> {if self.schema_version!=SESSION_SCHEMA_VERSION{anyhow::bail!("unsupported session schema version {}",self.schema_version)}if self.goal.trim().is_empty(){anyhow::bail!("session goal cannot be empty")}if let Some(integrity)=&self.integrity{let mut normalized=self.clone();normalized.integrity=None;let payload=serde_json::to_vec(&normalized)?;let expected=checksum(&payload);if integrity!=&expected{anyhow::bail!("session integrity check failed")}}for checkpoint in &self.checkpoints{if checkpoint.event_count>self.events.len(){anyhow::bail!("checkpoint {} references events beyond session history",checkpoint.id)}}Ok(())}
    pub fn refresh_integrity(&mut self){let mut normalized=self.clone();normalized.integrity=None;if let Ok(payload)=serde_json::to_vec(&normalized){self.integrity=Some(checksum(&payload));}}
    pub async fn save_to(&self,path:impl AsRef<Path>)->Result<()> {let path=path.as_ref();if let Some(p)=path.parent(){fs::create_dir_all(p).await?;}let tmp=temporary_path(path);fs::write(&tmp,self.save_json()?).await?;if let Err(err)=fs::rename(&tmp,path).await{#[cfg(windows)]{if fs::try_exists(path).await.unwrap_or(false){fs::remove_file(path).await?;fs::rename(&tmp,path).await?;return Ok(());}}let _=fs::remove_file(&tmp).await;return Err(err.into())}Ok(())}
    pub async fn load_from(path:impl AsRef<Path>)->Result<Self>{Self::load_json(&fs::read_to_string(path).await?)}
    pub fn default_path(workspace:impl AsRef<Path>)->PathBuf{workspace.as_ref().join(".x11/session.json")}
}

fn default_schema_version()->u32{SESSION_SCHEMA_VERSION}
fn temporary_path(path:&Path)->PathBuf{let mut p=path.to_path_buf();p.set_extension(format!("{}.tmp",Uuid::new_v4()));p}
fn checksum(bytes:&[u8])->String{let mut hasher=Sha256::new();hasher.update(bytes);format!("sha256:{:x}",hasher.finalize())}
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn fork_gets_new_identity_preserves_history_and_valid_integrity(){let mut s=Session::new("one");s.append(AgentEvent::Error{message:"x".into()});let checkpoint=s.checkpoint("before fork");let f=s.fork("two");assert_ne!(s.id,f.id);assert_eq!(f.goal,"two");assert_eq!(f.events.len(),s.events.len());assert_eq!(f.checkpoints.len(),1);assert_eq!(f.checkpoints[0].id,checkpoint);assert!(f.validate().is_ok());}
 #[test]fn tampering_is_detected(){let s=Session::new("goal");let mut json=s.save_json().unwrap();json=json.replacen("goal","evil",1);assert!(Session::load_json(&json).is_err());}
 #[tokio::test]async fn save_load_round_trip(){let dir=std::env::temp_dir().join(format!("x11-session-{}",Uuid::new_v4()));let path=dir.join("session.json");let s=Session::new("round trip");s.save_to(&path).await.unwrap();let loaded=Session::load_from(&path).await.unwrap();assert_eq!(loaded.id,s.id);assert!(loaded.validate().is_ok());let _=fs::remove_dir_all(dir).await;}
}