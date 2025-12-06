use crate::offer::Offer;
use crate::service::HasServiceErrorSource;
use async_trait::async_trait;
use std::error::Error;
use tokio::sync::watch;

#[async_trait]
pub trait LnBalancer {
    type Error: Error + Send + Sync + 'static + HasServiceErrorSource;

    async fn get_invoice(
        &self,
        offer: &Offer,
        amount_msat: u64,
        expiry_secs: u64,
        key: &[u8],
    ) -> Result<String, Self::Error>;

    async fn health(&self) -> Result<(), Self::Error>;
}

#[async_trait]
pub trait LnBalancerBackgroundServices {
    async fn start(&self, shutdown_rx: watch::Receiver<bool>);
}
