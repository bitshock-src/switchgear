use crate::api::discovery::{DiscoveryBackend, DiscoveryBackendImplementation};
use crate::api::offer::Offer;
use crate::api::service::ServiceErrorSource;
use crate::components::pool::cln::grpc::tonic_client::TonicClnGrpcClient;
use crate::components::pool::error::{LnPoolError, LnPoolErrorSourceKind};
use crate::components::pool::lnd::grpc::tonic_client::TonicLndGrpcClient;
use crate::components::pool::{
    Bolt11InvoiceDescription, LnClientPool, LnMetrics, LnMetricsCache, LnRpcClient,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type LnClientMap<K> =
    HashMap<K, Arc<Box<dyn LnRpcClient<Error = LnPoolError> + Send + Sync + 'static>>>;

#[derive(Clone)]
pub struct DefaultLnClientPool<K>
where
    K: Clone + std::hash::Hash + Eq,
{
    timeout: Duration,
    pool: Arc<Mutex<LnClientMap<K>>>,
    metrics_cache: Arc<Mutex<HashMap<K, LnMetrics>>>,
}

impl<K> DefaultLnClientPool<K>
where
    K: Clone + std::hash::Hash + Eq + Debug,
{
    pub fn new(timeout: Duration) -> DefaultLnClientPool<K> {
        Self {
            timeout,
            pool: Default::default(),
            metrics_cache: Default::default(),
        }
    }

    async fn get_client(
        &self,
        key: &K,
    ) -> Result<Arc<Box<dyn LnRpcClient<Error = LnPoolError> + Send + Sync + 'static>>, LnPoolError>
    {
        let pool = self.pool.lock().map_err(|e| {
            LnPoolError::new(
                LnPoolErrorSourceKind::Generic,
                ServiceErrorSource::Internal,
                e.to_string(),
            )
        })?;
        let client = pool.get(key).ok_or_else(|| {
            LnPoolError::from_invalid_configuration(
                format!("client for key: {key:?} not found in pool"),
                ServiceErrorSource::Internal,
                format!("fetching client from pool for key: {key:?}"),
            )
        })?;
        Ok(client.clone())
    }
}

#[async_trait]
impl<K> LnClientPool for DefaultLnClientPool<K>
where
    K: Clone + std::hash::Hash + Eq + Send + Sync + Debug + 'static,
{
    type Error = LnPoolError;
    type Key = K;

    async fn get_invoice(
        &self,
        offer: &Offer,
        key: &Self::Key,
        amount_msat: Option<u64>,
        expiry_secs: Option<u64>,
    ) -> Result<String, Self::Error> {
        let client = self.get_client(key).await?;

        let capabilities = client.get_features();

        let invoice_from_desc_hash =
            capabilities.map_or_else(|| false, |c| c.invoice_from_desc_hash);

        let description = if invoice_from_desc_hash {
            Bolt11InvoiceDescription::Hash(&offer.metadata_json_hash)
        } else {
            Bolt11InvoiceDescription::DirectIntoHash(offer.metadata_json_string.as_str())
        };

        Ok(client
            .get_invoice(amount_msat, description, expiry_secs)
            .await?)
    }

    async fn get_metrics(&self, key: &Self::Key) -> Result<LnMetrics, Self::Error> {
        let client = self.get_client(key).await?;

        let metrics = client.get_metrics().await?;

        let mut cache = self.metrics_cache.lock().map_err(|e| {
            LnPoolError::new(
                LnPoolErrorSourceKind::Generic,
                ServiceErrorSource::Internal,
                e.to_string(),
            )
        })?;

        cache.insert(key.clone(), metrics.clone());
        Ok(metrics)
    }

    fn connect(&self, key: Self::Key, backend: &DiscoveryBackend) -> Result<(), Self::Error> {
        let client: Box<dyn LnRpcClient<Error = LnPoolError> + std::marker::Send + Sync> =
            match &backend.backend.implementation {
                DiscoveryBackendImplementation::ClnGrpc(c) => {
                    Box::new(TonicClnGrpcClient::create(self.timeout, c.clone())?)
                }
                DiscoveryBackendImplementation::LndGrpc(c) => {
                    Box::new(TonicLndGrpcClient::create(self.timeout, c.clone())?)
                }
                DiscoveryBackendImplementation::RemoteHttp => {
                    return Err(LnPoolError::new(
                        LnPoolErrorSourceKind::Generic,
                        ServiceErrorSource::Internal,
                        "RemoteHttp backends not available",
                    ));
                }
            };

        let mut pool = self.pool.lock().map_err(|e| {
            LnPoolError::new(
                LnPoolErrorSourceKind::Generic,
                ServiceErrorSource::Internal,
                e.to_string(),
            )
        })?;
        pool.insert(key, Arc::new(client));

        Ok(())
    }
}

impl<K: Clone + std::hash::Hash + Eq> LnMetricsCache for DefaultLnClientPool<K> {
    type Key = K;
    fn get_cached_metrics(&self, key: &K) -> Option<LnMetrics> {
        match self.metrics_cache.lock() {
            Ok(cache) => cache.get(key).cloned(),
            Err(_) => None,
        }
    }
}
