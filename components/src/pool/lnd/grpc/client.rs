use crate::pool::error::LnPoolError;
use crate::pool::lnd::grpc::config::LndGrpcDiscoveryBackendImplementation;
use crate::pool::{Bolt11InvoiceDescription, LnFeatures, LnMetrics, LnRpcClient};
use crate::secrets::{SecretHeaderInterceptor, SecretStore};
use async_trait::async_trait;
use rustls::pki_types::CertificateDer;
use sha2::Digest;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use switchgear_error::ForeignContext;
use tokio::sync::Mutex;
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
    secrets: SecretStore,
}

impl TonicLndGrpcClient {
    #[tracing::instrument(skip_all)]
    pub fn create(
        timeout: Duration,
        config: LndGrpcDiscoveryBackendImplementation,
        trusted_roots: &[CertificateDer],
    ) -> Result<Self, LnPoolError> {
        let auth = &config.auth;

        let mut ca_certificates = trusted_roots
            .iter()
            .map(|c| {
                let c = Self::certificate_der_as_pem(c);
                Certificate::from_pem(&c)
            })
            .collect::<Vec<_>>();

        if let Some(tls_cert_path) = &auth.tls_cert_path {
            let ca_certificate = fs::read(tls_cert_path).with_foreign_context(
                || {
                    format!(
                        "reading LND CA certificate from {}",
                        tls_cert_path.to_string_lossy()
                    )
                },
                None,
            )?;
            ca_certificates.push(Certificate::from_pem(&ca_certificate));
        }

        let secrets = SecretStore::create(&auth.secrets);

        Ok(Self {
            timeout,
            config,
            features: Some(LnFeatures {
                invoice_from_desc_hash: true,
            }),
            inner: Arc::new(Default::default()),
            ca_certificates,
            secrets,
        })
    }

    #[tracing::instrument(skip_all)]
    async fn inner_connect(&self) -> Result<Arc<InnerTonicLndGrpcClient>, LnPoolError> {
        let mut inner = self.inner.lock().await;
        match inner.as_ref() {
            None => {
                let inner_connect = Arc::new(
                    InnerTonicLndGrpcClient::connect(
                        self.timeout,
                        self.ca_certificates.clone(),
                        self.secrets.clone(),
                        self.config.auth.macaroon_secret.clone(),
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

    #[tracing::instrument(skip_all)]
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

    #[tracing::instrument(skip_all)]
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

    #[tracing::instrument(skip_all)]
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
        tonic::service::interceptor::InterceptedService<Channel, SecretHeaderInterceptor>,
    >,
    url: String,
    amp_invoice: bool,
}

impl InnerTonicLndGrpcClient {
    #[tracing::instrument(skip_all)]
    async fn connect(
        timeout: Duration,
        ca_certificates: Vec<Certificate>,
        secrets: SecretStore,
        macaroon_secret: String,
        url: String,
        domain: Option<&str>,
        amp_invoice: bool,
    ) -> Result<Self, LnPoolError> {
        let endpoint = Channel::from_shared(url.clone())
            .with_foreign_context(|| format!("parsing LND endpoint address {url}"), None)?;

        let mut tls_config = ClientTlsConfig::new()
            .with_native_roots()
            .ca_certificates(ca_certificates);

        if let Some(domain) = domain {
            tls_config = tls_config.domain_name(domain);
        }

        let endpoint = endpoint
            .tls_config(tls_config)
            .with_foreign_context(|| format!("configuring LND TLS for {url}"), None)?;

        let channel = endpoint
            .connect_timeout(timeout)
            .timeout(timeout)
            .connect()
            .await
            .with_foreign_context(|| format!("connecting LND client to {url}"), None)?;

        let interceptor = SecretHeaderInterceptor::macaroon_hex(secrets, macaroon_secret);
        let client = LightningClient::with_interceptor(channel, interceptor);

        Ok(Self {
            client,
            url,
            amp_invoice,
        })
    }

    #[tracing::instrument(skip_all)]
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
            .with_foreign_context(|| format!("requesting LND invoice from {}", self.url), None)?
            .into_inner();

        Ok(response.payment_request)
    }

    #[tracing::instrument(skip_all)]
    async fn get_metrics(&self) -> Result<LnMetrics, LnPoolError> {
        let mut client = self.client.clone();

        let channel_balance_request = lnrpc::ChannelBalanceRequest {};
        let channels_balance_response = client
            .channel_balance(channel_balance_request)
            .await
            .with_foreign_context(
                || format!("querying LND channel balance for {}", self.url),
                None,
            )?
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
