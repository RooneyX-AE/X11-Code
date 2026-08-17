use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;
use crate::{Checkpoint, Session};

const ROLLBACK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPoint {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub checkpoint_id: Uuid,
    pub session_id: Uuid,
    pub note: String,
    pub event_count: usize,
    pub workspace: PathBuf,
    #[serde(default)]
    pub integrity: Option<String>,
}

impl RollbackPoint {
    pub fn from_session(session:&Session,checkpoint:&Checkpoint,workspace:impl Into<PathBuf>)->Self{
        let mut point=Self{schema_version:ROLLBACK_SCHEMA_VERSION,checkpoint_id:checkpoint.id,session_id:session.id,note:checkpoint.note.clone(),event_count:checkpoint.event_count,workspace:workspace.into(),integrity:None};
        point.refresh_integrity();
        point
    }
    pub fn validate(&self)->Result<()> {
        if self.schema_version!=ROLLBACK_SCHEMA_VERSION{anyhow::bail!("unsupported rollback schema version {}",self.schema_version)}
        if self.workspace.as_os_str().is_empty(){anyhow::bail!("rollback workspace cannot be empty")}
        if let Some(integrity)=&self.integrity{let mut normalized=self.clone();normalized.integrity=None;let bytes=serde_json::to_vec(&normalized)?;if integrity!=&checksum(&bytes){anyhow::bail!("rollback integrity check failed")}}
        Ok(())
    }
    pub fn refresh_integrity(&mut self){let mut normalized=self.clone();normalized.integrity=None;if let Ok(bytes)=serde_json::to_vec(&normalized){self.integrity=Some(checksum(&bytes));}}
    pub async fn save(&self,path:impl AsRef<Path>)->Result<()> {let path=path.as_ref();if let Some(parent)=path.parent(){fs::create_dir_all(parent).await?;}let tmp=path.with_extension(format!("{}.tmp",Uuid::new_v4()));fs::write(&tmp,serde_json::to_vec_pretty(self)?).await?;if let Err(err)=fs::rename(&tmp,path).await{#[cfg(windows)]{if fs::try_exists(path).await.unwrap_or(false){fs::remove_file(path).await?;fs::rename(&tmp,path).await?;return Ok(())}}let _=fs::remove_file(&tmp).await;return Err(err.into())}Ok(())}
    pub async fn load(path:impl AsRef<Path>)->Result<Self>{let bytes=fs::read(path).await.context("read rollback point")?;let point:Self=serde_json::from_slice(&bytes)?;point.validate()?;Ok(point)}
}

fn default_schema_version()->u32{ROLLBACK_SCHEMA_VERSION}
fn checksum(bytes:&[u8])->String{let mut hasher=Sha256::new();hasher.update(bytes);format!("sha256:{:x}",hasher.finalize())}

#[cfg(test)]
mod tests{
 use super::*;
 #[tokio::test]async fn rollback_round_trip_and_integrity(){let dir=std::env::temp_dir().join(format!("x11-rollback-{}",Uuid::new_v4()));let path=dir.join("point.json");let session=Session::new("goal");let mut session=session;let id=session.checkpoint("safe");let checkpoint=session.checkpoint_by_id(id).unwrap().clone();let point=RollbackPoint::from_session(&session,&checkpoint,&dir);point.save(&path).await.unwrap();let loaded=RollbackPoint::load(&path).await.unwrap();assert_eq!(loaded.checkpoint_id,id);assert_eq!(loaded.session_id,session.id);let _=fs::remove_dir_all(dir).await;}
 #[tokio::test]async fn tampered_rollback_is_rejected(){let dir=std::env::temp_dir().join(format!("x11-rollback-{}",Uuid::new_v4()));let path=dir.join("point.json");let session=Session::new("goal");let mut session=session;let id=session.checkpoint("safe");let checkpoint=session.checkpoint_by_id(id).unwrap().clone();let point=RollbackPoint::from_session(&session,&checkpoint,&dir);point.save(&path).await.unwrap();let mut text=fs::read_to_string(&path).await.unwrap();text=text.replacen("safe","evil",1);fs::write(&path,text).await.unwrap();assert!(RollbackPoint::load(&path).await.is_err());let _=fs::remove_dir_all(dir).await;}
}