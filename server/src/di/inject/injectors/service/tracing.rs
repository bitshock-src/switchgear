use crate::config::{OtlpConfig, OtlpTracingConfig};
use crate::di::error::DiError;
use crate::{CliArgs, RootCommands};
use hyper_rustls::{FixedServerNameResolver, HttpsConnectorBuilder};
use log::LevelFilter as LogLevelFilter;
use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{SpanExporter, WithTonicConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;
use switchgear_components::secrets::{ClientCertResolver, SecretHeaderInterceptor, SecretStore};
use switchgear_error::{ErrorOrigin, ForeignContext};
use tonic::transport::{Channel, Endpoint};
use tracing_ecs_formatter::EcsFormatter;
use tracing_subscriber::filter::{Directive, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const GLOBAL_SERVICE_NAME: &str = "swgr";

pub struct TracingSystemInjector;

impl TracingSystemInjector {
    pub fn init(args: &CliArgs) -> Result<(), DiError> {
        match &args.command {
            RootCommands::Service { .. } => init_subscriber(),
            _ => {
                let level = args.log_level.unwrap_or(LogLevelFilter::Info);
                init_cli_subscriber(level)
            }
        }
    }
}

pub struct ServiceSubscriber {
    pub dispatch: tracing::Dispatch,
    pub provider: Option<SdkTracerProvider>,
}

pub fn build_service_subscriber(
    service_name: &'static str,
    otel: Option<&OtlpConfig>,
) -> Result<ServiceSubscriber, DiError> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .event_format(EcsFormatter::new(service_name, env!("CARGO_PKG_VERSION")));

    let (provider, otel_layer) = match otel {
        None => (None, None),
        Some(otel) => match &otel.tracing {
            None => (None, None),
            Some(tracing) => {
                let provider = build_tracing_provider(service_name, tracing)?;
                let layer = tracing_opentelemetry::layer()
                    .with_tracer(provider.tracer(service_name))
                    .with_location(false)
                    .with_tracked_inactivity(false)
                    .with_threads(false);
                (Some(provider), Some(layer))
            }
        },
    };

    let subscriber = tracing_subscriber::registry()
        .with(otel_layer.with_filter(filter_fn(|m| !m.is_event())))
        .with(env_filter)
        .with(fmt_layer);

    Ok(ServiceSubscriber {
        dispatch: tracing::Dispatch::new(subscriber),
        provider,
    })
}

fn init_subscriber() -> Result<(), DiError> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .event_format(EcsFormatter::new(
            GLOBAL_SERVICE_NAME,
            env!("CARGO_PKG_VERSION"),
        ));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init()
        .foreign_context("initializing tracing subscriber", None)
}

fn init_cli_subscriber(level: LogLevelFilter) -> Result<(), DiError> {
    let directive: Directive = match level {
        LogLevelFilter::Off => "off",
        LogLevelFilter::Error => "error",
        LogLevelFilter::Warn => "warn",
        LogLevelFilter::Info => "info",
        LogLevelFilter::Debug => "debug",
        LogLevelFilter::Trace => "trace",
    }
    .parse()
    .expect("static log level parses as directive");

    serif::Config::new()
        .with_output(serif::Output::Stderr)
        .with_default(directive)
        .with_timestamp(serif::TimeFormat::none())
        .with_target(false)
        .init();

    Ok(())
}

fn build_tracing_provider(
    service_name: &'static str,
    tracing: &OtlpTracingConfig,
) -> Result<SdkTracerProvider, DiError> {
    let secrets = SecretStore::create(&tracing.secrets);
    let exporter = build_tracing_exporter(tracing, secrets)?;

    let resource = Resource::builder()
        .with_service_name(service_name)
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build();

    Ok(SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build())
}

fn build_tracing_exporter(
    tracing: &OtlpTracingConfig,
    secrets: SecretStore,
) -> Result<SpanExporter, DiError> {
    let tls_config = build_client_tls_config(tracing, &secrets)?;

    let mut connector_builder = HttpsConnectorBuilder::new()
        .with_tls_config(tls_config)
        .https_or_http();
    if let Some(addr) = tracing.trusted_root_address.as_ref() {
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

    let endpoint = Endpoint::from_shared(tracing.endpoint.to_string()).map_err(|e| {
        DiError::internal(format!(
            "parsing otel endpoint {} for tonic channel: {e}",
            tracing.endpoint
        ))
    })?;
    let channel = Channel::new(https, endpoint);

    let interceptor = SecretHeaderInterceptor::bearer(secrets, tracing.auth_token.clone());

    SpanExporter::builder()
        .with_tonic()
        .with_channel(channel)
        .with_interceptor(interceptor)
        .build()
        .with_foreign_context(
            || {
                format!(
                    "building otel span exporter for endpoint {}",
                    tracing.endpoint
                )
            },
            ErrorOrigin::Internal,
        )
}

fn build_client_tls_config(
    tracing: &OtlpTracingConfig,
    secrets: &SecretStore,
) -> Result<ClientConfig, DiError> {
    let crypto = CryptoProvider::get_default()
        .ok_or_else(|| DiError::internal("no rustls crypto provider installed for otel exporter"))?
        .clone();

    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    if let Some(path) = tracing.trusted_roots.as_ref() {
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
        .expect("rustls safe default protocol versions")
        .with_root_certificates(roots);

    Ok(match tracing.client_identity.as_ref() {
        Some(id) => builder.with_client_cert_resolver(Arc::new(ClientCertResolver::new(
            secrets.clone(),
            id.cert_secret.clone(),
            id.key_secret.clone(),
            crypto,
        ))),
        None => builder.with_no_client_auth(),
    })
}
