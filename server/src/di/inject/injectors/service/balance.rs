use crate::di::error::DiError;
use crate::di::inject::injectors::balance::BalancerInjector;
use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};
use crate::di::inject::injectors::service::subscriber::ServiceTracing;
use crate::di::inject::injectors::service::tls::load_server_x509_credentials;
use crate::di::inject::injectors::store::offer::OfferStoreInjector;
use std::cell::RefCell;
use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use switchgear_components::axum::middleware::logger::RequestLogger;
use switchgear_components::axum::middleware::{
    OtelAxumLayer, StripTraceContextLayer, WithSubscriberLayer,
};
use switchgear_components::offer::provider::StoreOfferProvider;
use switchgear_error::{ChainedContext, ForeignContext};
use switchgear_service::host::AllowedHosts;
use switchgear_service::scheme::Scheme;
use switchgear_service::{LnUrlBalancerService, LnUrlPayState};
use tracing::{info, warn};

const SERVICE_NAME: &str = "swgr.lnurl";

pub struct BalancerServiceInjector {
    config: ServerConfigInjector,
    enablement: ServiceEnablementInjector,
    balancer_injector: BalancerInjector,
    offer_store: OfferStoreInjector,
    tracing: RefCell<Option<ServiceTracing>>,
}

impl BalancerServiceInjector {
    pub fn new(
        config: ServerConfigInjector,
        enablement: ServiceEnablementInjector,
        balancer_injector: BalancerInjector,
        offer_store: OfferStoreInjector,
    ) -> Self {
        Self {
            config,
            enablement,
            balancer_injector,
            offer_store,
            tracing: RefCell::default(),
        }
    }

    pub async fn connect(
        &self,
    ) -> Result<Option<Pin<Box<dyn Future<Output = std::io::Result<()>>>>>, DiError> {
        if !self.enablement.lnurl_enabled() {
            return Ok(None);
        }

        let service_config = self
            .config
            .get()
            .lnurl_service
            .as_ref()
            .ok_or_else(|| DiError::internal("lnurl service enabled but has no config"))?;

        let tracing = ServiceTracing::build(SERVICE_NAME, service_config.otlp.as_ref())
            .chained_context("building lnurl service tracing subscriber", None)?;
        let dispatch = tracing.dispatch().clone();
        *self.tracing.borrow_mut() = Some(tracing);

        let balancer = self
            .balancer_injector
            .get()
            .await?
            .ok_or_else(|| DiError::internal("lnurl service enabled but has no balancer"))?;

        let offer_store = self
            .offer_store
            .get()
            .await?
            .ok_or_else(|| DiError::internal("lnurl service enabled but has no offer store"))?;

        let offer_store = StoreOfferProvider::new(offer_store);

        let listener = TcpListener::bind(service_config.address).with_foreign_context(
            || {
                format!(
                    "binding TCP listener for lnurl service to address {}",
                    service_config.address
                )
            },
            None,
        )?;
        let local_addr = listener
            .local_addr()
            .foreign_context("verifying lnurl service address", None)?;

        let acceptor = if let Some(tls) = &service_config.tls {
            let acceptor = load_server_x509_credentials(tls).with_chained_context(
                || {
                    format!(
                        "loading tls certificate for lnurl service {}",
                        service_config.address
                    )
                },
                None,
            )?;
            info!("lnurl service with TLS, listening on: {local_addr}");
            Some(acceptor)
        } else {
            warn!("lnurl service missing TLS, listening on: {local_addr}");
            None
        };

        let scheme = if acceptor.is_some() { "https" } else { "http" };
        let scheme = Scheme(scheme.to_string());

        let allowed_hosts = AllowedHosts(service_config.allowed_hosts.clone());

        let router = LnUrlBalancerService::router(LnUrlPayState::new(
            service_config.partitions.clone(),
            offer_store,
            balancer,
            service_config.invoice_expiry_secs,
            scheme,
            allowed_hosts,
            service_config.comment_allowed,
            service_config.bech32_qr_scale,
            service_config.bech32_qr_light,
            service_config.bech32_qr_dark,
        ))
        .layer(RequestLogger::new())
        .layer(OtelAxumLayer::default())
        .layer(StripTraceContextLayer)
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
                .chained_context("shutting down lnurl service tracing", None),
            None => Ok(()),
        }
    }
}
