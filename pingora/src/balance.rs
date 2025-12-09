use crate::error::PingoraLnError;
use crate::PingoraBackoffProvider;
use crate::{PingoraLnBackendExtension, PingoraLnClientPool, PingoraLnMetricsCache};
use async_trait::async_trait;
use backoff::backoff::Backoff;
use log::{error, warn};
use pingora_core::services::background::BackgroundService;
use pingora_load_balancing::selection::{BackendIter, BackendSelection};
use pingora_load_balancing::{Backend, LoadBalancer};
use std::error::Error;
use std::sync::Arc;
use switchgear_service_api::balance::{LnBalancer, LnBalancerBackgroundServices};
use switchgear_service_api::offer::Offer;
use switchgear_service_api::service::{HasServiceErrorSource, ServiceErrorSource};
use tokio::sync::watch::Receiver;
use tokio::time::sleep;

pub trait MaxIterations: Clone + Send + Sync {
    fn max_iterations(&self, backends: usize) -> usize;
}

#[derive(Clone)]
pub struct RoundRobinMaxIterations;

impl MaxIterations for RoundRobinMaxIterations {
    fn max_iterations(&self, backends: usize) -> usize {
        backends
    }
}

#[derive(Clone)]
pub struct RandomMaxIterations;

impl MaxIterations for RandomMaxIterations {
    fn max_iterations(&self, backends: usize) -> usize {
        if backends <= 1 {
            return backends;
        }
        let n = backends as f64;
        let calculation = n * n.ln() * 4.0;
        calculation.ceil() as usize
    }
}

#[derive(Clone)]
pub struct ConsistentMaxIterations {
    max_iterations: usize,
}

impl ConsistentMaxIterations {
    pub fn new(max_iterations: usize) -> Self {
        Self { max_iterations }
    }
}

impl MaxIterations for ConsistentMaxIterations {
    fn max_iterations(&self, _backends: usize) -> usize {
        self.max_iterations
    }
}

pub struct PingoraLnBalancer<S, P, M, B, X>
where
    P: Clone,
    M: Clone,
    B: PingoraBackoffProvider,
    X: MaxIterations,
{
    load_balancer: Arc<LoadBalancer<S>>,
    backoff_provider: B,
    pool: P,
    metrics: M,
    select_max_iterations: X,
    parallel_health_check: bool,
    selection_capacity_bias: Option<f64>,
}

impl<S, P, M, B, X> Clone for PingoraLnBalancer<S, P, M, B, X>
where
    P: Clone,
    M: Clone,
    B: PingoraBackoffProvider,
    X: MaxIterations,
{
    fn clone(&self) -> Self {
        Self {
            load_balancer: self.load_balancer.clone(),
            backoff_provider: self.backoff_provider.clone(),
            select_max_iterations: self.select_max_iterations.clone(),
            pool: self.pool.clone(),
            metrics: self.metrics.clone(),
            parallel_health_check: self.parallel_health_check,
            selection_capacity_bias: self.selection_capacity_bias,
        }
    }
}

