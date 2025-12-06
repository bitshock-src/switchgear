use async_trait::async_trait;
use std::collections::BTreeSet;
use std::error::Error;
use switchgear_service_api::discovery::{DiscoveryBackend, DiscoveryBackends};
use switchgear_service_api::offer::Offer;

pub mod backoff;
pub mod balance;
pub mod discovery;
pub mod error;
pub mod health;

#[derive(Debug, Clone)]
pub struct PingoraLnBackendExtension {
    pub partitions: BTreeSet<String>,
}

#[async_trait]
pub trait PingoraBackendProvider {
    type Error: Error + Send + Sync + 'static;

    async fn backends(&self, etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error>;
}

#[async_trait]
pub trait PingoraLnClientPool {
    type Error: Error + Send + Sync + 'static;
    type Key: std::hash::Hash + Eq + Send + Sync + 'static;

    async fn get_invoice(
        &self,
        offer: &Offer,
        key: &Self::Key,
        amount_msat: Option<u64>,
        expiry_secs: Option<u64>,
    ) -> Result<String, Self::Error>;

    async fn get_metrics(&self, key: &Self::Key) -> Result<PingoraLnMetrics, Self::Error>;

    fn connect(&self, key: Self::Key, backend: &DiscoveryBackend) -> Result<(), Self::Error>;
}

pub trait PingoraLnMetricsCache {
    type Key: std::hash::Hash + Eq;

    fn get_cached_metrics(&self, key: &Self::Key) -> Option<PingoraLnMetrics>;
}

#[derive(Eq, PartialEq, Debug, Clone, Ord, PartialOrd)]
pub struct PingoraLnMetrics {
    pub healthy: bool,
    pub node_effective_inbound_msat: u64,
}
