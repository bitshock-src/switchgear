use crate::pool::cln::grpc::client::TonicClnGrpcClient;
use crate::pool::error::LnPoolError;
use crate::pool::lnd::grpc::client::TonicLndGrpcClient;
use crate::pool::{
    Bolt11InvoiceDescription, DiscoveryBackendImplementation, LnMetrics, LnRpcClient,
};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use switchgear_service_api::discovery::DiscoveryBackend;
use switchgear_service_api::offer::Offer;
use switchgear_service_api::service::ServiceErrorSource;
use tonic::transport::CertificateDer;

type LnClientMap<K> =
    HashMap<K, Arc<Box<dyn LnRpcClient<Error = LnPoolError> + Send + Sync + 'static>>>;

#[derive(Clone)]
pub struct LnClientPool<K>
where
    K: Clone + std::hash::Hash + Eq,
{
    timeout: Duration,
    pool: Arc<Mutex<LnClientMap<K>>>,
    metrics_cache: Arc<Mutex<HashMap<K, LnMetrics>>>,
    trusted_roots: Vec<CertificateDer<'static>>,
}

impl<K> LnClientPool<K>
where
    K: Clone + std::hash::Hash + Eq + Debug,
{
    pub fn new(timeout: Duration, trusted_roots: Vec<CertificateDer<'static>>) -> LnClientPool<K> {
        Self {
            timeout,
            pool: Default::default(),
            metrics_cache: Default::default(),
            trusted_roots,
        }
    }

    async fn get_client(
        &self,
        key: &K,
    ) -> Result<Arc<Box<dyn LnRpcClient<Error = LnPoolError> + Send + Sync + 'static>>, LnPoolError>
    {
        let pool = self.pool.lock().map_err(|e| {
            LnPoolError::from_memory_error(
                e.to_string(),
                format!("fetching client from pool for key: {key:?}"),
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

    pub async fn get_invoice(
        &self,
        offer: &Offer,
        key: &K,
        amount_msat: Option<u64>,
        expiry_secs: Option<u64>,
    ) -> Result<String, LnPoolError> {
        let client = self.get_client(key).await?;

        let capabilities = client.get_features();

        let invoice_from_desc_hash =
            capabilities.map_or_else(|| false, |c| c.invoice_from_desc_hash);

        let description = if invoice_from_desc_hash {
            Bolt11InvoiceDescription::Hash(&offer.metadata_json_hash)
        } else {
            Bolt11InvoiceDescription::DirectIntoHash(offer.metadata_json_string.as_str())
        };

        client
            .get_invoice(amount_msat, description, expiry_secs)
            .await
    }

    pub async fn get_metrics(&self, key: &K) -> Result<LnMetrics, LnPoolError> {
        let client = self.get_client(key).await?;

        let metrics = client.get_metrics().await?;

        let mut cache = self.metrics_cache.lock().map_err(|e| {
            LnPoolError::from_memory_error(e.to_string(), format!("get node metrics key: {key:?}"))
        })?;

        cache.insert(key.clone(), metrics.clone());
        Ok(metrics)
    }

    pub fn connect(&self, key: K, backend: &DiscoveryBackend) -> Result<(), LnPoolError> {
        let implementation: DiscoveryBackendImplementation =
            serde_json::from_slice(backend.backend.implementation.as_slice())
                .map_err(|e| LnPoolError::from_json_error(e, "parsing backend implementation"))?;
        let client: Box<dyn LnRpcClient<Error = LnPoolError> + Send + Sync> = match implementation {
            DiscoveryBackendImplementation::ClnGrpc(implementation) => Box::new(
                TonicClnGrpcClient::create(self.timeout, implementation, &self.trusted_roots)?,
            ),
            DiscoveryBackendImplementation::LndGrpc(implementation) => Box::new(
                TonicLndGrpcClient::create(self.timeout, implementation, &self.trusted_roots)?,
            ),
        };

        let mut pool = self.pool.lock().map_err(|e| {
            LnPoolError::from_memory_error(e.to_string(), format!("connecting ln client {key:?}"))
        })?;
        pool.insert(key, Arc::new(client));

        Ok(())
    }

    pub fn get_cached_metrics(&self, key: &K) -> Option<LnMetrics> {
        match self.metrics_cache.lock() {
            Ok(cache) => cache.get(key).cloned(),
            Err(_) => None,
        }
    }
}