impl<S, P, M, B, X> PingoraLnBalancer<S, P, M, B, X>
where
    S: BackendSelection + 'static,
    S::Iter: BackendIter,
    P: PingoraLnClientPool<Key = Backend> + Clone,
    P::Error: Error + Send + Sync + 'static + HasServiceErrorSource,
    M: PingoraLnMetricsCache<Key = Backend> + Clone,
    B: PingoraBackoffProvider,
    X: MaxIterations,
{
    pub fn new(
        load_balancer: Arc<LoadBalancer<S>>,
        pool: P,
        metrics: M,
        backoff_provider: B,
        select_max_iterations: X,
        parallel_health_check: bool,
        selection_capacity_bias: Option<f64>,
    ) -> Self {
        Self {
            load_balancer,
            pool,
            metrics,
            backoff_provider,
            select_max_iterations,
            parallel_health_check,
            selection_capacity_bias,
        }
    }

    fn select_backend(
        &self,
        offer: &Offer,
        amount_msat: u64,
        key: &[u8],
        current_selection_capacity_bias: Option<f64>,
    ) -> Option<Backend> {
        let select_max_iterations = self
            .select_max_iterations
            .max_iterations(self.load_balancer.backends().get_backend().len());
        self.load_balancer
            .select_with(key, select_max_iterations, |backend, health| {
                if !health {
                    return false;
                }
                if let Some(extension) = backend.ext.get::<PingoraLnBackendExtension>() {
                    if extension.partitions.contains(&offer.partition) {
                        if let Some(metrics) = self.metrics.get_cached_metrics(backend) {
                            if let Some(current_selection_capacity_bias) =
                                current_selection_capacity_bias
                            {
                                if amount_msat as f64
                                    <= metrics.node_effective_inbound_msat as f64
                                        * (1.0 + current_selection_capacity_bias)
                                {
                                    return true;
                                }
                            } else {
                                return true;
                            }
                        }
                    }
                }
                false
            })
    }

    async fn get_invoice_from_backend(
        &self,
        offer: &Offer,
        amount_msat: u64,
        expiry_secs: u64,
        backend: &Backend,
    ) -> Result<String, PingoraLnError> {
        let invoice = self
            .pool
            .get_invoice(offer, backend, amount_msat.into(), expiry_secs.into())
            .await
            .map_err(|e| {
                PingoraLnError::from_service_error(
                    format!("get invoice for offer {}/{}", offer.partition, offer.id),
                    e,
                )
            })?;

        Ok(invoice)
    }
}

