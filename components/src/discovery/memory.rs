use crate::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use secp256k1::PublicKey;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendStore, DiscoveryBackends,
};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
struct DiscoveryBackendTimestamped {
    created: chrono::DateTime<chrono::Utc>,
    backend: DiscoveryBackend,
}

#[derive(Clone, Debug)]
pub struct MemoryDiscoveryBackendStore {
    store: Arc<Mutex<HashMap<PublicKey, DiscoveryBackendTimestamped>>>,
    etag: Arc<AtomicU64>,
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
            etag: Arc::new(Default::default()),
        }
    }
}

#[async_trait]
impl DiscoveryBackendStore for MemoryDiscoveryBackendStore {
    type Error = DiscoveryBackendStoreError;

    async fn get(&self, public_key: &PublicKey) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let store = self.store.lock().await;
        Ok(store.get(public_key).map(|b| b.backend.clone()))
    }

    async fn get_all(&self, request_etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        let store = self.store.lock().await;
        let mut backends: Vec<DiscoveryBackendTimestamped> = store.values().cloned().collect();

        backends.sort_by(|a, b| {
            a.created
                .cmp(&b.created)
                .then_with(|| a.backend.public_key.cmp(&b.backend.public_key))
        });

        let response_etag = self.etag.load(Ordering::Relaxed);

        if request_etag == Some(response_etag) {
            Ok(DiscoveryBackends {
                etag: response_etag,
                backends: None,
            })
        } else {
            let backends = backends.into_iter().map(|b| b.backend).collect();
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
        store.insert(
            backend.public_key,
            DiscoveryBackendTimestamped {
                created: chrono::Utc::now(),
                backend,
            },
        );
        self.etag.fetch_add(1, Ordering::Relaxed);
        Ok(Some(key))
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let key = backend.public_key;
        let (created, was_new) = match store.get(&key) {
            Some(existing) => (existing.created, false),
            None => (chrono::Utc::now(), true),
        };
        store.insert(key, DiscoveryBackendTimestamped { created, backend });
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
            entry.backend.backend.weight = weight;
        }
        if let Some(enabled) = backend.backend.enabled {
            entry.backend.backend.enabled = enabled;
        }
        if let Some(partitions) = backend.backend.partitions {
            entry.backend.backend.partitions = partitions;
        }
        if let Some(name) = backend.backend.name {
            entry.backend.backend.name = name;
        }
        self.etag.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }

    async fn delete(&self, public_key: &PublicKey) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let was_found = store.remove(public_key).is_some();
        if was_found {
            self.etag.fetch_add(1, Ordering::Relaxed);
        }
        Ok(was_found)
    }
}
