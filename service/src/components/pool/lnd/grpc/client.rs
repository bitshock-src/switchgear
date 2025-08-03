use crate::api::service::ServiceErrorSource;
use crate::components::pool::error::{LnPoolError, LnPoolErrorSourceKind};
use crate::components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcDiscoveryBackendImplementation,
};
use crate::components::pool::{Bolt11InvoiceDescription, LnFeatures, LnMetrics, LnRpcClient};
use async_trait::async_trait;
use fedimint_tonic_lnd::lnrpc::{ChannelBalanceRequest, Invoice};
pub use fedimint_tonic_lnd::tonic;
use fedimint_tonic_lnd::{connect, Client};
use sha2::Digest;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use url::Url;

pub struct DefaultLndGrpcClient {
    timeout: Duration,
    config: LndGrpcDiscoveryBackendImplementation,
    features: Option<LnFeatures>,
    inner: Arc<Mutex<Option<Arc<InnerLndGrpcClient>>>>,
}

#[allow(clippy::result_large_err)]
impl DefaultLndGrpcClient {
    pub fn create(
        timeout: Duration,
        config: LndGrpcDiscoveryBackendImplementation,
    ) -> Result<Self, LnPoolError> {
        Ok(Self {
            timeout,
            config,
            features: Some(LnFeatures {
                invoice_from_desc_hash: true,
            }),
            inner: Arc::new(Default::default()),
        })
    }

    async fn inner_connect(&self) -> Result<Arc<InnerLndGrpcClient>, LnPoolError> {
        let mut inner = self.inner.lock().await;
        match inner.as_ref() {
            None => {
                let inner_connect = Arc::new(
                    InnerLndGrpcClient::connect(
                        self.timeout,
                        self.config.clone(),
                        self.config.url.clone(),
                    )
                    .await?,
                );
                *inner = Some(inner_connect.clone());
                Ok(inner_connect)
            }
            Some(inner) => Ok(inner.clone()),
        }
    }

    async fn inner_disconnect(&self) {
        let mut inner = self.inner.lock().await;
        *inner = None;
    }
}

#[async_trait]
impl LnRpcClient for DefaultLndGrpcClient {
    type Error = LnPoolError;

    async fn get_invoice<'a>(
        &self,
        amount_msat: Option<u64>,
        description: Bolt11InvoiceDescription<'a>,
        expiry_secs: Option<u64>,
    ) -> Result<String, Self::Error> {
        let inner = self.inner_connect().await?;

        let r = timeout(
            self.timeout,
            inner.get_invoice(amount_msat, description, expiry_secs),
        )
        .await;

        let r = match r {
            Ok(r) => r,
            Err(_) => Err(LnPoolError::from_timeout_error(
                ServiceErrorSource::Upstream,
                format!(
                    "LND get invoice from {}, requesting invoice",
                    self.config.url
                ),
            )),
        };

        if let Err(e) = &r {
            match e.source() {
                LnPoolErrorSourceKind::LndTonicError(_) | LnPoolErrorSourceKind::Timeout => {
                    self.inner_disconnect().await;
                }
                _ => {}
            }
        }
        r
    }

    async fn get_metrics(&self) -> Result<LnMetrics, Self::Error> {
        let inner = self.inner_connect().await?;

        let r = timeout(self.timeout, inner.get_metrics()).await;

        let r = match r {
            Ok(r) => r,
            Err(_) => Err(LnPoolError::from_timeout_error(
                ServiceErrorSource::Upstream,
                format!(
                    "LND get metrics for {}, requesting channels",
                    self.config.url
                ),
            )),
        };

        if let Err(e) = &r {
            match e.source() {
                LnPoolErrorSourceKind::LndTonicError(_) | LnPoolErrorSourceKind::Timeout => {
                    self.inner_disconnect().await;
                }
                _ => {}
            }
        }
        r
    }

    fn get_features(&self) -> Option<&LnFeatures> {
        self.features.as_ref()
    }
}

struct InnerLndGrpcClient {
    client: Client,
    amp_invoice: bool,
    config: LndGrpcDiscoveryBackendImplementation,
}

impl InnerLndGrpcClient {
    async fn connect(
        timeout_d: Duration,
        config: LndGrpcDiscoveryBackendImplementation,
        url: Url,
    ) -> Result<Self, LnPoolError> {
        let LndGrpcClientAuth::Path(auth) = config.auth.clone();

        let client = timeout(
            timeout_d,
            connect(
                url.as_str().to_string(),
                &auth.tls_cert_path,
                &auth.macaroon_path,
            ),
        )
        .await
        .map_err(|_| {
            LnPoolError::from_timeout_error(
                ServiceErrorSource::Upstream,
                format!("connecting LND client to {url}"),
            )
        })?
        .map_err(|e| {
            LnPoolError::from_lnd_connect_error(
                e,
                ServiceErrorSource::Upstream,
                format!("connecting LND client to {url}"),
            )
        })?;

        Ok(Self {
            client,
            amp_invoice: config.amp_invoice,
            config,
        })
    }

    async fn get_invoice<'a>(
        &self,
        amount_msat: Option<u64>,
        description: Bolt11InvoiceDescription<'a>,
        expiry_secs: Option<u64>,
    ) -> Result<String, LnPoolError> {
        let (memo, description_hash) = match description {
            Bolt11InvoiceDescription::Direct(d) => (d.to_string(), Default::default()),
            Bolt11InvoiceDescription::DirectIntoHash(d) => (
                Default::default(),
                sha2::Sha256::digest(d.as_bytes()).to_vec(),
            ),
            Bolt11InvoiceDescription::Hash(h) => (Default::default(), h.to_vec()),
        };

        let invoice_request = Invoice {
            memo,
            value_msat: amount_msat.map_or_else(Default::default, |ms| ms as i64),
            description_hash,
            expiry: expiry_secs.map_or_else(Default::default, |n| n as i64),
            is_amp: self.amp_invoice,
            ..Default::default()
        };

        let response = self
            .client
            .clone()
            .lightning()
            .add_invoice(invoice_request)
            .await
            .map_err(|e| {
                LnPoolError::from_lnd_tonic_error(
                    e,
                    format!(
                        "LND get invoice from {}, requesting invoice",
                        self.config.url
                    ),
                )
            })?
            .into_inner();

        Ok(response.payment_request)
    }

    async fn get_metrics(&self) -> Result<LnMetrics, LnPoolError> {
        let channel_balance_request = ChannelBalanceRequest {};

        let channels_balance_response = self
            .client
            .clone()
            .lightning()
            .channel_balance(channel_balance_request)
            .await
            .map_err(|e| {
                LnPoolError::from_lnd_tonic_error_with_esource(
                    e,
                    ServiceErrorSource::Upstream,
                    format!(
                        "LND get metrics for {}, requesting channels",
                        self.config.url
                    ),
                )
            })?
            .into_inner();

        let node_effective_inbound_msat = match channels_balance_response.remote_balance {
            None => 0,
            Some(remote_balance) => remote_balance.msat,
        };

        Ok(LnMetrics {
            healthy: true,
            node_effective_inbound_msat,
        })
    }
}
