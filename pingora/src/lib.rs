use async_trait::async_trait;
use std::collections::HashSet;
use std::error::Error;
use switchgear_service::api::discovery::DiscoveryBackend;

pub mod balance;
pub mod discovery;
pub mod error;
pub mod health;
mod socket;

#[derive(Debug, Clone)]
pub struct PingoraLnBackendExtension {
    pub partitions: HashSet<String>,
}

#[async_trait]
pub trait PingoraBackendProvider {
    type Error: Error + Send + Sync + 'static;

    async fn backends(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error>;
}
