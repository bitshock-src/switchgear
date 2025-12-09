use crate::testing::error::TestError;
use async_trait::async_trait;
use indexmap::IndexMap;
use secp256k1::PublicKey;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendStore, DiscoveryBackends,
};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct TestDiscoveryBackendStore {
    store: Arc<Mutex<IndexMap<PublicKey, DiscoveryBackend>>>,
    etag: Arc<AtomicU64>,
}

impl Default for TestDiscoveryBackendStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TestDiscoveryBackendStore {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(IndexMap::new())),
            etag: Arc::new(Default::default()),
        }
    }
}

#[async_trait]
impl DiscoveryBackendStore for TestDiscoveryBackendStore {
    type Error = TestError;

    async fn get(&self, public_key: &PublicKey) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let store = self.store.lock().await;
        Ok(store.get(public_key).cloned())
    }

    async fn get_all(&self, request_etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        let store = self.store.lock().await;
        let response_etag = self.etag.load(Ordering::Relaxed);

        if request_etag == Some(response_etag) {
            Ok(DiscoveryBackends {
                etag: response_etag,
                backends: None,
            })
        } else {
            let backends: Vec<DiscoveryBackend> = store.values().cloned().collect();

            Ok(DiscoveryBackends {
                etag: response_etag,
                backends: Some(backends),
            })
        }
    }

    async fn post(&self, backend: DiscoveryBackend) -> Result<Option<PublicKey>, Self::Error> {
        let mut store = self.store.lock().await;
        if store.contains_key(&backend.public_key) {
            return Ok(None);
        }
        let key = backend.public_key;
        store.insert(backend.public_key, backend);
        self.etag.fetch_add(1, Ordering::Relaxed);
        Ok(Some(key))
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let key = backend.public_key;
        let was_new = !store.contains_key(&key);
        store.insert(key, backend);
        self.etag.fetch_add(1, Ordering::Relaxed);
        Ok(was_new)
    }

    async fn patch(&self, backend: DiscoveryBackendPatch) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let entry = match store.get_mut(&backend.public_key) {
            None => return Ok(false),
            Some(entry) => entry,
        };
        if let Some(weight) = backend.backend.weight {
            entry.backend.weight = weight;
        }
        if let Some(enabled) = backend.backend.enabled {
            entry.backend.enabled = enabled;
        }
        if let Some(partitions) = backend.backend.partitions {
            entry.backend.partitions = partitions;
        }
        if let Some(name) = backend.backend.name {
            entry.backend.name = name;
        }
        self.etag.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }

    async fn delete(&self, public_key: &PublicKey) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let was_found = store.swap_remove(public_key).is_some();
        if was_found {
            self.etag.fetch_add(1, Ordering::Relaxed);
        }
        Ok(was_found)
    }
}
