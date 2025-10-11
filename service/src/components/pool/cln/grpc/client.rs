use crate::api::service::ServiceErrorSource;
use crate::components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcClientAuthPath, ClnGrpcDiscoveryBackendImplementation,
};
use crate::components::pool::error::LnPoolError;
use crate::components::pool::{Bolt11InvoiceDescription, LnFeatures, LnMetrics, LnRpcClient};
use async_trait::async_trait;
use secp256k1::hashes::hex::DisplayHex;
use sha2::Digest;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use url::Url;

#[allow(clippy::all)]
pub mod cln {
    tonic::include_proto!("cln");
}

use cln::node_client::NodeClient;

type ClientCredentials = (Vec<u8>, Vec<u8>, Vec<u8>);

pub struct TonicClnGrpcClient {
    timeout: Duration,
    config: ClnGrpcDiscoveryBackendImplementation,
    features: Option<LnFeatures>,
    inner: Arc<Mutex<Option<Arc<InnerTonicClnGrpcClient>>>>,
}

impl TonicClnGrpcClient {
    pub fn create(
        timeout: Duration,
        config: ClnGrpcDiscoveryBackendImplementation,
    ) -> Result<Self, LnPoolError> {
        Ok(Self {
            timeout,
            config,
            features: Some(LnFeatures {
                invoice_from_desc_hash: false,
            }),
            inner: Arc::new(Default::default()),
        })
    }

    async fn inner_connect(&self) -> Result<Arc<InnerTonicClnGrpcClient>, LnPoolError> {
        let mut inner = self.inner.lock().await;
        match inner.as_ref() {
            None => {
                let inner_connect = Arc::new(
                    InnerTonicClnGrpcClient::connect(
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
impl LnRpcClient for TonicClnGrpcClient {
    type Error = LnPoolError;

    async fn get_invoice<'a>(
        &self,
        amount_msat: Option<u64>,
        description: Bolt11InvoiceDescription<'a>,
        expiry_secs: Option<u64>,
    ) -> Result<String, Self::Error> {
        let inner = self.inner_connect().await?;

        let r = inner
            .get_invoice(amount_msat, description, expiry_secs)
            .await;

        if r.is_err() {
            self.inner_disconnect().await;
        }
        r
    }

    async fn get_metrics(&self) -> Result<LnMetrics, Self::Error> {
        let inner = self.inner_connect().await?;

        let r = inner.get_metrics().await;

        if r.is_err() {
            self.inner_disconnect().await;
        }
        r
    }

    fn get_features(&self) -> Option<&LnFeatures> {
        self.features.as_ref()
    }
}

struct InnerTonicClnGrpcClient {
    client: NodeClient<Channel>,
    config: ClnGrpcDiscoveryBackendImplementation,
}

impl InnerTonicClnGrpcClient {
    async fn connect(
        timeout: Duration,
        config: ClnGrpcDiscoveryBackendImplementation,
        url: Url,
    ) -> Result<Self, LnPoolError> {
        let ClnGrpcClientAuth::Path(auth) = config.auth.clone();

        let (ca_cert_data, client_cert_data, client_key_data) =
            Self::load_client_credentials(&auth)?;

        let endpoint = Channel::from_shared(url.to_string()).map_err(|e| {
            LnPoolError::from_invalid_configuration(
                format!("Invalid endpoint URI: {}", e),
                ServiceErrorSource::Internal,
                format!("CLN connecting to endpoint address {url}"),
            )
        })?;

        let channel = Self::connect_with_tls(
            timeout,
            &url,
            endpoint,
            &ca_cert_data,
            &client_cert_data,
            &client_key_data,
            config.domain.as_deref(),
        )
        .await?;

        let client = NodeClient::new(channel);
        Ok(Self { client, config })
    }

    fn load_client_credentials(
        auth: &ClnGrpcClientAuthPath,
    ) -> Result<ClientCredentials, LnPoolError> {
        let ca_cert_path = &auth.ca_cert_path;
        let client_cert_path = &auth.client_cert_path;
        let client_key_path = &auth.client_key_path;

        let ca_cert = fs::read(ca_cert_path).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!(
                    "loading CLN credentials and reading CA certificate from path {}",
                    ca_cert_path.to_string_lossy()
                ),
            )
        })?;

        let client_cert = fs::read(client_cert_path).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!(
                    "loading CLN credentials and reading client certificate from path {}",
                    client_cert_path.to_string_lossy()
                ),
            )
        })?;

