use crate::api::service::ServiceErrorSource;
use crate::components::pool::error::LnPoolError;
use crate::components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcDiscoveryBackendImplementation,
};
use crate::components::pool::{Bolt11InvoiceDescription, LnFeatures, LnMetrics, LnRpcClient};
use async_trait::async_trait;
use rustls::pki_types::CertificateDer;
use sha2::Digest;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tonic::service::Interceptor;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

#[allow(clippy::all)]
pub mod lnrpc {
    tonic::include_proto!("lnrpc");
}

use lnrpc::lightning_client::LightningClient;

pub struct TonicLndGrpcClient {
    timeout: Duration,
    config: LndGrpcDiscoveryBackendImplementation,
    features: Option<LnFeatures>,
    inner: Arc<Mutex<Option<Arc<InnerTonicLndGrpcClient>>>>,
    ca_certificates: Vec<Certificate>,
    macaroon: String,
}

impl TonicLndGrpcClient {
    pub fn create(
        timeout: Duration,
        config: LndGrpcDiscoveryBackendImplementation,
        trusted_roots: &[CertificateDer],
    ) -> Result<Self, LnPoolError> {
        let LndGrpcClientAuth::Path(auth) = &config.auth;

        let mut ca_certificates = trusted_roots
            .iter()
            .map(|c| {
                let c = Self::certificate_der_as_pem(c);
                Certificate::from_pem(&c)
            })
            .collect::<Vec<_>>();

        if let Some(tls_cert_path) = &auth.tls_cert_path {
            let ca_certificate = fs::read(tls_cert_path).map_err(|e| {
                LnPoolError::from_invalid_credentials(
                    e.to_string(),
                    ServiceErrorSource::Internal,
                    format!(
                        "loading LND credentials and reading CA certificate from path {}",
                        tls_cert_path.to_string_lossy()
                    ),
                )
            })?;
            ca_certificates.push(Certificate::from_pem(&ca_certificate));
        }

        let macaroon = fs::read(&auth.macaroon_path).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!(
                    "loading LND macaroon from {}",
                    auth.macaroon_path.to_string_lossy()
                ),
            )
        })?;
        let macaroon = hex::encode(&macaroon);

        Ok(Self {
            timeout,
            config,
            features: Some(LnFeatures {
                invoice_from_desc_hash: true,
            }),
            inner: Arc::new(Default::default()),
            ca_certificates,
            macaroon,
        })
    }

    async fn inner_connect(&self) -> Result<Arc<InnerTonicLndGrpcClient>, LnPoolError> {
        let mut inner = self.inner.lock().await;
        match inner.as_ref() {
            None => {
                let inner_connect = Arc::new(
                    InnerTonicLndGrpcClient::connect(
                        self.timeout,
                        self.ca_certificates.clone(),
                        self.macaroon.clone(),
                        self.config.url.to_string(),
                        self.config.domain.as_deref(),
                        self.config.amp_invoice,
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
impl LnRpcClient for TonicLndGrpcClient {
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

struct InnerTonicLndGrpcClient {
    client: LightningClient<
        tonic::service::interceptor::InterceptedService<Channel, MacaroonInterceptor>,
    >,
    url: String,
    amp_invoice: bool,
}

impl InnerTonicLndGrpcClient {
    async fn connect(
        timeout: Duration,
        ca_certificates: Vec<Certificate>,
        macaroon: String,
        url: String,
        domain: Option<&str>,
        amp_invoice: bool,
    ) -> Result<Self, LnPoolError> {
        let endpoint = Channel::from_shared(url.clone()).map_err(|e| {
            LnPoolError::from_invalid_configuration(
                format!("Invalid endpoint URI: {}", e),
                ServiceErrorSource::Internal,
                format!("LND connecting to endpoint address {url}"),
            )
        })?;

        let mut tls_config = ClientTlsConfig::new()
            .with_native_roots()
            .ca_certificates(ca_certificates);

        if let Some(domain) = domain {
            tls_config = tls_config.domain_name(domain);
        }

        let endpoint = endpoint.tls_config(tls_config).map_err(|e| {
            LnPoolError::from_invalid_credentials(
                e.to_string(),
                ServiceErrorSource::Internal,
                format!("loading LND TLS configuration into client for {url}"),
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
                    format!("connecting LND client to {url}"),
                )
            })?;

        let interceptor = MacaroonInterceptor { macaroon };

        let client = LightningClient::with_interceptor(channel, interceptor);
        Ok(Self {
            client,
            url,
            amp_invoice,
        })
    }

    async fn get_invoice<'a>(
        &self,
        amount_msat: Option<u64>,
        description: Bolt11InvoiceDescription<'a>,
        expiry_secs: Option<u64>,
    ) -> Result<String, LnPoolError> {
        let mut client = self.client.clone();

        let (memo, description_hash) = match description {
            Bolt11InvoiceDescription::Direct(d) => (d.to_string(), vec![]),
            Bolt11InvoiceDescription::DirectIntoHash(d) => {
                (String::new(), sha2::Sha256::digest(d.as_bytes()).to_vec())
            }
            Bolt11InvoiceDescription::Hash(h) => (String::new(), h.to_vec()),
        };

        let invoice_request = lnrpc::Invoice {
            memo,
            value_msat: amount_msat.unwrap_or(0) as i64,
            description_hash,
            expiry: expiry_secs.unwrap_or(3600) as i64,
            is_amp: self.amp_invoice,
            ..Default::default()
        };

        let response = client
            .add_invoice(invoice_request)
            .await
            .map_err(|e| {
                LnPoolError::from_tonic_error(
                    e,
                    format!("LND get invoice from {}, requesting invoice", self.url),
                )
            })?
            .into_inner();

        Ok(response.payment_request)
    }

    async fn get_metrics(&self) -> Result<LnMetrics, LnPoolError> {
        let mut client = self.client.clone();

        let channel_balance_request = lnrpc::ChannelBalanceRequest {};
        let channels_balance_response = client
            .channel_balance(channel_balance_request)
            .await
            .map_err(|e| {
                LnPoolError::from_tonic_error(
                    e,
                    format!("LND get metrics for {}, requesting channels", self.url),
                )
            })?
            .into_inner();

        let node_effective_inbound_msat = channels_balance_response
            .remote_balance
            .map(|balance| balance.msat)
            .unwrap_or(0);

        Ok(LnMetrics {
            healthy: true,
            node_effective_inbound_msat,
        })
    }
}

#[derive(Clone)]
struct MacaroonInterceptor {
    macaroon: String,
}

impl Interceptor for MacaroonInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        req.metadata_mut().insert(
            "macaroon",
            tonic::metadata::MetadataValue::try_from(self.macaroon.clone())
                .map_err(|_| tonic::Status::invalid_argument("Invalid macaroon"))?,
        );
        Ok(req)
    }
}
