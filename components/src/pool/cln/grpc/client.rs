use crate::metrics::grpc::record_rpc_call;
use crate::pool::cln::grpc::config::{ClnGrpcClientAuth, ClnGrpcDiscoveryBackendImplementation};
use crate::pool::error::LnPoolError;
use crate::pool::{Bolt11InvoiceDescription, LnFeatures, LnMetrics, LnRpcClient};
use crate::secrets::{ClientCertResolver, SecretStore};
use async_trait::async_trait;
use hex::ToHex;
use hyper_rustls::{FixedServerNameResolver, HttpsConnectorBuilder};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use sha2::Digest;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use switchgear_error::{ErrorOrigin, ForeignContext};
use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};
use url::Url;

#[allow(clippy::all)]
pub mod cln {
    tonic::include_proto!("cln");
}

use cln::node_client::NodeClient;

pub struct TonicClnGrpcClient {
    timeout: Duration,
    config: ClnGrpcDiscoveryBackendImplementation,
    trusted_roots: Vec<CertificateDer<'static>>,
    features: Option<LnFeatures>,
    inner: Arc<Mutex<Option<Arc<InnerTonicClnGrpcClient>>>>,
    secrets: SecretStore,
}

impl TonicClnGrpcClient {
    pub fn create(
        timeout: Duration,
        config: ClnGrpcDiscoveryBackendImplementation,
        trusted_roots: &[CertificateDer],
    ) -> Result<Self, LnPoolError> {
        let secrets = SecretStore::create(&config.auth.secrets);
        Ok(Self {
            timeout,
            config,
            trusted_roots: trusted_roots
                .iter()
                .map(|c| c.clone().into_owned())
                .collect(),
            features: Some(LnFeatures {
                invoice_from_desc_hash: false,
            }),
            inner: Arc::new(Default::default()),
            secrets,
        })
    }

    async fn inner_connect(&self) -> Result<Arc<InnerTonicClnGrpcClient>, LnPoolError> {
        let mut inner = self.inner.lock().await;
        match inner.as_ref() {
            None => {
                let inner_connect = Arc::new(
                    InnerTonicClnGrpcClient::connect(
                        self.timeout,
                        &self.trusted_roots,
                        &self.config.auth,
                        self.secrets.clone(),
                        self.config.url.clone(),
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
}

#[async_trait]
impl LnRpcClient for TonicClnGrpcClient {
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

    #[tracing::instrument(skip_all)]
    fn get_features(&self) -> Option<&LnFeatures> {
        self.features.as_ref()
    }
}

struct InnerTonicClnGrpcClient {
    client: NodeClient<Channel>,
    url: Url,
    server_address: String,
    server_port: u16,
}

impl InnerTonicClnGrpcClient {
    async fn connect(
        timeout: Duration,
        trusted_roots: &[CertificateDer<'static>],
        auth: &ClnGrpcClientAuth,
        secrets: SecretStore,
        url: Url,
        domain: Option<&str>,
    ) -> Result<Self, LnPoolError> {
        let tls_config = build_client_tls_config(trusted_roots, auth, &secrets)?;

        let mut connector_builder = HttpsConnectorBuilder::new()
            .with_tls_config(tls_config)
            .https_or_http();
        if let Some(domain) = domain {
            let name: ServerName<'static> =
                ServerName::try_from(domain.to_owned()).map_err(|e| {
                    LnPoolError::message(
                        ErrorOrigin::Internal,
                        format!("parsing CLN domain {domain} as TLS server name: {e}"),
                    )
                })?;
            connector_builder =
                connector_builder.with_server_name_resolver(FixedServerNameResolver::new(name));
        }
        let https = connector_builder.enable_http2().build();

        let endpoint = Endpoint::from_shared(url.to_string())
            .with_foreign_context(|| format!("parsing CLN endpoint address {url}"), None)?
            .connect_timeout(timeout)
            .timeout(timeout);

        let channel = Channel::new(https, endpoint);

        let server_address = url.host_str().unwrap_or_default().to_owned();
        let server_port = url.port_or_known_default().unwrap_or(0);

        Ok(Self {
            client: NodeClient::new(channel),
            url,
            server_address,
            server_port,
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
                (d.to_string(), Some(true), hash.encode_hex())
            }
            Bolt11InvoiceDescription::Hash(_) => {
                return Err(LnPoolError::message(
                    ErrorOrigin::Internal,
                    format!(
                        "generating CLN invoice from {}, resolving invoice description: hash descriptions unsupported",
                        self.url
                    ),
                ));
            }
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .with_foreign_context(
                || format!("computing invoice label timestamp for CLN {}", self.url),
                None,
            )?;
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

        let started = Instant::now();
        let response = client.invoice(request).await;
        record_rpc_call(
            started.elapsed(),
            "cln.Node/Invoice",
            &self.server_address,
            self.server_port,
            &response,
        );
        let response = response
            .with_foreign_context(|| format!("requesting CLN invoice from {}", self.url), None)?
            .into_inner();

        Ok(response.bolt11)
    }

    async fn get_metrics(&self) -> Result<LnMetrics, LnPoolError> {
        let channels_request = cln::ListpeerchannelsRequest {
            id: None,
            short_channel_id: None,
        };
        let mut client = self.client.clone();
        let started = Instant::now();
        let channels_response = client.list_peer_channels(channels_request).await;
        record_rpc_call(
            started.elapsed(),
            "cln.Node/ListPeerChannels",
            &self.server_address,
            self.server_port,
            &channels_response,
        );
        let channels_response = channels_response
            .with_foreign_context(
                || format!("listing CLN peer channels for {}", self.url),
                None,
            )?
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

fn build_client_tls_config(
    trusted_roots: &[CertificateDer<'static>],
    auth: &ClnGrpcClientAuth,
    secrets: &SecretStore,
) -> Result<ClientConfig, LnPoolError> {
    let crypto = CryptoProvider::get_default()
        .ok_or_else(|| {
            LnPoolError::message(
                ErrorOrigin::Internal,
                "no rustls crypto provider installed for CLN gRPC client",
            )
        })?
        .clone();

    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    for root in trusted_roots {
        roots.add(root.clone()).with_foreign_context(
            || "adding pool trusted root to CLN roots".to_string(),
            ErrorOrigin::Internal,
        )?;
    }
    if let Some(ca_cert_path) = auth.ca_cert_path.as_ref() {
        let pem = std::fs::read(ca_cert_path).with_foreign_context(
            || format!("reading CLN CA certificate from {}", ca_cert_path.display()),
            None,
        )?;
        let extra: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&pem)
            .collect::<Result<_, _>>()
            .with_foreign_context(
                || format!("parsing CLN CA certificate from {}", ca_cert_path.display()),
                None,
            )?;
        roots.add_parsable_certificates(extra);
    }

    Ok(ClientConfig::builder_with_provider(crypto.clone())
        .with_safe_default_protocol_versions()
        .foreign_context(
            "configuring CLN gRPC client TLS protocol versions",
            ErrorOrigin::Internal,
        )?
        .with_root_certificates(roots)
        .with_client_cert_resolver(Arc::new(ClientCertResolver::new(
            secrets.clone(),
            auth.client_cert_secret.clone(),
            auth.client_key_secret.clone(),
            crypto,
        ))))
}
