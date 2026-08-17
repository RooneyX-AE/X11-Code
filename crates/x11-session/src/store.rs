use anyhow::Result;
use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tokio::fs;
use uuid::Uuid;
use crate::Session;

#[derive(Debug, Clone)]
pub struct SessionStore { pub root: PathBuf }

impl SessionStore {
    pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: root.into() } }
    pub fn path_for(&self, id: Uuid) -> PathBuf { self.root.join(format!("{id}.json")) }
    pub async fn save(&self, session: &Session) -> Result<PathBuf> {
        fs::create_dir_all(&self.root).await?;
        let path = self.path_for(session.id);
        session.save_to(&path).await?;
        Ok(path)
    }
    pub async fn load(&self, id: Uuid) -> Result<Session> { Ok(Session::load_from(self.path_for(id)).await?) }
    pub async fn list(&self) -> Result<Vec<(Uuid, u64, String)>> {
        let mut out = Vec::new();
        if !self.root.is_dir() { return Ok(out); }
        let mut rd = fs::read_dir(&self.root).await?;
        while let Some(entry) = rd.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") { continue; }
            if let Ok(session) = Session::load_from(&path).await { out.push((session.id, session.updated_at, session.goal)); }
        }
        out.sort_by(|a,b| b.1.cmp(&a.1));
        Ok(out)
    }
    pub async fn remove(&self, id: Uuid) -> Result<()> {
        let path = self.path_for(id);
        if fs::try_exists(&path).await? { fs::remove_file(path).await?; }
        Ok(())
    }
    pub fn timestamp() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    #[tokio::test]
    async fn store_round_trip() {
        let root = std::env::temp_dir().join(format!("x11-store-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let store = SessionStore::new(&root);
        let s = Session::new("store test");
        let id = s.id;
        store.save(&s).await.unwrap();
        let loaded = store.load(id).await.unwrap();
        assert_eq!(loaded.goal, s.goal);
        store.remove(id).await.unwrap();
        let _ = fs::remove_dir_all(root).await;
    }
}
