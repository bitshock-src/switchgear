pub mod backoff;
pub mod balance;
pub mod discovery;
pub mod error;
pub mod health;
pub mod pool;

use ::backoff::backoff::Backoff;
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::error::Error;
use switchgear_service_api::discovery::{DiscoveryBackend, DiscoveryBackends};
use switchgear_service_api::offer::Offer;
use switchgear_service_api::service::HasServiceErrorSource;

#[derive(Debug, Clone)]
pub struct PingoraLnBackendExtension {
    pub partitions: BTreeSet<String>,
}

#[async_trait]
pub trait PingoraBackendProvider {
    async fn backends(&self, etag: Option<u64>)
        -> Result<DiscoveryBackends, pingora_error::BError>;
}

#[async_trait]
pub trait PingoraLnClientPool {
    type Error: Error + Send + Sync + 'static + HasServiceErrorSource;
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

pub trait PingoraBackoffProvider: Clone + Send + Sync {
    type Item: Backoff + Send;

    fn get_backoff(&self) -> Self::Item;
}
