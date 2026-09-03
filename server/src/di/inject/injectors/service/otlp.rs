use crate::config::{OtlpExportConfig, SamplerConfig, TemporalityConfig};
use crate::di::error::DiError;
use hyper_rustls::{FixedServerNameResolver, HttpsConnectorBuilder};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithTonicConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;
use std::time::Duration;
use switchgear_components::secrets::{ClientCertResolver, SecretHeaderInterceptor, SecretStore};
use switchgear_error::{ErrorOrigin, ForeignContext};
use tonic::transport::{Channel, Endpoint};

impl From<&SamplerConfig> for Sampler {
    fn from(config: &SamplerConfig) -> Self {
        match config {
            SamplerConfig::AlwaysOn => Sampler::AlwaysOn,
            SamplerConfig::AlwaysOff => Sampler::AlwaysOff,
            SamplerConfig::TraceIdRatio { ratio } => Sampler::TraceIdRatioBased(*ratio),
            SamplerConfig::ParentBasedTraceIdRatio { ratio } => {
                Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(*ratio)))
            }
        }
    }
}

impl From<&TemporalityConfig> for Temporality {
    fn from(config: &TemporalityConfig) -> Self {
        match config {
            TemporalityConfig::Cumulative => Temporality::Cumulative,
            TemporalityConfig::Delta => Temporality::Delta,
            TemporalityConfig::LowMemory => Temporality::LowMemory,
        }
    }
}

pub fn resource(service_name: &'static str) -> Resource {
    Resource::builder()
        .with_service_name(service_name)
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build()
}

pub fn client_tls_config(
    export: &OtlpExportConfig,
    secrets: &SecretStore,
) -> Result<ClientConfig, DiError> {
    let crypto = CryptoProvider::get_default()
        .ok_or_else(|| DiError::internal("no rustls crypto provider installed for otel exporter"))?
        .clone();

    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    if let Some(path) = export.trusted_roots.as_ref() {
        let pem = std::fs::read(path).with_foreign_context(
            || format!("reading otel trusted roots from {}", path.display()),
            None,
        )?;
        let extra: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&pem)
            .collect::<Result<_, _>>()
            .with_foreign_context(
                || format!("parsing otel trusted roots from {}", path.display()),
                None,
            )?;
        roots.add_parsable_certificates(extra);
    }

    let builder = ClientConfig::builder_with_provider(crypto.clone())
        .with_safe_default_protocol_versions()
        .foreign_context(
            "configuring otel exporter TLS protocol versions",
            ErrorOrigin::Internal,
        )?
        .with_root_certificates(roots);

    Ok(match export.client_identity.as_ref() {
        Some(id) => builder.with_client_cert_resolver(Arc::new(ClientCertResolver::new(
            secrets.clone(),
            id.cert_secret.clone(),
            id.key_secret.clone(),
            crypto,
        ))),
        None => builder.with_no_client_auth(),
    })
}

pub fn channel(export: &OtlpExportConfig, secrets: &SecretStore) -> Result<Channel, DiError> {
    let tls_config = client_tls_config(export, secrets)?;

    let mut connector_builder = HttpsConnectorBuilder::new()
        .with_tls_config(tls_config)
        .https_or_http();
    if let Some(addr) = export.trusted_root_address.as_ref() {
        let name: ServerName<'static> =
            ServerName::try_from(addr.ip().to_string()).map_err(|e| {
                DiError::internal(format!(
                    "parsing otel trusted-root-address {} as TLS server name: {e}",
                    addr.ip()
                ))
            })?;
        connector_builder =
            connector_builder.with_server_name_resolver(FixedServerNameResolver::new(name));
    }
    let https = connector_builder.enable_http2().build();

    let mut endpoint = Endpoint::from_shared(export.endpoint.to_string()).map_err(|e| {
        DiError::internal(format!(
            "parsing otel endpoint {} for tonic channel: {e}",
            export.endpoint
        ))
    })?;
    if let Some(secs) = export.export_timeout_secs {
        endpoint = endpoint.timeout(Duration::from_secs_f64(secs));
    }

    Ok(Channel::new(https, endpoint))
}

pub fn transport(export: &OtlpExportConfig) -> Result<(Channel, SecretStore), DiError> {
    let secrets = SecretStore::create(&export.secrets);
    let channel = channel(export, &secrets)?;
    Ok((channel, secrets))
}

pub fn span_exporter(
    export: &OtlpExportConfig,
    channel: Channel,
    secrets: SecretStore,
) -> Result<SpanExporter, DiError> {
    let interceptor = SecretHeaderInterceptor::bearer(secrets, export.auth_token.clone());

    SpanExporter::builder()
        .with_tonic()
        .with_channel(channel)
        .with_interceptor(interceptor)
        .build()
        .with_foreign_context(
            || {
                format!(
                    "building otel span exporter for endpoint {}",
                    export.endpoint
                )
            },
            ErrorOrigin::Internal,
        )
}

pub fn tracer_provider(
    service_name: &'static str,
    export: &OtlpExportConfig,
    channel: Channel,
    secrets: SecretStore,
    sampler: &SamplerConfig,
) -> Result<SdkTracerProvider, DiError> {
    let exporter = span_exporter(export, channel, secrets)?;

    Ok(SdkTracerProvider::builder()
        .with_resource(resource(service_name))
        .with_sampler(Sampler::from(sampler))
        .with_batch_exporter(exporter)
        .build())
}

pub fn metric_exporter(
    export: &OtlpExportConfig,
    channel: Channel,
    secrets: SecretStore,
    temporality: &TemporalityConfig,
) -> Result<MetricExporter, DiError> {
    let interceptor = SecretHeaderInterceptor::bearer(secrets, export.auth_token.clone());

    MetricExporter::builder()
        .with_tonic()
        .with_channel(channel)
        .with_interceptor(interceptor)
        .with_temporality(Temporality::from(temporality))
        .build()
        .with_foreign_context(
            || {
                format!(
                    "building otel metric exporter for endpoint {}",
                    export.endpoint
                )
            },
            ErrorOrigin::Internal,
        )
}

pub fn meter_provider(
    service_name: &'static str,
    export: &OtlpExportConfig,
    channel: Channel,
    secrets: SecretStore,
    temporality: &TemporalityConfig,
) -> Result<SdkMeterProvider, DiError> {
    let exporter = metric_exporter(export, channel, secrets, temporality)?;
    let reader = PeriodicReader::builder(exporter).build();

    Ok(SdkMeterProvider::builder()
        .with_resource(resource(service_name))
        .with_reader(reader)
        .build())
}
