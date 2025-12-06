use crate::pool::cln::grpc::config::{ClnGrpcClientAuth, ClnGrpcDiscoveryBackendImplementation};
use crate::pool::error::LnPoolError;
use crate::pool::{Bolt11InvoiceDescription, LnFeatures, LnMetrics, LnRpcClient};
use async_trait::async_trait;
use hex::ToHex;
use rustls::pki_types::CertificateDer;
use sha2::Digest;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use switchgear_service_api::service::ServiceErrorSource;
use tokio::sync::Mutex;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

#[allow(clippy::all)]
pub mod cln {
    tonic::include_proto!("cln");
}

use cln::node_client::NodeClient;

pub struct TonicClnGrpcClient {
    timeout: Duration,
    config: ClnGrpcDiscoveryBackendImplementation,
    features: Option<LnFeatures>,
    inner: Arc<Mutex<Option<Arc<InnerTonicClnGrpcClient>>>>,
    ca_certificates: Vec<Certificate>,
    identity: Identity,
}

impl TonicClnGrpcClient {
    pub fn create(
        timeout: Duration,
        config: ClnGrpcDiscoveryBackendImplementation,
        trusted_roots: &[CertificateDer],
    ) -> Result<Self, LnPoolError> {
        let ClnGrpcClientAuth::Path(auth) = &config.auth;

        let mut ca_certificates = trusted_roots
            .iter()
            .map(|c| {
                let c = Self::certificate_der_as_pem(c);
                Certificate::from_pem(&c)
            })
            .collect::<Vec<_>>();

        if let Some(ca_cert_path) = &auth.ca_cert_path {
            let ca_certificate = fs::read(ca_cert_path).map_err(|e| {
                LnPoolError::from_invalid_credentials(
                    e.to_string(),
                    ServiceErrorSource::Internal,
                    format!(
                        "loading CLN credentials and reading CA certificate from path {}",
                        ca_cert_path.to_string_lossy()
                    ),
                )
            })?;
            ca_certificates.push(Certificate::from_pem(&ca_certificate));
        }

        let client_cert = fs::read(&auth.client_cert_path).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!(
                    "loading CLN credentials and reading client certificate from path {}",
                    auth.client_cert_path.to_string_lossy()
                ),
            )
        })?;

        let client_key = fs::read(&auth.client_key_path).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!(
                    "loading CLN credentials and reading client key from path {}",
                    auth.client_key_path.to_string_lossy()
                ),
            )
        })?;

        let identity = Identity::from_pem(client_cert, client_key);

        Ok(Self {
            timeout,
            config,
            features: Some(LnFeatures {
                invoice_from_desc_hash: false,
            }),
            inner: Arc::new(Default::default()),
            ca_certificates,
            identity,
        })
    }

    async fn inner_connect(&self) -> Result<Arc<InnerTonicClnGrpcClient>, LnPoolError> {
        let mut inner = self.inner.lock().await;
        match inner.as_ref() {
            None => {
                let inner_connect = Arc::new(
                    InnerTonicClnGrpcClient::connect(
                        self.timeout,
                        self.ca_certificates.clone(),
                        self.identity.clone(),
                        self.config.url.to_string(),
                        self.config.domain.as_deref(),
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

    fn certificate_der_as_pem(certificate: &CertificateDer) -> String {
        use base64::Engine;
        let base64_cert = base64::engine::general_purpose::STANDARD.encode(certificate.as_ref());
        format!("-----BEGIN CERTIFICATE-----\n{base64_cert}\n-----END CERTIFICATE-----")
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
    url: String,
}

impl InnerTonicClnGrpcClient {
    async fn connect(
        timeout: Duration,
        ca_certificates: Vec<Certificate>,
        identity: Identity,
        url: String,
        domain: Option<&str>,
    ) -> Result<Self, LnPoolError> {
        let endpoint = Channel::from_shared(url.clone()).map_err(|e| {
            LnPoolError::from_invalid_configuration(
                format!("Invalid endpoint URI: {}", e),
                ServiceErrorSource::Internal,
                format!("CLN connecting to endpoint address {url}"),
            )
        })?;

        let mut tls_config = ClientTlsConfig::new()
            .with_native_roots()
            .ca_certificates(ca_certificates)
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

        let channel = endpoint
            .connect_timeout(timeout)
            .timeout(timeout)
            .connect()
            .await
            .map_err(|e| {
                LnPoolError::from_transport_error(
                    e,
                    ServiceErrorSource::Upstream,
                    format!("connecting CLN client to {url}"),
                )
            })?;

        let client = NodeClient::new(channel);
        Ok(Self { client, url })
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
                (d.to_string(), Some(true), hash.encode_hex())
            }
            Bolt11InvoiceDescription::Hash(_) => {
                return Err(LnPoolError::from_invalid_configuration(
                    "hash descriptions unsupported".to_string(),
                    ServiceErrorSource::Internal,
                    format!(
                        "CLN get invoice from {}, parsing invoice description",
                        self.url
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
                    self.url
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
                LnPoolError::from_tonic_error(
                    e,
                    format!("CLN get invoice from {}, requesting invoice", self.url),
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
                LnPoolError::from_tonic_error(
                    e,
                    format!("CLN get metrics for {}, requesting channels", self.url),
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
