use crate::api::service::ServiceErrorSource;
use crate::components::pool::error::LnPoolError;
use crate::components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcDiscoveryBackendImplementation,
};
use crate::components::pool::{Bolt11InvoiceDescription, LnFeatures, LnMetrics, LnRpcClient};
use async_trait::async_trait;
use sha2::Digest;
use std::sync::Arc;
use std::time::Duration;
pub use tonic_0_14_2 as tonic;
use tonic::service::{Interceptor, interceptor::InterceptedService};

use hyper_util::client::legacy::connect::HttpConnector;
use rustls::client::danger::{ServerCertVerifier, HandshakeSignatureValid, ServerCertVerified};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as TlsError, SignatureScheme};
use rustls_pemfile;

pub mod ln_lnd {
    include!(concat!(env!("OUT_DIR"), "/ln/lnrpc.rs"));
}

use ln_lnd::lightning_client::LightningClient;
use ln_lnd::{ChannelBalanceRequest, Invoice};


#[derive(Debug)]
struct LndCertificateVerifier {
    expected_cert: Vec<u8>,
}

impl LndCertificateVerifier {
    fn new(cert_der: Vec<u8>) -> Self {
        Self {
            expected_cert: cert_der,
        }
    }
}

impl ServerCertVerifier for LndCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        // For LND, we just check if the presented cert matches our expected cert
        if end_entity.as_ref() == self.expected_cert.as_slice() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(TlsError::General(
                "Server certificate does not match expected".to_string(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .expect("Must install default crypto provider");
        rustls::crypto::verify_tls12_signature(message, cert, dss, &provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .expect("Must install default crypto provider");
        rustls::crypto::verify_tls13_signature(message, cert, dss, &provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .expect("Must install default crypto provider");
        provider.signature_verification_algorithms.supported_schemes()
    }
}

type Service = InterceptedService<
    hyper_util::client::legacy::Client<
        hyper_rustls::HttpsConnector<HttpConnector>,
        tonic::body::Body,
    >,
    MacaroonInterceptor,
>;

pub struct TonicLndGrpcClient {
    config: LndGrpcDiscoveryBackendImplementation,
    features: Option<LnFeatures>,
    client: Arc<tokio::sync::Mutex<Option<LightningClient<Service>>>>,
}

impl TonicLndGrpcClient {
    pub fn create(
        _timeout: Duration,
        config: LndGrpcDiscoveryBackendImplementation,
    ) -> Result<Self, LnPoolError> {
        Ok(Self {
            config,
            features: Some(LnFeatures {
                invoice_from_desc_hash: true,
            }),
            client: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    async fn get_client(&self) -> Result<LightningClient<Service>, LnPoolError> {
        let mut client_guard = self.client.lock().await;
        if client_guard.is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();

            let LndGrpcClientAuth::Path(auth) = self.config.auth.clone();

            let tls_cert_pem = tokio::fs::read(&auth.tls_cert_path).await.map_err(|e| {
                LnPoolError::from_invalid_credentials(
                    e.to_string(),
                    ServiceErrorSource::Internal,
                    format!(
                        "loading LND TLS certificate from {}",
                        auth.tls_cert_path.to_string_lossy()
                    ),
                )
            })?;

            let macaroon_bytes = tokio::fs::read(&auth.macaroon_path).await.map_err(|e| {
                LnPoolError::from_invalid_credentials(
                    e.to_string(),
                    ServiceErrorSource::Internal,
                    format!(
                        "loading LND macaroon from {}",
                        auth.macaroon_path.to_string_lossy()
                    ),
                )
            })?;

            let mut cert_reader = std::io::Cursor::new(&tls_cert_pem);
            let cert_der = rustls_pemfile::certs(&mut cert_reader)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    LnPoolError::from_invalid_credentials(
                        e.to_string(),
                        ServiceErrorSource::Internal,
                        format!("parsing LND TLS certificate from {}", auth.tls_cert_path.to_string_lossy()),
                    )
                })?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    LnPoolError::from_invalid_credentials(
                        "No certificate found in PEM file".to_string(),
                        ServiceErrorSource::Internal,
                        format!("parsing LND TLS certificate from {}", auth.tls_cert_path.to_string_lossy()),
                    )
                })?;

            let tls_config = ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(LndCertificateVerifier::new(cert_der.to_vec())))
                .with_no_client_auth();

            let connector = hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls_config)
                .https_or_http()
                .enable_http2()
                .build();

            let http_client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(connector);

            let macaroon_hex = hex::encode(&macaroon_bytes);
            let service = InterceptedService::new(
                http_client,
                MacaroonInterceptor {
                    macaroon: macaroon_hex,
                }
            );

            let uri = self.config.url.to_string().parse().map_err(|e| {
                LnPoolError::from_invalid_configuration(
                    format!("Invalid URI: {}", e),
                    ServiceErrorSource::Internal,
                    format!("parsing LND URL {}", self.config.url),
                )
            })?;

            let lightning_client = LightningClient::with_origin(service, uri);
            *client_guard = Some(lightning_client);
        }

        Ok(client_guard.as_ref().unwrap().clone())
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
        let mut client = self.get_client().await?;

        let (memo, description_hash) = match description {
            Bolt11InvoiceDescription::Direct(d) => (d.to_string(), vec![]),
            Bolt11InvoiceDescription::DirectIntoHash(d) => (
                String::new(),
                sha2::Sha256::digest(d.as_bytes()).to_vec(),
            ),
            Bolt11InvoiceDescription::Hash(h) => (String::new(), h.to_vec()),
        };

        let invoice_request = Invoice {
            memo,
            value_msat: amount_msat.unwrap_or(0) as i64,
            description_hash,
            expiry: expiry_secs.unwrap_or(3600) as i64,
            is_amp: self.config.amp_invoice,
            ..Default::default()
        };

        let response = client
            .add_invoice(invoice_request)
            .await
            .map_err(|e| {
                LnPoolError::from_invalid_configuration(
                    format!("gRPC error: {}", e),
                    ServiceErrorSource::Upstream,
                    format!(
                        "LND get invoice from {}, requesting invoice",
                        self.config.url
                    ),
                )
            })?
            .into_inner();

        Ok(response.payment_request)
    }

    async fn get_metrics(&self) -> Result<LnMetrics, Self::Error> {
        let mut client = self.get_client().await?;

        let channel_balance_request = ChannelBalanceRequest {};
        let channels_balance_response = client
            .channel_balance(channel_balance_request)
            .await
            .map_err(|e| {
                LnPoolError::from_invalid_configuration(
                    format!("gRPC error: {}", e),
                    ServiceErrorSource::Upstream,
                    format!(
                        "LND get metrics for {}, requesting channels",
                        self.config.url
                    ),
                )
            })?
            .into_inner();

        let node_effective_inbound_msat = channels_balance_response
            .remote_balance
            .map(|balance| balance.msat as u64)
            .unwrap_or(0);

        Ok(LnMetrics {
            healthy: true,
            node_effective_inbound_msat,
        })
    }

    fn get_features(&self) -> Option<&LnFeatures> {
        self.features.as_ref()
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