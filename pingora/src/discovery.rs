use crate::error::PingoraLnError;
use crate::socket::IntoPingoraSocketAddr;
use crate::{PingoraBackendProvider, PingoraLnBackendExtension};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use axum::http::Extensions;
use log::error;
use pingora_load_balancing::discovery::ServiceDiscovery;
use pingora_load_balancing::Backend;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use switchgear_service::api::discovery::DiscoveryBackendStore;
use switchgear_service::api::discovery::{DiscoveryBackend, DiscoveryBackendAddress};
use switchgear_service::api::service::ServiceErrorSource;
use switchgear_service::components::discovery::db::DbDiscoveryBackendStore;
use switchgear_service::components::discovery::file::FileDiscoveryBackendStore;
use switchgear_service::components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_service::components::discovery::memory::MemoryDiscoveryBackendStore;
use switchgear_service::components::pool::LnClientPool;

pub struct DefaultPingoraLnDiscovery<B, P> {
    backend_provider: B,
    pool: P,
    partitions: HashSet<String>,
    pingora_backend_cache: ArcSwap<BTreeSet<Backend>>,
    last_discovery_backend_hash: AtomicU64,
}

impl<B, P> DefaultPingoraLnDiscovery<B, P> {
    pub fn new(backend_provider: B, pool: P, partitions: HashSet<String>) -> Self {
        DefaultPingoraLnDiscovery {
            backend_provider,
            pool,
            partitions,
            pingora_backend_cache: ArcSwap::new(Arc::new(BTreeSet::new())),
            last_discovery_backend_hash: Default::default(),
        }
    }
}

#[async_trait]
impl<B, P> ServiceDiscovery for DefaultPingoraLnDiscovery<B, P>
where
    B: PingoraBackendProvider + Send + Sync,
    P: LnClientPool<Key = Backend> + Send + Sync,
{
    async fn discover(&self) -> pingora_error::Result<(BTreeSet<Backend>, HashMap<u64, bool>)> {
        let mut discovery_backends = BTreeSet::<DiscoveryBackend>::new();

        for partition in &self.partitions {
            let partitioned_backends =
                self.backend_provider
                    .backends(partition)
                    .await
                    .map_err(|e| {
                        pingora_error::Error::because(
                            pingora_error::ErrorType::InternalError,
                            "discovery for backends",
                            e,
                        )
                    })?;
            discovery_backends.extend(partitioned_backends);
        }
        let mut hasher = DefaultHasher::new();
        discovery_backends.hash(&mut hasher);
        let latest_discovery_backend_hash = hasher.finish();

        if self.last_discovery_backend_hash.swap(
            latest_discovery_backend_hash,
            std::sync::atomic::Ordering::Relaxed,
        ) == latest_discovery_backend_hash
        {
            return Ok((
                (**self.pingora_backend_cache.load()).clone(),
                HashMap::new(),
            ));
        }

        let mut pingora_backends_by_address = BTreeMap::<DiscoveryBackendAddress, Backend>::new();
        let mut enablement = HashMap::new();

        for discovery_backend in discovery_backends {
            let pingora_backend =
                match pingora_backends_by_address.get_mut(&discovery_backend.address) {
                    None => {
                        let mut ext = Extensions::new();
                        ext.insert(PingoraLnBackendExtension {
                            partitions: Default::default(),
                        });
                        let pingora_backend = Backend {
                            addr: discovery_backend.address.as_pingora_socket_addr(),
                            weight: discovery_backend.backend.weight,
                            ext,
                        };
                        if let Err(e) = self
                            .pool
                            .connect(pingora_backend.clone(), &discovery_backend)
                        {
                            error!("Failed to connect to backend {discovery_backend:?}: {e}");
                            None
                        } else {
                            Some(
                                pingora_backends_by_address
                                    .entry(discovery_backend.address.clone())
                                    .or_insert(pingora_backend),
                            )
                        }
                    }
                    Some(b) => Some(b),
                };

            if let Some(pingora_backend) = pingora_backend {
                #[allow(clippy::expect_used)]
                pingora_backend
                    .ext
                    .get_mut::<PingoraLnBackendExtension>()
                    .expect("just added")
                    .partitions
                    .insert(discovery_backend.partition.clone());
                let mut hasher = DefaultHasher::new();
                pingora_backend.hash(&mut hasher);
                let hash = hasher.finish();
                enablement.insert(hash, discovery_backend.backend.enabled);
            }
        }

        let backends = BTreeSet::from_iter(pingora_backends_by_address.into_values());
        self.pingora_backend_cache.store(Arc::new(backends.clone()));

        Ok((backends, enablement))
    }
}

