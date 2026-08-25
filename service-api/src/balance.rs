use crate::offer::Offer;
use async_trait::async_trait;
use switchgear_error::ContextError;
use switchgear_error::IntoBoxedTrait;
use tokio::sync::watch;

pub trait LnBalancerError: ContextError {}

#[async_trait]
pub trait LnBalancer {
    type Error: LnBalancerError + IntoBoxedTrait<dyn LnBalancerError>;

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