        let client_key = fs::read(client_key_path).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!(
                    "loading CLN credentials and reading client key from path {}",
                    client_key_path.to_string_lossy()
                ),
            )
        })?;

        Ok((ca_cert, client_cert, client_key))
    }

    async fn connect_with_tls(
        timeout: Duration,
        url: &Url,
        endpoint: Endpoint,
        ca_cert: &[u8],
        client_cert: &[u8],
        client_key: &[u8],
        domain: Option<&str>,
    ) -> Result<Channel, LnPoolError> {
        let ca_cert = Certificate::from_pem(ca_cert);
        let identity = Identity::from_pem(client_cert, client_key);

        let mut tls_config = ClientTlsConfig::new()
            .ca_certificate(ca_cert)
            .identity(identity);

        if let Some(domain) = domain {
            tls_config = tls_config.domain_name(domain);
        }

        let endpoint = endpoint.tls_config(tls_config).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!("loading CLN TLS configuration into client for {url}"),
            )
        })?;

        endpoint
            .connect_timeout(timeout)
            .timeout(timeout)
            .connect()
            .await
            .map_err(|e| {
                LnPoolError::from_cln_transport_error(
                    e,
                    ServiceErrorSource::Upstream,
                    format!("connecting CLN client to {url}"),
                )
            })
    }

    async fn get_invoice<'a>(
        &self,
        amount_msat: Option<u64>,
        description: Bolt11InvoiceDescription<'a>,
        expiry_secs: Option<u64>,
    ) -> Result<String, LnPoolError> {
        let (description_str, deschashonly, label) = match description {
            Bolt11InvoiceDescription::Direct(d) => (d.to_string(), Some(false), d.to_string()),
            Bolt11InvoiceDescription::DirectIntoHash(d) => {
                let hash = sha2::Sha256::digest(d.as_bytes()).to_vec();
                (d.to_string(), Some(true), hash.to_lower_hex_string())
            }
            Bolt11InvoiceDescription::Hash(_) => {
                return Err(LnPoolError::from_invalid_configuration(
                    "hash descriptions unsupported".to_string(),
                    ServiceErrorSource::Internal,
                    format!(
                        "CLN get invoice from {}, parsing invoice description",
                        self.config.url
                    ),
                ))
            }
        };

        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| {
            LnPoolError::from_invalid_configuration(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!(
                    "CLN get invoice from {}, getting current time for label",
                    self.config.url
                ),
            )
        })?;
        let label = format!("{label}:{}", now.as_nanos());

        let mut client = self.client.clone();
        let request = cln::InvoiceRequest {
            amount_msat: match amount_msat {
                Some(msat) => Some(cln::AmountOrAny {
                    value: Some(cln::amount_or_any::Value::Amount(cln::Amount { msat })),
                }),
                None => Some(cln::AmountOrAny {
                    value: Some(cln::amount_or_any::Value::Any(true)),
                }),
            },
            description: description_str,
            label,
            deschashonly,
            expiry: expiry_secs,
            ..Default::default()
        };

        let response = client
            .invoice(request)
            .await
            .map_err(|e| {
                LnPoolError::from_cln_tonic_error(
                    e,
                    format!(
                        "CLN get invoice from {}, requesting invoice",
                        self.config.url
                    ),
                )
            })?
            .into_inner();

        Ok(response.bolt11)
    }

    async fn get_metrics(&self) -> Result<LnMetrics, LnPoolError> {
        let channels_request = cln::ListpeerchannelsRequest {
            id: None,
            short_channel_id: None,
        };
        let mut client = self.client.clone();
        let channels_response = client
            .list_peer_channels(channels_request)
            .await
            .map_err(|e| {
                LnPoolError::from_cln_tonic_error(
                    e,
                    format!(
                        "CLN get metrics for {}, requesting channels",
                        self.config.url
                    ),
                )
            })?
            .into_inner();

        let mut node_effective_inbound_msat = 0u64;

        const CHANNELD_NORMAL: i32 = 2;

        for channel in &channels_response.channels {
            if channel.state == CHANNELD_NORMAL {
                let receivable_msat = channel
                    .receivable_msat
                    .as_ref()
                    .map(|a| a.msat)
                    .unwrap_or(0);
                node_effective_inbound_msat += receivable_msat;
            }
        }

        Ok(LnMetrics {
            healthy: true,
            node_effective_inbound_msat,
        })
    }
}