#[async_trait]
impl<S, P, M, B, X> LnBalancer for PingoraLnBalancer<S, P, M, B, X>
where
    S: BackendSelection + Send + Sync + 'static,
    S::Iter: BackendIter,
    P: PingoraLnClientPool<Key = Backend> + Send + Sync + Clone + 'static,
    P::Error: Error + Send + Sync + 'static + HasServiceErrorSource,
    M: PingoraLnMetricsCache<Key = Backend> + Send + Sync + Clone + 'static,
    B: PingoraBackoffProvider + Send + Sync + 'static,
    X: MaxIterations,
{
    type Error = PingoraLnError;

    async fn get_invoice(
        &self,
        offer: &Offer,
        amount_msat: u64,
        expiry_secs: u64,
        key: &[u8],
    ) -> Result<String, Self::Error> {
        let mut backoff = self.backoff_provider.get_backoff();
        let mut current_selection_capacity_bias = self.selection_capacity_bias;

        loop {
            let invoice =
                self.select_backend(offer, amount_msat, key, current_selection_capacity_bias);
            if current_selection_capacity_bias.is_some() {
                current_selection_capacity_bias = None;
                if invoice.is_none() {
                    continue;
                }
            }

            let invoice = match invoice.ok_or_else(|| {
                PingoraLnError::no_available_nodes(
                    ServiceErrorSource::Upstream,
                    format!("load balancing invoice request for offer {offer:?}"),
                )
            }) {
                Ok(backend) => {
                    self.get_invoice_from_backend(offer, amount_msat, expiry_secs, &backend)
                        .await
                }
                Err(e) => Err(e),
            };

            match invoice {
                Ok(invoice) => return Ok(invoice),
                Err(e) => {
                    if ServiceErrorSource::Downstream == e.esource() {
                        return Err(e);
                    } else {
                        match backoff.next_backoff() {
                            Some(duration) => {
                                warn!(
                                    "error retrieving invoice: {e}, retrying in {}s",
                                    duration.as_secs()
                                );
                                tokio::join!(sleep(duration), async {
                                    if let Err(e) = self.load_balancer.update().await {
                                        error!("Error updating load balancer discovery: {e}");
                                    }
                                    self.load_balancer
                                        .backends()
                                        .run_health_check(self.parallel_health_check)
                                        .await
                                });
                            }
                            None => {
                                error!("Too many retries, giving up: {e}");
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn health(&self) -> Result<(), Self::Error> {
        let select_max_iterations = self
            .select_max_iterations
            .max_iterations(self.load_balancer.backends().get_backend().len());
        self.load_balancer
            .select(&[], select_max_iterations)
            .ok_or_else(|| {
                PingoraLnError::no_available_nodes(ServiceErrorSource::Upstream, "health check")
            })?;

        Ok(())
    }
}

#[async_trait]
impl<S, P, M, B, X> LnBalancerBackgroundServices for PingoraLnBalancer<S, P, M, B, X>
where
    S: BackendSelection + Send + Sync + 'static,
    S::Iter: BackendIter,
    P: PingoraLnClientPool<Key = Backend> + Send + Sync + Clone + 'static,
    P::Error: Error + Send + Sync + 'static + HasServiceErrorSource,
    M: PingoraLnMetricsCache<Key = Backend> + Send + Sync + Clone + 'static,
    B: PingoraBackoffProvider + Send + Sync + 'static,
    X: MaxIterations,
{
    async fn start(&self, shutdown_rx: Receiver<bool>) {
        self.load_balancer.start(shutdown_rx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backoff::StopBackoffProvider;
    use crate::{PingoraLnBackendExtension, PingoraLnMetrics};
    use async_trait::async_trait;
    use pingora_error::Result as PingoraResult;
    use pingora_load_balancing::discovery::ServiceDiscovery;
    use pingora_load_balancing::health_check::HealthCheck;
    use pingora_load_balancing::selection::RoundRobin;
    use pingora_load_balancing::{Backends, LoadBalancer};
    use std::collections::{BTreeSet, HashMap};
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::sync::{Arc, Mutex};
    use switchgear_service_api::balance::LnBalancer;
    use switchgear_service_api::discovery::DiscoveryBackend;
    use switchgear_service_api::service::ServiceErrorSource;
    use uuid::Uuid;

    #[derive(Clone)]
    struct MockLnClientPool {
        should_succeed: bool,
        backend_specific_response: bool,
    }

    #[async_trait]
    impl PingoraLnClientPool for MockLnClientPool {
        type Error = PingoraLnError;
        type Key = Backend;

        async fn get_invoice(
            &self,
            _offer: &Offer,
            key: &Self::Key,
            _amount_msat: Option<u64>,
            _expiry_secs: Option<u64>,
        ) -> Result<String, Self::Error> {
            if self.should_succeed {
                if self.backend_specific_response {
                    Ok(format!("invoice_from_{}", key.addr))
                } else {
                    Ok("mock_invoice".to_string())
                }
            } else {
                Err(PingoraLnError::general_error(
                    ServiceErrorSource::Upstream,
                    "mock get_invoice",
                    "forced error".to_string(),
                ))
            }
        }

        async fn get_metrics(&self, _key: &Self::Key) -> Result<PingoraLnMetrics, Self::Error> {
            unimplemented!("get_metrics not needed for these tests")
        }

        fn connect(&self, _key: Self::Key, _backend: &DiscoveryBackend) -> Result<(), Self::Error> {
            unimplemented!("connect not needed for these tests")
        }
    }

    #[derive(Clone, Default)]
    struct MockLnMetricsCache {
        metrics: Arc<Mutex<HashMap<Backend, PingoraLnMetrics>>>,
    }

    impl MockLnMetricsCache {
        fn set_metrics_for_backend(&self, backend: &Backend, metrics: PingoraLnMetrics) {
            self.metrics
                .lock()
                .unwrap()
                .insert(backend.clone(), metrics);
        }
    }

    impl PingoraLnMetricsCache for MockLnMetricsCache {
        type Key = Backend;
        fn get_cached_metrics(&self, backend: &Backend) -> Option<PingoraLnMetrics> {
            self.metrics.lock().unwrap().get(backend).cloned()
        }
    }

    struct MockServiceDiscovery {
        backends: BTreeSet<Backend>,
        enablement: HashMap<u64, bool>,
    }

    #[async_trait]
    impl ServiceDiscovery for MockServiceDiscovery {
        async fn discover(&self) -> PingoraResult<(BTreeSet<Backend>, HashMap<u64, bool>)> {
            Ok((self.backends.clone(), self.enablement.clone()))
        }
    }

    struct NoOpHealthCheck;

    #[async_trait]
    impl HealthCheck for NoOpHealthCheck {
        async fn check(&self, _backend: &Backend) -> PingoraResult<()> {
            Ok(())
        }

        fn health_threshold(&self, _: bool) -> usize {
            1
        }
    }

    fn create_mock_backend_with_partitions(addr: &str, partitions: Vec<&str>) -> Backend {
        let mut backend = Backend::new(addr).unwrap();
        backend.ext.insert(PingoraLnBackendExtension {
            partitions: partitions.into_iter().map(|s| s.to_string()).collect(),
        });
        backend
    }

    fn create_mock_backend(addr: &str, partition: &str) -> Backend {
        create_mock_backend_with_partitions(addr, vec![partition])
    }

    fn create_test_offer() -> Offer {
        Offer {
            partition: "default".to_string(),
            id: Uuid::new_v4(),
            max_sendable: 1000000,
            min_sendable: 1000,
            metadata_json_string: "{}".to_string(),
            metadata_json_hash: [0; 32],
            timestamp: chrono::Utc::now() - chrono::Duration::hours(1),
            expires: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        }
    }

    async fn setup_balancer_with_backends_and_optional_bias(
        should_succeed: bool,
        backend_configs: Vec<(Backend, bool)>, // (backend, enabled)
        selection_capacity_bias: Option<f64>,
        backend_specific_response: bool,
    ) -> PingoraLnBalancer<
        RoundRobin,
        MockLnClientPool,
        MockLnMetricsCache,
        StopBackoffProvider,
        RoundRobinMaxIterations,
    > {
        let pool = MockLnClientPool {
            should_succeed,
            backend_specific_response,
        };
        let metrics_cache = MockLnMetricsCache::default();

        let mut backends = BTreeSet::new();
        let mut enablement = HashMap::new();

        for (backend, enabled) in backend_configs {
            backends.insert(backend.clone());
            let mut hasher = DefaultHasher::new();
            backend.hash(&mut hasher);
            enablement.insert(hasher.finish(), enabled);
        }

        let discovery = Box::new(MockServiceDiscovery {
            backends,
            enablement,
        });

        let mut load_balancer = LoadBalancer::<RoundRobin>::from_backends(Backends::new(discovery));
        load_balancer.set_health_check(Box::new(NoOpHealthCheck));
        let load_balancer_arc = Arc::new(load_balancer);

        // Force discovery and health check to populate backends
        load_balancer_arc.update().await.unwrap();

        let backoff_provider = StopBackoffProvider;

        PingoraLnBalancer::new(
            load_balancer_arc,
            pool,
            metrics_cache,
            backoff_provider,
            RoundRobinMaxIterations,
            true,
            selection_capacity_bias,
        )
    }

    async fn setup_balancer_with_backends_and_bias(
        should_succeed: bool,
        backend_configs: Vec<(Backend, bool)>, // (backend, enabled)
        selection_capacity_bias: f64,
        backend_specific_response: bool,
    ) -> PingoraLnBalancer<
        RoundRobin,
        MockLnClientPool,
        MockLnMetricsCache,
        StopBackoffProvider,
        RoundRobinMaxIterations,
    > {
        setup_balancer_with_backends_and_optional_bias(
            should_succeed,
            backend_configs,
            Some(selection_capacity_bias),
            backend_specific_response,
        )
        .await
    }

    async fn setup_balancer_with_backends(
        should_succeed: bool,
        backend_configs: Vec<(Backend, bool)>, // (backend, enabled)
    ) -> PingoraLnBalancer<
        RoundRobin,
        MockLnClientPool,
        MockLnMetricsCache,
        StopBackoffProvider,
        RoundRobinMaxIterations,
    > {
        setup_balancer_with_backends_and_bias(should_succeed, backend_configs, 10.0, false).await
    }

    async fn setup_balancer(
        should_succeed: bool,
    ) -> PingoraLnBalancer<
        RoundRobin,
        MockLnClientPool,
        MockLnMetricsCache,
        StopBackoffProvider,
        RoundRobinMaxIterations,
    > {
        let backend = create_mock_backend("127.0.0.1:8080", "default");
        setup_balancer_with_backends(should_succeed, vec![(backend, true)]).await
    }

    #[tokio::test]
    async fn test_get_invoice_success() {
        let balancer = setup_balancer(true).await;
        let offer = create_test_offer();

        // Set metrics to allow the invoice
        let backend = create_mock_backend("127.0.0.1:8080", &offer.partition);
        balancer.metrics.set_metrics_for_backend(
            &backend,
            PingoraLnMetrics {
                healthy: true,
                node_effective_inbound_msat: 100000,
            },
        );

        let result = balancer.get_invoice(&offer, 50000, 3600, &[]).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "mock_invoice");
    }

    #[tokio::test]
    async fn test_get_invoice_pool_failure() {
        let balancer = setup_balancer(false).await;
        let offer = create_test_offer();

        // Set metrics to allow the invoice
        let backend = create_mock_backend("127.0.0.1:8080", &offer.partition);
        balancer.metrics.set_metrics_for_backend(
            &backend,
            PingoraLnMetrics {
                healthy: true,
                node_effective_inbound_msat: 100000,
            },
        );

        let result = balancer.get_invoice(&offer, 50000, 3600, &[]).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.esource(), ServiceErrorSource::Upstream);
    }

    #[tokio::test]
    async fn test_get_invoice_no_backends() {
        let pool = MockLnClientPool {
            should_succeed: true,
            backend_specific_response: false,
        };
        let metrics_cache = MockLnMetricsCache::default();

        // Create empty discovery
        let discovery = Box::new(MockServiceDiscovery {
            backends: BTreeSet::new(),
            enablement: HashMap::new(),
        });

        let load_balancer = LoadBalancer::<RoundRobin>::from_backends(Backends::new(discovery));
        let load_balancer_arc = Arc::new(load_balancer);

        let backoff_provider = StopBackoffProvider;

        let balancer = PingoraLnBalancer::new(
            load_balancer_arc,
            pool,
            metrics_cache,
            backoff_provider,
            RoundRobinMaxIterations,
            true,
            None,
        );

        let offer = create_test_offer();

        let result = balancer.get_invoice(&offer, 50000, 3600, &[]).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.esource(), ServiceErrorSource::Upstream);
    }

    #[tokio::test]
    async fn test_backend_with_multiple_partitions_can_produce_invoice() {
        let backend = create_mock_backend_with_partitions(
            "127.0.0.1:8080",
            vec!["default", "partition1", "partition2"],
        );
        let balancer = setup_balancer_with_backends(true, vec![(backend.clone(), true)]).await;

        // Set metrics to allow the invoice
        balancer.metrics.set_metrics_for_backend(
            &backend,
            PingoraLnMetrics {
                healthy: true,
                node_effective_inbound_msat: 100000,
            },
        );

        // Test all partitions
        for partition in ["default", "partition1", "partition2"] {
            let mut offer = create_test_offer();
            offer.partition = partition.to_string();
            let result = balancer.get_invoice(&offer, 50000, 3600, &[]).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "mock_invoice");
        }
    }

    #[tokio::test]
    async fn test_foreign_partition_will_never_be_selected() {
        let backend1 = create_mock_backend("127.0.0.1:8080", "partition1");
        let backend2 = create_mock_backend("127.0.0.1:8081", "partition2");

        let balancer = setup_balancer_with_backends(
            true,
            vec![(backend1.clone(), true), (backend2.clone(), true)],
        )
        .await;

        // Set metrics for both backends
        for backend in [&backend1, &backend2] {
            balancer.metrics.set_metrics_for_backend(
                backend,
                PingoraLnMetrics {
                    healthy: true,
                    node_effective_inbound_msat: 100000,
                },
            );
        }

        // Create an offer for a foreign partition that neither backend has
        let mut offer_foreign = create_test_offer();
        offer_foreign.partition = "foreign_partition".to_string();

        // This should fail because no backend has the "foreign_partition"
        let result = balancer.get_invoice(&offer_foreign, 50000, 3600, &[]).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().esource(), ServiceErrorSource::Upstream);

        // Verify that partition1 and partition2 still work
        for partition in ["partition1", "partition2"] {
            let mut offer = create_test_offer();
            offer.partition = partition.to_string();
            let result = balancer.get_invoice(&offer, 50000, 3600, &[]).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_selection_capacity_bias_negative_properly_enforces_capacity_over_weight() {
        // This test demonstrates that capacity constraints override weight preferences
        // Create backends where the higher weight backends don't meet capacity
        let backend_low_weight = create_mock_backend("127.0.0.1:8080", "default");
        let backend_high_weight_1 = create_mock_backend("127.0.0.1:8081", "default");
        let backend_high_weight_2 = create_mock_backend("127.0.0.1:8082", "default");
        let backend_high_weight_3 = create_mock_backend("127.0.0.1:8083", "default");

        // Create balancer with higher weight backends (simulating weight 3 vs 1)
        let balancer = setup_balancer_with_backends_and_bias(
            true,
            vec![
                (backend_low_weight.clone(), true),    // weight 1
                (backend_high_weight_1.clone(), true), // weight 3 (different addresses to avoid deduplication)
                (backend_high_weight_2.clone(), true),
                (backend_high_weight_3.clone(), true),
            ],
            -0.2, // Negative bias reduces effective capacity by 20%
            true, // Enable backend-specific responses
        )
        .await;

        // Set metrics: low weight has sufficient capacity, high weight backends do not
        balancer.metrics.set_metrics_for_backend(
            &backend_low_weight,
            PingoraLnMetrics {
                healthy: true,
                node_effective_inbound_msat: 100000, // 100k * 0.8 = 80k effective (sufficient for 75k)
            },
        );

        // All high weight backends have insufficient capacity
        for backend in [
            &backend_high_weight_1,
            &backend_high_weight_2,
            &backend_high_weight_3,
        ] {
            balancer.metrics.set_metrics_for_backend(
                backend,
                PingoraLnMetrics {
                    healthy: true,
                    node_effective_inbound_msat: 80000, // 80k * 0.8 = 64k effective (insufficient for 75k)
                },
            );
        }

        // Request 75k msat invoice - this should ONLY select the low weight backend
        // because it's the only one meeting capacity requirements
        let mut low_weight_count = 0;
        let mut high_weight_count = 0;

        for _ in 0..20 {
            let offer = create_test_offer();
            let result = balancer.get_invoice(&offer, 75000, 3600, &[]).await;
            assert!(result.is_ok());
            let invoice = result.unwrap();

            if invoice == "invoice_from_127.0.0.1:8080" {
                low_weight_count += 1;
            } else if invoice.starts_with("invoice_from_127.0.0.1:808") {
                high_weight_count += 1;
            }
        }

        // With proper capacity enforcement, ONLY the low weight backend should be selected
        // This proves capacity constraints override weight preferences
        assert_eq!(
            low_weight_count, 20,
            "Only the low weight backend with sufficient capacity should be selected"
        );
        assert_eq!(
            high_weight_count, 0,
            "High weight backends without sufficient capacity should never be selected"
        );

        // Now test with a smaller amount that all backends can handle
        // This will show the weight distribution when capacity constraints don't apply
        low_weight_count = 0;
        high_weight_count = 0;

        for _ in 0..20 {
            let offer = create_test_offer();
            let result = balancer.get_invoice(&offer, 50000, 3600, &[]).await; // 50k is within all backends' capacity
            assert!(result.is_ok());
            let invoice = result.unwrap();

            if invoice == "invoice_from_127.0.0.1:8080" {
                low_weight_count += 1;
            } else if invoice.starts_with("invoice_from_127.0.0.1:808") {
                high_weight_count += 1;
            }
        }

        // With 50k requests, all backends meet capacity, so weight distribution should apply
        assert!(
            low_weight_count > 0,
            "Low weight backend should be selected at least once"
        );
        assert!(
            high_weight_count > 0,
            "High weight backends should be selected when they meet capacity"
        );
        assert!(high_weight_count > low_weight_count, "High weight backends should be selected more often: low={low_weight_count}, high={high_weight_count}");
    }

    #[tokio::test]
    async fn test_selection_capacity_bias_fallback_when_no_backend_meets_capacity() {
        // This test demonstrates that when NO backends meet capacity requirements,
        // the algorithm falls back to ignoring capacity constraints to ensure service availability

        let backend = create_mock_backend("127.0.0.1:8080", "default");

        let balancer = setup_balancer_with_backends_and_bias(
            true,
            vec![(backend.clone(), true)],
            -0.2, // Negative bias reduces effective capacity by 20%
            true, // Enable backend-specific responses
        )
        .await;

        // Set metrics: backend has insufficient capacity for the requested amount
        balancer.metrics.set_metrics_for_backend(
            &backend,
            PingoraLnMetrics {
                healthy: true,
                node_effective_inbound_msat: 80000, // 80k * 0.8 = 64k effective capacity
            },
        );

        // Request 75k msat invoice - exceeds the backend's effective capacity (64k)
        // First pass: no backend meets capacity (75k > 64k effective)
        // Second pass: capacity ignored, backend is selected for service availability
        let offer = create_test_offer();
        let result = balancer.get_invoice(&offer, 75000, 3600, &[]).await;

        // Should succeed despite insufficient capacity due to fallback mechanism
        assert!(
            result.is_ok(),
            "Invoice generation should succeed via fallback mechanism"
        );
        assert_eq!(result.unwrap(), "invoice_from_127.0.0.1:8080");

        // Test multiple requests to ensure consistent behavior
        for _ in 0..5 {
            let offer = create_test_offer();
            let result = balancer.get_invoice(&offer, 75000, 3600, &[]).await;
            assert!(result.is_ok(), "All requests should succeed via fallback");
            assert_eq!(result.unwrap(), "invoice_from_127.0.0.1:8080");
        }
    }

    #[tokio::test]
    async fn test_none_selection_capacity_bias_reverts_to_pure_weight_based_selection() {
        // This test demonstrates that when selection_capacity_bias is None,
        // capacity constraints are completely ignored and selection is purely weight-based

        let backend_low_weight = create_mock_backend("127.0.0.1:8080", "default");
        let backend_high_weight_1 = create_mock_backend("127.0.0.1:8081", "default");
        let backend_high_weight_2 = create_mock_backend("127.0.0.1:8082", "default");
        let backend_high_weight_3 = create_mock_backend("127.0.0.1:8083", "default");

        // Create balancer with NO capacity bias (None)
        let balancer = setup_balancer_with_backends_and_optional_bias(
            true,
            vec![
                (backend_low_weight.clone(), true),    // weight 1
                (backend_high_weight_1.clone(), true), // weight 3 (same backend type, different addresses)
                (backend_high_weight_2.clone(), true),
                (backend_high_weight_3.clone(), true),
            ],
            None, // NO capacity bias - pure weight-based selection
            true, // Enable backend-specific responses
        )
        .await;

        // Set metrics where high weight backends have VERY LOW capacity
        // but with None bias, this should be ignored
        balancer.metrics.set_metrics_for_backend(
            &backend_low_weight,
            PingoraLnMetrics {
                healthy: true,
                node_effective_inbound_msat: 100000, // Plenty of capacity
            },
        );

        // High weight backends have very low capacity - but this should be ignored with None bias
        for backend in [
            &backend_high_weight_1,
            &backend_high_weight_2,
            &backend_high_weight_3,
        ] {
            balancer.metrics.set_metrics_for_backend(
                backend,
                PingoraLnMetrics {
                    healthy: true,
                    node_effective_inbound_msat: 10000, // Very low capacity (10k)
                },
            );
        }

        // Request 90k msat invoice - WAY more than high weight backends' capacity (10k)
        // But with None bias, capacity should be completely ignored
        let mut low_weight_count = 0;
        let mut high_weight_count = 0;

        for _ in 0..20 {
            let offer = create_test_offer();
            let result = balancer.get_invoice(&offer, 90000, 3600, &[]).await; // 90k >> 10k capacity
            assert!(result.is_ok());
            let invoice = result.unwrap();

            if invoice == "invoice_from_127.0.0.1:8080" {
                low_weight_count += 1;
            } else if invoice.starts_with("invoice_from_127.0.0.1:808") {
                high_weight_count += 1;
            }
        }

        // With None bias, capacity is completely ignored - only weight matters
        // High weight backends should be selected more often despite having insufficient capacity
        assert!(
            low_weight_count > 0,
            "Low weight backend should be selected at least once"
        );
        assert!(
            high_weight_count > 0,
            "High weight backends should be selected despite very low capacity"
        );
        assert!(
            high_weight_count > low_weight_count,
            "High weight backends should be selected more often despite insufficient capacity: low={low_weight_count}, high={high_weight_count}",
        );
        assert_eq!(
            low_weight_count + high_weight_count,
            20,
            "All requests should be handled"
        );

        // The ratio should be roughly 3:1 (high:low) due to weight distribution
        // Allow some variance due to RoundRobin starting position
        let ratio = high_weight_count as f64 / low_weight_count as f64;
        assert!(
            ratio > 1.5,
            "High weight backends should be selected significantly more often (ratio: {ratio})",
        );
    }
}