#[async_trait]
impl PingoraBackendProvider for MemoryDiscoveryBackendStore {
    type Error = PingoraLnError;

    async fn backends(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let backends = self.get_all(partition).await.map_err(|e| {
            PingoraLnError::from_discovery_backend_store_err(
                ServiceErrorSource::Internal,
                "getting all discovery backends",
                e,
            )
        })?;
        Ok(backends)
    }
}

#[async_trait]
impl PingoraBackendProvider for DbDiscoveryBackendStore {
    type Error = PingoraLnError;

    async fn backends(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let backends = self.get_all(partition).await.map_err(|e| {
            PingoraLnError::from_discovery_backend_store_err(
                ServiceErrorSource::Internal,
                "getting all discovery backends",
                e,
            )
        })?;
        Ok(backends)
    }
}

#[async_trait]
impl PingoraBackendProvider for HttpDiscoveryBackendStore {
    type Error = PingoraLnError;

    async fn backends(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let backends = self.get_all(partition).await.map_err(|e| {
            PingoraLnError::from_discovery_backend_store_err(
                ServiceErrorSource::Internal,
                "getting all discovery backends",
                e,
            )
        })?;
        Ok(backends)
    }
}

#[async_trait]
impl PingoraBackendProvider for FileDiscoveryBackendStore {
    type Error = PingoraLnError;

    async fn backends(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let backends = self.get_all(partition).await.map_err(|e| {
            PingoraLnError::from_discovery_backend_store_err(
                ServiceErrorSource::Internal,
                "getting all discovery backends",
                e,
            )
        })?;
        Ok(backends)
    }
}

#[cfg(test)]
mod tests {
    use crate::discovery::DefaultPingoraLnDiscovery;
    use crate::error::PingoraLnError;
    use crate::PingoraBackendProvider;
    use async_trait::async_trait;
    use pingora_load_balancing::discovery::ServiceDiscovery;
    use pingora_load_balancing::Backend;
    use std::collections::{BTreeSet, HashSet};
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::sync::Arc;
    use switchgear_service::api::discovery::{
        DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
        DiscoveryBackendSparse,
    };
    use switchgear_service::api::offer::Offer;
    use switchgear_service::api::service::ServiceErrorSource;
    use switchgear_service::components::pool::error::{LnPoolError, LnPoolErrorSourceKind};
    use switchgear_service::components::pool::LnClientPool;
    use switchgear_service::components::pool::LnMetrics;
    use tokio::sync::Mutex;
    use url::Url;

    struct MockBackendProvider {
        backends_to_return: Arc<Mutex<Option<BTreeSet<DiscoveryBackend>>>>,
    }

    #[async_trait]
    impl PingoraBackendProvider for MockBackendProvider {
        type Error = PingoraLnError;

        async fn backends(&self, _partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error> {
            self.backends_to_return
                .lock()
                .await
                .as_ref()
                .cloned()
                .map(|s| s.into_iter().collect::<Vec<_>>())
                .ok_or_else(|| {
                    PingoraLnError::new(
                        crate::error::PingoraLnErrorSourceKind::PoolError(LnPoolError::new(
                            LnPoolErrorSourceKind::Generic,
                            ServiceErrorSource::Internal,
                            "Mock BackendProvider forced error",
                        )),
                        ServiceErrorSource::Internal,
                        "Mock BackendProvider forced error",
                    )
                })
        }
    }

    struct MockLnClientPool {
        should_fail_connect: bool,
    }

    #[async_trait]
    impl LnClientPool for MockLnClientPool {
        type Error = PingoraLnError;
        type Key = Backend;

        async fn get_invoice(
            &self,
            _offer: &Offer,
            _key: &Self::Key,
            _amount_msat: Option<u64>,
            _expiry_secs: Option<u64>,
        ) -> Result<String, Self::Error> {
            unimplemented!("get_invoice not implemented for MockLnClientPool")
        }

        async fn get_metrics(&self, _key: &Self::Key) -> Result<LnMetrics, Self::Error> {
            unimplemented!("get_metrics not implemented for MockLnClientPool")
        }

