use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Default)]
pub struct WorkspaceLockManager {
    inner: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
}

impl WorkspaceLockManager {
    pub async fn acquire(&self, key: impl Into<String>) -> OwnedMutexGuard<()> {
        let key = key.into();
        let lock = {
            let mut map = self.inner.lock().await;
            map.entry(key).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
        };
        lock.lock_owned().await
    }

    pub async fn active_keys(&self) -> Vec<String> {
        self.inner.lock().await.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn same_key_is_serialized() {
        let locks = WorkspaceLockManager::default();
        let first = locks.acquire("src/lib.rs").await;
        let locks2 = locks.clone();
        let blocked = tokio::spawn(async move {
            let _second = locks2.acquire("src/lib.rs").await;
            true
        });
        assert!(tokio::time::timeout(Duration::from_millis(25), blocked).await.is_err());
        drop(first);
    }
}
