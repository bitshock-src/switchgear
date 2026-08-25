use crate::di::error::DiError;
use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};
use crate::di::inject::injectors::service::tls::load_server_x509_credentials;
use crate::di::inject::injectors::service::tracing::{ServiceSubscriber, build_service_subscriber};
use crate::di::inject::injectors::store::discovery::DiscoveryStoreInjector;
use jsonwebtoken::DecodingKey;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use switchgear_components::axum::middleware::logger::RequestLogger;
use switchgear_components::axum::middleware::{
    OtelAxumLayer, OtelInResponseLayer, WithSubscriberLayer,
};
use switchgear_error::{ChainedContext, ForeignContext};
use switchgear_service::{DiscoveryService, DiscoveryState};
use tracing::{info, warn};

const SERVICE_NAME: &str = "swgr.discovery";

pub struct DiscoveryServiceInjector {
    config: ServerConfigInjector,
    enablement: ServiceEnablementInjector,
    store_injector: DiscoveryStoreInjector,
}

impl DiscoveryServiceInjector {
    pub fn new(
        config: ServerConfigInjector,
        enablement: ServiceEnablementInjector,
        store_injector: DiscoveryStoreInjector,
    ) -> Self {
        Self {
            config,
            enablement,
            store_injector,
        }
    }

    pub async fn connect(
        &self,
        otel_providers: &mut Vec<(&'static str, SdkTracerProvider)>,
    ) -> Result<Option<Pin<Box<dyn Future<Output = std::io::Result<()>>>>>, DiError> {
        if !self.enablement.discovery_enabled() {
            return Ok(None);
        }

        let service_config = self
            .config
            .get()
            .discovery_service
            .as_ref()
            .ok_or_else(|| DiError::internal("discover service enabled but has no config"))?;

        let ServiceSubscriber { dispatch, provider } =
            build_service_subscriber(SERVICE_NAME, service_config.otlp.as_ref())
                .chained_context("building discovery service tracing subscriber", None)?;
        if let Some(p) = provider {
            otel_providers.push((SERVICE_NAME, p));
        }

        let store = self
            .store_injector
            .get()
            .await?
            .ok_or_else(|| DiError::internal("discover service enabled but has no store"))?;

        let listener = TcpListener::bind(service_config.address).with_foreign_context(
            || {
                format!(
                    "binding TCP listener for discovery service to address {}",
                    service_config.address
                )
            },
            None,
        )?;
        let local_addr = listener
            .local_addr()
            .foreign_context("verifying discovery service address", None)?;

        let acceptor = if let Some(tls) = &service_config.tls {
            let acceptor = load_server_x509_credentials(tls).with_chained_context(
                || {
                    format!(
                        "loading tls certificate for discovery service {}",
                        service_config.address
                    )
                },
                None,
            )?;
            info!("discovery service with TLS, listening on: {local_addr}");
            Some(acceptor)
        } else {
            warn!("discovery service missing TLS, listening on: {local_addr}",);
            None
        };

        let auth_authority_pem = std::fs::read(service_config.auth_authority.as_path())
            .with_foreign_context(
                || {
                    format!(
                        "reading auth authority from: {}",
                        service_config.auth_authority.to_string_lossy()
                    )
                },
                None,
            )?;
        let auth_authority = DecodingKey::from_ec_pem(&auth_authority_pem).with_foreign_context(
            || {
                format!(
                    "decoding auth authority from: {}",
                    service_config.auth_authority.to_string_lossy()
                )
            },
            None,
        )?;

        let router = DiscoveryService::router(DiscoveryState::new(store, auth_authority))
            .layer(RequestLogger::new())
            .layer(OtelInResponseLayer)
            .layer(OtelAxumLayer::default())
            .layer(WithSubscriberLayer::new(dispatch))
            .into_make_service_with_connect_info::<SocketAddr>();

        let f = async move {
            match acceptor {
                Some(acceptor) => {
                    axum_server::from_tcp_rustls(listener, acceptor)
                        .serve(router)
                        .await
                }
                None => axum_server::from_tcp(listener).serve(router).await,
            }
        };

        Ok(Some(Box::pin(f)))
    }
}