        fn connect(&self, _key: Self::Key, _backend: &DiscoveryBackend) -> Result<(), Self::Error> {
            if self.should_fail_connect {
                Err(PingoraLnError::new(
                    crate::error::PingoraLnErrorSourceKind::PoolError(LnPoolError::new(
                        LnPoolErrorSourceKind::Generic,
                        ServiceErrorSource::Internal,
                        "Mock LnClientPool forced connect error",
                    )),
                    ServiceErrorSource::Upstream,
                    "Mock LnClientPool forced connect error",
                ))
            } else {
                Ok(())
            }
        }
    }

    fn create_discovery_backend(
        partition: &str,
        address: &str,
        weight: usize,
        enabled: bool,
    ) -> DiscoveryBackend {
        DiscoveryBackend {
            partition: partition.to_string(),
            address: DiscoveryBackendAddress::Url(
                Url::parse(&format!("https://{address}")).unwrap(),
            ),
            backend: DiscoveryBackendSparse {
                weight,
                enabled,
                implementation: DiscoveryBackendImplementation::RemoteHttp,
            },
        }
    }

    #[tokio::test]
    async fn discover_when_no_backends_exist_then_returns_empty_sets() {
        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(Some(BTreeSet::new()))),
        };
        let mock_ln_client_pool = MockLnClientPool {
            should_fail_connect: false,
        };
        let discovery = DefaultPingoraLnDiscovery::new(
            mock_backend_provider,
            mock_ln_client_pool,
            HashSet::from(["default".to_string()]),
        );
        let (backends, enablement) = discovery.discover().await.unwrap();

