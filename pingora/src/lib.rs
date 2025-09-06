use async_trait::async_trait;
use std::collections::BTreeSet;
use std::error::Error;
use switchgear_service::api::discovery::DiscoveryBackend;

pub mod balance;
pub mod discovery;
pub mod error;
pub mod health;
mod socket;

#[derive(Debug, Clone)]
pub struct PingoraLnBackendExtension {
    pub partitions: BTreeSet<String>,
}

#[async_trait]
pub trait PingoraBackendProvider {
    type Error: Error + Send + Sync + 'static;

    async fn backends(&self) -> Result<Vec<DiscoveryBackend>, Self::Error>;
}
