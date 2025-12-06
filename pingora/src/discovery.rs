use crate::{PingoraBackendProvider, PingoraLnBackendExtension, PingoraLnClientPool};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use axum::http::Extensions;
use log::error;
use pingora_core::protocols::l4::socket::SocketAddr;
use pingora_load_balancing::discovery::ServiceDiscovery;
use pingora_load_balancing::Backend;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::{Ipv6Addr, SocketAddrV6};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct LnServiceDiscovery<B, P> {
    backend_provider: B,
    pool: P,
    partitions: BTreeSet<String>,
    pingora_backend_cache: ArcSwap<BTreeSet<Backend>>,
    last_etag: AtomicU64,
}

impl<B, P> LnServiceDiscovery<B, P> {
    pub fn new(backend_provider: B, pool: P, partitions: HashSet<String>) -> Self {
        LnServiceDiscovery {
            backend_provider,
            pool,
            partitions: partitions.into_iter().collect(),
            pingora_backend_cache: ArcSwap::new(Arc::new(BTreeSet::new())),
            last_etag: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl<B, P> ServiceDiscovery for LnServiceDiscovery<B, P>
where
    B: PingoraBackendProvider + Send + Sync,
    P: PingoraLnClientPool<Key = Backend> + Send + Sync,
{
    async fn discover(&self) -> pingora_error::Result<(BTreeSet<Backend>, HashMap<u64, bool>)> {
        let etag = self.last_etag.load(Ordering::Relaxed);
        let backends = self
            .backend_provider
            .backends(Some(etag))
            .await
            .map_err(|e| {
                pingora_error::Error::because(
                    pingora_error::ErrorType::InternalError,
                    "getting all discovery backends",
                    e,
                )
            })?;

        let discovery_backends = match backends.backends {
            None => {
                return Ok((
                    (**self.pingora_backend_cache.load()).clone(),
                    HashMap::new(),
                ))
            }
            Some(backends) => BTreeSet::from_iter(backends),
        };

        self.last_etag.store(backends.etag, Ordering::Relaxed);

        let mut enablement = HashMap::new();
        let mut pingora_backends = BTreeSet::new();
        for discovery_backend in discovery_backends {
            if discovery_backend
                .backend
                .partitions
                .is_disjoint(&self.partitions)
            {
                continue;
            }
            let mut ext = Extensions::new();
            ext.insert(PingoraLnBackendExtension {
                partitions: discovery_backend.backend.partitions.clone(),
            });

            let addr = discovery_backend.public_key.serialize();
            let mut hasher = DefaultHasher::new();
            addr.hash(&mut hasher);
            let addr = hasher.finish();
            // 👍
            let addr = Ipv6Addr::from_bits(addr as u128);
            let addr = SocketAddr::Inet(SocketAddrV6::new(addr, 1, 0, 0).into());

            let pingora_backend = Backend {
                addr,
                weight: discovery_backend.backend.weight,
                ext,
            };
            if let Err(e) = self
                .pool
                .connect(pingora_backend.clone(), &discovery_backend)
            {
                error!("Failed to connect to backend {discovery_backend:?}: {e}");
                continue;
            }
            let mut hasher = DefaultHasher::new();
            pingora_backend.hash(&mut hasher);
            let hash = hasher.finish();
            enablement.insert(hash, discovery_backend.backend.enabled);
            pingora_backends.insert(pingora_backend);
        }

        self.pingora_backend_cache
            .store(Arc::new(pingora_backends.clone()));
        Ok((pingora_backends, enablement))
    }
}

#[cfg(test)]
mod tests {
    use crate::discovery::LnServiceDiscovery;
    use crate::error::PingoraLnError;
    use crate::{PingoraBackendProvider, PingoraLnClientPool, PingoraLnMetrics};
    use async_trait::async_trait;
    use pingora_load_balancing::discovery::ServiceDiscovery;
    use pingora_load_balancing::Backend;
    use rand::Rng;
    use secp256k1::{PublicKey, Secp256k1, SecretKey};
    use std::collections::{BTreeSet, HashSet};
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::io;
    use std::sync::Arc;
    use switchgear_service_api::discovery::{
        DiscoveryBackend, DiscoveryBackendSparse, DiscoveryBackends,
    };
    use switchgear_service_api::offer::Offer;
    use switchgear_service_api::service::ServiceErrorSource;
    use tokio::sync::Mutex;

    struct MockBackendProvider {
        backends_to_return: Arc<Mutex<Option<BTreeSet<DiscoveryBackend>>>>,
    }

    #[async_trait]
    impl PingoraBackendProvider for MockBackendProvider {
        type Error = PingoraLnError;

        async fn backends(&self, _etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
            let backends = self
                .backends_to_return
                .lock()
                .await
                .as_ref()
                .cloned()
                .map(|s| s.into_iter().collect::<Vec<_>>())
                .ok_or_else(|| {
                    PingoraLnError::from_io_err(
                        ServiceErrorSource::Internal,
                        "Mock BackendProvider forced error",
                        io::Error::from(io::ErrorKind::Other),
                    )
                })?;
            Ok(DiscoveryBackends {
                etag: 0,
                backends: Some(backends),
            })
        }
    }

    struct MockLnClientPool {
        should_fail_connect: bool,
    }

    #[async_trait]
    impl PingoraLnClientPool for MockLnClientPool {
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

        async fn get_metrics(&self, _key: &Self::Key) -> Result<PingoraLnMetrics, Self::Error> {
            unimplemented!("get_metrics not implemented for MockLnClientPool")
        }

        fn connect(&self, _key: Self::Key, _backend: &DiscoveryBackend) -> Result<(), Self::Error> {
            if self.should_fail_connect {
                Err(PingoraLnError::from_io_err(
                    ServiceErrorSource::Upstream,
                    "Mock LnClientPool forced connect error",
                    io::Error::from(io::ErrorKind::Other),
                ))
            } else {
                Ok(())
            }
        }
    }

    fn create_discovery_backend(partition: &str, weight: usize, enabled: bool) -> DiscoveryBackend {
        let secp = Secp256k1::new();
        let mut rng = rand::thread_rng();

        let backend_secret_key = SecretKey::from_byte_array(rng.gen::<[u8; 32]>()).unwrap();
        let backend_public_key = PublicKey::from_secret_key(&secp, &backend_secret_key);

        DiscoveryBackend {
            public_key: backend_public_key,
            backend: DiscoveryBackendSparse {
                name: None,
                partitions: [partition.to_string()].into(),
                weight,
                enabled,
                implementation: "{}".as_bytes().to_vec(),
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
        let discovery = LnServiceDiscovery::new(
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
        let backend1 = create_discovery_backend("default", 100, true);
        let backend2 = create_discovery_backend("default", 200, false);

        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(Some(BTreeSet::from([
                backend1.clone(),
                backend2.clone(),
            ])))),
        };

        let mock_ln_client_pool = MockLnClientPool {
            should_fail_connect: false,
        };
        let discovery = LnServiceDiscovery::new(
            mock_backend_provider,
            mock_ln_client_pool,
            HashSet::from(["default".to_string()]),
        );

        let (pingora_backends, enablement) = discovery.discover().await.unwrap();

        assert_eq!(pingora_backends.len(), 2);
        assert_eq!(enablement.len(), 2);

        let pingora_backend1 = pingora_backends.iter().find(|b| b.weight == 100).unwrap();
        let pingora_backend2 = pingora_backends.iter().find(|b| b.weight == 200).unwrap();

        let mut hasher = DefaultHasher::new();
        pingora_backend1.hash(&mut hasher);
        let pingora_backend1_hash = hasher.finish();

        let mut hasher = DefaultHasher::new();
        pingora_backend2.hash(&mut hasher);
        let pingora_backend2_hash = hasher.finish();

        assert!(
            enablement.get(&pingora_backend1_hash).unwrap(),
            "backend 1 enabled"
        );
        assert!(
            !enablement.get(&pingora_backend2_hash).unwrap(),
            "backend 2 disabled"
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
        let discovery = LnServiceDiscovery::new(
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
        let backend1 = create_discovery_backend("default", 100, true);
        let mock_backend_provider = MockBackendProvider {
            backends_to_return: Arc::new(Mutex::new(Some(BTreeSet::from([backend1.clone()])))),
        };

        let mock_ln_client_pool = MockLnClientPool {
            should_fail_connect: true,
        };
        let discovery = LnServiceDiscovery::new(
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
        let backend1 = create_discovery_backend("default", 100, true);
        let backend2 = create_discovery_backend("default", 200, true);
        let backend3 = create_discovery_backend("default", 300, true);

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
        impl PingoraLnClientPool for SelectiveMockLnClientPool {
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

            async fn get_metrics(&self, _key: &Self::Key) -> Result<PingoraLnMetrics, Self::Error> {
                unimplemented!("get_metrics not implemented for SelectiveMockLnClientPool")
            }

            fn connect(
                &self,
                _key: Self::Key,
                backend: &DiscoveryBackend,
            ) -> Result<(), Self::Error> {
                let addr_str = backend.public_key.to_string();
                if self
                    .fail_addresses
                    .iter()
                    .any(|fail_addr| addr_str.as_str() == fail_addr.as_str())
                {
                    Err(PingoraLnError::from_io_err(
                        ServiceErrorSource::Upstream,
                        "Selective mock pool forced connect error",
                        io::Error::from(io::ErrorKind::Other),
                    ))
                } else {
                    Ok(())
                }
            }
        }

        let mock_ln_client_pool = SelectiveMockLnClientPool {
            fail_addresses: vec![backend2.public_key.to_string()],
        };

        let discovery = LnServiceDiscovery::new(
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
}
