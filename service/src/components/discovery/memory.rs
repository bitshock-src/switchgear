use crate::api::discovery::{DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendStore};
use crate::components::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct MemoryDiscoveryBackendStore {
    store: Arc<Mutex<HashMap<(String, DiscoveryBackendAddress), DiscoveryBackend>>>,
}

impl Default for MemoryDiscoveryBackendStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryDiscoveryBackendStore {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl DiscoveryBackendStore for MemoryDiscoveryBackendStore {
    type Error = DiscoveryBackendStoreError;

    async fn get(
        &self,
        partition: &str,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let store = self.store.lock().await;
        Ok(store.get(&(partition.to_string(), addr.clone())).cloned())
    }

    async fn get_all(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let store = self.store.lock().await;
        let backends: Vec<DiscoveryBackend> = store
            .iter()
            .filter(|((p, _), _)| p == partition)
            .map(|(_, backend)| backend.clone())
            .collect();
        Ok(backends)
    }

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error> {
        let mut store = self.store.lock().await;
        let key = (backend.partition.to_string(), backend.address.clone());
        if store.contains_key(&key) {
            return Ok(None);
        }
        let addr = backend.address.clone();
        store.insert(key, backend);
        Ok(Some(addr))
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let key = (backend.partition.to_string(), backend.address.clone());
        let was_new = store.insert(key, backend).is_none();
        Ok(was_new)
    }

    async fn delete(
        &self,
        partition: &str,
        addr: &DiscoveryBackendAddress,
    ) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let key = (partition.to_string(), addr.clone());
        let was_found = store.remove(&key).is_some();
        Ok(was_found)
    }
}
