use crate::di::macros::{
    delegate_to_discovery_store_variants, delegate_to_ln_balancer_variants,
    delegate_to_offer_store_variants,
};
use anyhow::Result;
use async_trait::async_trait;
use pingora_load_balancing::selection::{Consistent, Random, RoundRobin};
use secp256k1::PublicKey;
use switchgear_pingora::balance::{
    ConsistentMaxIterations, RandomMaxIterations, RoundRobinMaxIterations,
};
use switchgear_pingora::error::PingoraLnError;
use switchgear_pingora::PingoraBackendProvider;
use switchgear_service::api::balance::{LnBalancer, LnBalancerBackgroundServices};
use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendStore, DiscoveryBackends,
};
use switchgear_service::api::offer::Offer;
use switchgear_service::api::offer::{OfferMetadataStore, OfferProvider, OfferStore};
use switchgear_service::components::backoff::{
    BackoffInstance, BackoffProvider, ExponentialBackoffProvider, StopBackoffProvider,
};
use switchgear_service::components::discovery::db::DbDiscoveryBackendStore;
use switchgear_service::components::discovery::error::DiscoveryBackendStoreError;
use switchgear_service::components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_service::components::discovery::memory::MemoryDiscoveryBackendStore;
use switchgear_service::components::offer::db::DbOfferStore;
use switchgear_service::components::offer::error::OfferStoreError;
use switchgear_service::components::offer::http::HttpOfferStore;
use switchgear_service::components::offer::memory::MemoryOfferStore;
use tokio::sync::watch;
use uuid::Uuid;
// ===== TYPE ALIASES =====

type Balancer<T, X> = switchgear_pingora::balance::PingoraLnBalancer<
    T,
    super::Pool,
    super::Pool,
    BackoffProviderDelegate,
    X,
>;

// ===== LN BALANCER DELEGATE =====

pub enum LnBalancerDelegate {
    RoundRobin(Balancer<RoundRobin, RoundRobinMaxIterations>),
    Random(Balancer<Random, RandomMaxIterations>),
    Consistent(Balancer<Consistent, ConsistentMaxIterations>),
}

#[async_trait]
impl LnBalancerBackgroundServices for LnBalancerDelegate {
    async fn start(&self, shutdown_rx: watch::Receiver<bool>) {
        match &self {
            LnBalancerDelegate::RoundRobin(b) => b.start(shutdown_rx).await,
            LnBalancerDelegate::Random(b) => b.start(shutdown_rx).await,
            LnBalancerDelegate::Consistent(b) => b.start(shutdown_rx).await,
        }
    }
}

impl Clone for LnBalancerDelegate {
    fn clone(&self) -> Self {
        match self {
            Self::RoundRobin(a) => Self::RoundRobin(a.clone()),
            Self::Random(a) => Self::Random(a.clone()),
            Self::Consistent(a) => Self::Consistent(a.clone()),
        }
    }
}

#[async_trait]
impl LnBalancer for LnBalancerDelegate {
    type Error = PingoraLnError;

    async fn get_invoice(
        &self,
        offer: &Offer,
        amount_msat: u64,
        expiry_secs: u64,
        key: &[u8],
    ) -> Result<String, Self::Error> {
        delegate_to_ln_balancer_variants!(self, get_invoice, offer, amount_msat, expiry_secs, key)
            .await
    }

    async fn health(&self) -> std::result::Result<(), Self::Error> {
        delegate_to_ln_balancer_variants!(self, health).await
    }
}

// ===== OFFER STORE DELEGATES =====

#[derive(Clone)]
pub enum OfferStoreDelegate {
    Database(DbOfferStore),
    Memory(MemoryOfferStore),
    Http(HttpOfferStore),
}

#[async_trait]
impl OfferStore for OfferStoreDelegate {
    type Error = OfferStoreError;

    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<switchgear_service::api::offer::OfferRecord>, Self::Error> {
        delegate_to_offer_store_variants!(self, get_offer, partition, id).await
    }