        assert!(backends.is_empty());
        assert!(enablement.is_empty());
    }

    #[tokio::test]
    async fn discover_when_new_backends_exist_then_creates_pingora_backends() {
        let backend1 = create_discovery_backend("default", "127.0.0.1:8001", 100, true);
        let backend2 = create_discovery_backend("default", "127.0.0.1:8002", 200, false); // Disabled backend

        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(Some(BTreeSet::from([
                backend1.clone(),
                backend2.clone(),
            ])))),
        };
        let mock_ln_client_pool = MockLnClientPool {
            should_fail_connect: false,
        };
        let discovery = DefaultPingoraLnDiscovery::new(
            mock_backend_provider,
            mock_ln_client_pool,
            HashSet::from(["default".to_string()]),
        );

        let (pingora_backends, enablement) = discovery.discover().await.unwrap();

        assert_eq!(pingora_backends.len(), 2);

        let pingora_backends_vec = pingora_backends.iter().collect::<Vec<_>>();

        // prove pingora_backends_vec order is what we expect
        assert_eq!(pingora_backends_vec[0].weight, 200);
        assert_eq!(pingora_backends_vec[1].weight, 100);

        let mut hasher = DefaultHasher::new();
        pingora_backends_vec[0].hash(&mut hasher);
        let pingora_backend1_hash = hasher.finish();

        let mut hasher = DefaultHasher::new();
        pingora_backends_vec[1].hash(&mut hasher);
        let pingora_backend2_hash = hasher.finish();

        assert!(
            enablement.get(&pingora_backend2_hash).unwrap(),
            "backend 2 enabled"
        );
        assert!(
            !enablement.get(&pingora_backend1_hash).unwrap(),
            "backend 1 disabled"
        );
    }

    #[tokio::test]
    async fn discover_when_backend_provider_fails_then_returns_error() {
        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(None)),
        };
        let mock_ln_client_pool = MockLnClientPool {
            should_fail_connect: false,
        };
        let discovery = DefaultPingoraLnDiscovery::new(
            mock_backend_provider,
            mock_ln_client_pool,
            HashSet::from(["default".to_string()]),
        );

        let result = discovery.discover().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.etype, pingora_error::ErrorType::InternalError);
        assert!(err
            .cause
            .unwrap()
            .to_string()
            .contains("Mock BackendProvider forced error"));
    }

    #[tokio::test]
    async fn discover_when_pool_connect_fails_then_returns_empty_backends() {
        let backend1 = create_discovery_backend("default", "127.0.0.1:8001", 100, true);
        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(Some(BTreeSet::from([backend1.clone()])))),
        };

        let mock_ln_client_pool = MockLnClientPool {
            should_fail_connect: true,
        };
        let discovery = DefaultPingoraLnDiscovery::new(
            mock_backend_provider,
            mock_ln_client_pool,
            HashSet::from(["default".to_string()]),
        );

        let (backends, enablement) = discovery.discover().await.unwrap();

        // When all backends fail to connect, we get empty results
        assert!(backends.is_empty());
        assert!(enablement.is_empty());
    }

    #[tokio::test]
    async fn discover_when_some_backends_fail_then_returns_successful_backends() {
        let backend1 = create_discovery_backend("default", "127.0.0.1:8001", 100, true);
        let backend2 = create_discovery_backend("default", "127.0.0.1:8002", 200, true);
        let backend3 = create_discovery_backend("default", "127.0.0.1:8003", 300, true);

        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(Some(BTreeSet::from([
                backend1.clone(),
                backend2.clone(),
                backend3.clone(),
            ])))),
        };

        // Create a custom pool that fails for specific addresses
        struct SelectiveMockLnClientPool {
            fail_addresses: Vec<String>,
        }

        #[async_trait]
        impl LnClientPool for SelectiveMockLnClientPool {
            type Error = PingoraLnError;
            type Key = Backend;

            async fn get_invoice(
                &self,
                _offer: &Offer,
                _key: &Self::Key,
                _amount_msat: Option<u64>,
                _expiry_secs: Option<u64>,
            ) -> Result<String, Self::Error> {
                unimplemented!("get_invoice not implemented for SelectiveMockLnClientPool")
            }

            async fn get_metrics(&self, _key: &Self::Key) -> Result<LnMetrics, Self::Error> {
                unimplemented!("get_metrics not implemented for SelectiveMockLnClientPool")
            }

            fn connect(
                &self,
                _key: Self::Key,
                backend: &DiscoveryBackend,
            ) -> Result<(), Self::Error> {
                let addr_str = format!("{:?}", backend.address);
                if self
                    .fail_addresses
                    .iter()
                    .any(|fail_addr| addr_str.contains(fail_addr))
                {
                    Err(PingoraLnError::new(
                        crate::error::PingoraLnErrorSourceKind::PoolError(LnPoolError::new(
                            LnPoolErrorSourceKind::Generic,
                            ServiceErrorSource::Upstream,
                            format!("Forced failure for address: {addr_str}"),
                        )),
                        ServiceErrorSource::Upstream,
                        "Selective mock pool forced connect error",
                    ))
                } else {
                    Ok(())
                }
            }
        }

        let mock_ln_client_pool = SelectiveMockLnClientPool {
            fail_addresses: vec!["8002".to_string()], // Fail only the second backend
        };

        let discovery = DefaultPingoraLnDiscovery::new(
            mock_backend_provider,
            mock_ln_client_pool,
            HashSet::from(["default".to_string()]),
        );

        let (backends, enablement) = discovery.discover().await.unwrap();

        // Should have 2 successful backends (1st and 3rd)
        assert_eq!(backends.len(), 2);
        assert_eq!(enablement.len(), 2);

        // Verify the weights to ensure we got the right backends
        let backend_weights: Vec<usize> = backends.iter().map(|b| b.weight).collect();
        assert!(backend_weights.contains(&100)); // First backend
        assert!(backend_weights.contains(&300)); // Third backend
        assert!(!backend_weights.contains(&200)); // Second backend should be missing
    }

    #[tokio::test]
    async fn discover_when_backend_disabled_then_adds_to_enablement_map() {
        let backend1 = create_discovery_backend("default", "127.0.0.1:8001", 100, false); // Disabled
        let backend2 = create_discovery_backend("default", "127.0.0.1:8002", 200, true); // Enabled

        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(Some(BTreeSet::from([
                backend1.clone(),
                backend2.clone(),
            ])))),
        };

        let mock_ln_client_pool = MockLnClientPool {
            should_fail_connect: false,
        };
        let discovery = DefaultPingoraLnDiscovery::new(
            mock_backend_provider,
            mock_ln_client_pool,
            HashSet::from(["default".to_string()]),
        );

        let (pingora_backends, enablement) = discovery.discover().await.unwrap();

        let pingora_backends_vec = pingora_backends.iter().collect::<Vec<_>>();

        // prove pingora_backends_vec order is what we expect
        assert_eq!(pingora_backends_vec[0].weight, 200);
        assert_eq!(pingora_backends_vec[1].weight, 100);

        let mut hasher = DefaultHasher::new();
        pingora_backends_vec[0].hash(&mut hasher);
        let pingora_backend1_hash = hasher.finish();

        let mut hasher = DefaultHasher::new();
        pingora_backends_vec[1].hash(&mut hasher);
        let pingora_backend2_hash = hasher.finish();

        assert_eq!(pingora_backends.len(), 2);

        assert!(
            !enablement.get(&pingora_backend2_hash).unwrap(),
            "backend 2 disabled"
        );
        assert!(
            enablement.get(&pingora_backend1_hash).unwrap(),
            "backend 1 enabled"
        );
    }
}
