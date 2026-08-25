use crate::{
    PingoraLnClientPool, PingoraLnClientPoolError, PingoraLnMetrics, PingoraLnMetricsCache,
};
use async_trait::async_trait;
use pingora_load_balancing::Backend;
use switchgear_components::pool::LnClientPool;
use switchgear_components::pool::error::LnPoolError;
use switchgear_error::IntoBoxedTrait;
use switchgear_service_api::discovery::DiscoveryBackend;
use switchgear_service_api::offer::Offer;

impl PingoraLnClientPoolError for LnPoolError {}

impl IntoBoxedTrait<dyn PingoraLnClientPoolError> for LnPoolError {
    fn into_boxed(self) -> Box<dyn PingoraLnClientPoolError> {
        Box::new(self)
    }
}

#[derive(Clone)]
pub struct DefaultPingoraLnClientPool {
    pool: LnClientPool<Backend>,
}

impl DefaultPingoraLnClientPool {
    pub fn new(pool: LnClientPool<Backend>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PingoraLnClientPool for DefaultPingoraLnClientPool {
    type Error = LnPoolError;
    type Key = Backend;

    #[tracing::instrument(skip_all)]
    async fn get_invoice(
        &self,
        offer: &Offer,
        key: &Self::Key,
        amount_msat: Option<u64>,
        expiry_secs: Option<u64>,
    ) -> Result<String, Self::Error> {
        self.pool
            .get_invoice(offer, key, amount_msat, expiry_secs)
            .await
    }

    #[tracing::instrument(skip_all)]
    async fn get_metrics(&self, key: &Self::Key) -> Result<PingoraLnMetrics, Self::Error> {
        let metrics = self.pool.get_metrics(key).await?;
        Ok(PingoraLnMetrics {
            healthy: metrics.healthy,
            node_effective_inbound_msat: metrics.node_effective_inbound_msat,
        })
    }

    #[tracing::instrument(skip_all)]
    fn connect(&self, key: Self::Key, backend: &DiscoveryBackend) -> Result<(), Self::Error> {
        self.pool.connect(key, backend)
    }
}

impl PingoraLnMetricsCache for DefaultPingoraLnClientPool {
    type Key = Backend;

    fn get_cached_metrics(&self, key: &Self::Key) -> Option<PingoraLnMetrics> {
        let metrics = self.pool.get_cached_metrics(key);
        metrics.map(|m| PingoraLnMetrics {
            healthy: m.healthy,
            node_effective_inbound_msat: m.node_effective_inbound_msat,
        })
    }
}