    async fn get_offers(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<switchgear_service::api::offer::OfferRecord>, Self::Error> {
        delegate_to_offer_store_variants!(self, get_offers, partition, start, count).await
    }

    async fn post_offer(
        &self,
        offer: switchgear_service::api::offer::OfferRecord,
    ) -> Result<Option<Uuid>, Self::Error> {
        delegate_to_offer_store_variants!(self, post_offer, offer).await
    }

    async fn put_offer(
        &self,
        offer: switchgear_service::api::offer::OfferRecord,
    ) -> Result<bool, Self::Error> {
        delegate_to_offer_store_variants!(self, put_offer, offer).await
    }

    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        delegate_to_offer_store_variants!(self, delete_offer, partition, id).await
    }
}

#[async_trait]
impl OfferMetadataStore for OfferStoreDelegate {
    type Error = OfferStoreError;

    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<switchgear_service::api::offer::OfferMetadata>, Self::Error> {
        delegate_to_offer_store_variants!(self, get_metadata, partition, id).await
    }

    async fn get_all_metadata(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<switchgear_service::api::offer::OfferMetadata>, Self::Error> {
        delegate_to_offer_store_variants!(self, get_all_metadata, partition, start, count).await
    }

    async fn post_metadata(
        &self,
        metadata: switchgear_service::api::offer::OfferMetadata,
    ) -> Result<Option<Uuid>, Self::Error> {
        delegate_to_offer_store_variants!(self, post_metadata, metadata).await
    }

    async fn put_metadata(
        &self,
        metadata: switchgear_service::api::offer::OfferMetadata,
    ) -> Result<bool, Self::Error> {
        delegate_to_offer_store_variants!(self, put_metadata, metadata).await
    }

    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        delegate_to_offer_store_variants!(self, delete_metadata, partition, id).await
    }
}

#[async_trait]
impl OfferProvider for OfferStoreDelegate {
    type Error = OfferStoreError;

    async fn offer(
        &self,
        hostname: &str,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<Offer>, Self::Error> {
        delegate_to_offer_store_variants!(self, offer, hostname, partition, id).await
    }
}

// ===== DISCOVERY BACKEND STORE DELEGATE =====

#[derive(Clone)]
pub enum DiscoveryBackendStoreDelegate {
    Database(DbDiscoveryBackendStore),
    Memory(MemoryDiscoveryBackendStore),
    Http(HttpDiscoveryBackendStore),
}

#[async_trait]
impl DiscoveryBackendStore for DiscoveryBackendStoreDelegate {
    type Error = DiscoveryBackendStoreError;

    async fn get(&self, public_key: &PublicKey) -> Result<Option<DiscoveryBackend>, Self::Error> {
        delegate_to_discovery_store_variants!(self, get, public_key).await
    }

    async fn get_all(&self, etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        delegate_to_discovery_store_variants!(self, get_all, etag).await
    }

    async fn post(&self, backend: DiscoveryBackend) -> Result<Option<PublicKey>, Self::Error> {
        delegate_to_discovery_store_variants!(self, post, backend).await
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        delegate_to_discovery_store_variants!(self, put, backend).await
    }

    async fn patch(
        &self,
        backend: DiscoveryBackendPatch,
    ) -> std::result::Result<bool, Self::Error> {
        delegate_to_discovery_store_variants!(self, patch, backend).await
    }

    async fn delete(&self, public_key: &PublicKey) -> Result<bool, Self::Error> {
        delegate_to_discovery_store_variants!(self, delete, public_key).await
    }
}

#[async_trait]
impl PingoraBackendProvider for DiscoveryBackendStoreDelegate {
    type Error = PingoraLnError;

    async fn backends(&self, etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        delegate_to_discovery_store_variants!(self, backends, etag).await
    }
}

// ===== BACKOFF PROVIDER DELEGATE =====

#[derive(Clone)]
pub enum BackoffProviderDelegate {
    Stop(StopBackoffProvider),
    Exponential(ExponentialBackoffProvider),
}

impl BackoffProvider for BackoffProviderDelegate {
    type Item = BackoffInstance;

    fn get_backoff(&self) -> Self::Item {
        match self {
            Self::Stop(provider) => BackoffInstance::Stop(provider.get_backoff()),
            Self::Exponential(provider) => BackoffInstance::Exponential(provider.get_backoff()),
        }
    }
}
