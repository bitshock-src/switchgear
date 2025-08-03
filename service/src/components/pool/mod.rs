use crate::api::discovery::DiscoveryBackend;
use crate::api::offer::Offer;
use async_trait::async_trait;
use std::error::Error;

pub mod cln;
pub mod default_pool;
pub mod error;
pub mod lnd;

#[async_trait]
pub trait LnRpcClient {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn get_invoice<'a>(
        &self,
        amount_msat: Option<u64>,
        description: Bolt11InvoiceDescription<'a>,
        expiry_secs: Option<u64>,
    ) -> Result<String, Self::Error>;

    async fn get_metrics(&self) -> Result<LnMetrics, Self::Error>;

    fn get_features(&self) -> Option<&LnFeatures>;
}

#[async_trait]
pub trait LnClientPool {
    type Error: Error + Send + Sync + 'static;
    type Key: std::hash::Hash + Eq + Send + Sync + 'static;

    async fn get_invoice(
        &self,
        offer: &Offer,
        key: &Self::Key,
        amount_msat: Option<u64>,
        expiry_secs: Option<u64>,
    ) -> Result<String, Self::Error>;

    async fn get_metrics(&self, key: &Self::Key) -> Result<LnMetrics, Self::Error>;

    fn connect(&self, key: Self::Key, backend: &DiscoveryBackend) -> Result<(), Self::Error>;
}

pub trait LnMetricsCache {
    type Key: std::hash::Hash + Eq;

    fn get_cached_metrics(&self, key: &Self::Key) -> Option<LnMetrics>;
}

#[derive(Eq, PartialEq, Debug, Clone, Ord, PartialOrd)]
pub enum Bolt11InvoiceDescription<'a> {
    Direct(&'a str),
    DirectIntoHash(&'a str),
    Hash(&'a [u8; 32]),
}

#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct LnFeatures {
    pub invoice_from_desc_hash: bool,
}

#[derive(Eq, PartialEq, Debug, Clone, Ord, PartialOrd)]
pub struct LnMetrics {
    pub healthy: bool,
    pub node_effective_inbound_msat: u64,
}
