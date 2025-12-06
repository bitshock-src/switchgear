mod client_pool;
pub mod cln;
pub mod error;
pub mod lnd;

use crate::pool::cln::grpc::config::ClnGrpcDiscoveryBackendImplementation;
use crate::pool::lnd::grpc::config::LndGrpcDiscoveryBackendImplementation;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use client_pool::LnClientPool;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum DiscoveryBackendImplementation {
    ClnGrpc(ClnGrpcDiscoveryBackendImplementation),
    LndGrpc(LndGrpcDiscoveryBackendImplementation),
}

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
