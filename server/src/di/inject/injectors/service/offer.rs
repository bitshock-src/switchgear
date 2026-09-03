use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};

use crate::di::error::DiError;
use crate::di::inject::injectors::service::subscriber::ServiceTracing;
use crate::di::inject::injectors::service::tls::load_server_x509_credentials;
use crate::di::inject::injectors::store::offer::OfferStoreInjector;
use jsonwebtoken::DecodingKey;
use std::cell::RefCell;
use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use switchgear_components::axum::middleware::logger::RequestLogger;
use switchgear_components::axum::middleware::{
    OtelAxumLayer, OtelInResponseLayer, WithSubscriberLayer,
};
use switchgear_error::{ChainedContext, ForeignContext};
use switchgear_service::{OfferService, OfferState};
use tracing::{info, warn};

const SERVICE_NAME: &str = "swgr.offer";

pub struct OfferServiceInjector {
    config: ServerConfigInjector,
    enablement: ServiceEnablementInjector,
    store_injector: OfferStoreInjector,
    tracing: RefCell<Option<ServiceTracing>>,
}

impl OfferServiceInjector {
    pub fn new(
        config: ServerConfigInjector,
        enablement: ServiceEnablementInjector,
        store_injector: OfferStoreInjector,
    ) -> Self {
        Self {
            config,
            enablement,
            store_injector,
            tracing: RefCell::default(),
        }
    }

    pub async fn connect(
        &self,
    ) -> Result<Option<Pin<Box<dyn Future<Output = std::io::Result<()>>>>>, DiError> {
        if !self.enablement.offer_enabled() {
            return Ok(None);
        }

        let service_config = self
            .config
            .get()
            .offer_service
            .as_ref()
            .ok_or_else(|| DiError::internal("offer service enabled but has no config"))?;

        let tracing = ServiceTracing::build(SERVICE_NAME, service_config.otlp.as_ref())
            .chained_context("building offer service tracing subscriber", None)?;
        let dispatch = tracing.dispatch().clone();
        *self.tracing.borrow_mut() = Some(tracing);

        let store = self
            .store_injector
            .get()
            .await?
            .ok_or_else(|| DiError::internal("offer service enabled but has no store"))?;

        let listener = TcpListener::bind(service_config.address).with_foreign_context(
            || {
                format!(
                    "binding TCP listener for offer service to address {}",
                    service_config.address
                )
            },
            None,
        )?;
        let local_addr = listener
            .local_addr()
            .foreign_context("verifying offer service address", None)?;

        let acceptor = if let Some(tls) = &service_config.tls {
            let acceptor = load_server_x509_credentials(tls).with_chained_context(
                || {
                    format!(
                        "loading tls certificate for offer service {}",
                        service_config.address
                    )
                },
                None,
            )?;
            info!("offer service with TLS, listening on: {local_addr}");
            Some(acceptor)
        } else {
            warn!("offer service missing TLS, listening on: {local_addr}");
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

        let router = OfferService::router(OfferState::new(
            store.clone(),
            store,
            auth_authority,
            service_config.max_page_size,
        ))
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

    pub async fn shutdown_tracing(&self) -> Result<(), DiError> {
        let tracing = self.tracing.borrow_mut().take();
        match tracing {
            Some(t) => t
                .shutdown()
                .await
                .chained_context("shutting down offer service tracing", None),
            None => Ok(()),
        }
    }
}
