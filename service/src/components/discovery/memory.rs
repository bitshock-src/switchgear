use crate::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendPatch, DiscoveryBackendStore,
    DiscoveryBackends,
};
use crate::components::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
struct DiscoveryBackendTimestamped {
    created: chrono::DateTime<chrono::Utc>,
    backend: DiscoveryBackend,
}

#[derive(Clone, Debug)]
pub struct MemoryDiscoveryBackendStore {
    store: Arc<Mutex<HashMap<DiscoveryBackendAddress, DiscoveryBackendTimestamped>>>,
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

    async fn get(
        &self,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let store = self.store.lock().await;
        Ok(store.get(addr).map(|b| b.backend.clone()))
    }

    async fn get_all(&self, request_etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        let store = self.store.lock().await;
        let mut backends: Vec<DiscoveryBackendTimestamped> = store.values().cloned().collect();

        backends.sort_by(|a, b| {
            a.created
                .cmp(&b.created)
                .then_with(|| a.backend.address.cmp(&b.backend.address))
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

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error> {
        let mut store = self.store.lock().await;
        let key = backend.address.clone();
        if store.contains_key(&key) {
            return Ok(None);
        }
        let addr = backend.address.clone();
        store.insert(
            key,
            DiscoveryBackendTimestamped {
                created: chrono::Utc::now(),
                backend,
            },
        );
        self.etag.fetch_add(1, Ordering::Relaxed);
        Ok(Some(addr))
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let key = backend.address.clone();
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
        let entry = match store.get_mut(&backend.address) {
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

    async fn delete(&self, addr: &DiscoveryBackendAddress) -> Result<bool, Self::Error> {
        let mut store = self.store.lock().await;
        let key = addr.clone();
        let was_found = store.remove(&key).is_some();
        if was_found {
            self.etag.fetch_add(1, Ordering::Relaxed);
        }
        Ok(was_found)
    }
}
