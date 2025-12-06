use crate::di::inject::injectors::balance::BalancerInjector;
use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};
use crate::di::inject::injectors::service::tls::load_server_x509_credentials;
use crate::di::inject::injectors::store::offer::OfferStoreInjector;
use anyhow::{anyhow, Context};
use log::{info, warn};
use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use switchgear_components::axum::middleware::logger::ClfLogger;
use switchgear_service::scheme::Scheme;
use switchgear_service::{LnUrlBalancerService, LnUrlPayState};

pub struct BalancerServiceInjector {
    config: ServerConfigInjector,
    enablement: ServiceEnablementInjector,
    balancer_injector: BalancerInjector,
    offer_store: OfferStoreInjector,
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
        }
    }

    pub async fn connect(
        &self,
    ) -> anyhow::Result<Option<Pin<Box<dyn Future<Output = std::io::Result<()>>>>>> {
        if !self.enablement.lnurl_enabled() {
            return Ok(None);
        }

        let service_config = self
            .config
            .get()
            .lnurl_service
            .as_ref()
            .ok_or_else(|| anyhow!("lnurl service enabled but has no config"))?;

        let balancer = self
            .balancer_injector
            .get()
            .await?
            .ok_or_else(|| anyhow!("lnurl service enabled but has no balancer"))?;

        let offer_store = self
            .offer_store
            .get()
            .await?
            .ok_or_else(|| anyhow!("lnurl service enabled but has no offer store"))?;

        let listener = TcpListener::bind(service_config.address).with_context(|| {
            format!(
                "binding TCP listener for lnurl service to address {}",
                service_config.address
            )
        })?;
        let local_addr = listener
            .local_addr()
            .with_context(|| "verifying lnurl service address")?;

        let acceptor = if let Some(tls) = &service_config.tls {
            let acceptor = load_server_x509_credentials(tls).with_context(|| {
                format!(
                    "loading tls certificate for lnurl service {}",
                    service_config.address
                )
            })?;
            info!("lnurl service with TLS, listening on: {local_addr}");
            Some(acceptor)
        } else {
            warn!("lnurl service missing TLS, listening on: {local_addr}");
            None
        };

        let scheme = if acceptor.is_some() { "https" } else { "http" };
        let scheme = Scheme(scheme.to_string());

        let router = LnUrlBalancerService::router(LnUrlPayState::new(
            service_config.partitions.clone(),
            offer_store,
            balancer,
            service_config.invoice_expiry_secs,
            scheme,
            service_config.allowed_hosts.clone(),
            service_config.comment_allowed,
            service_config.bech32_qr_scale,
            service_config.bech32_qr_light,
            service_config.bech32_qr_dark,
        ))
        .layer(ClfLogger::new("lnurl"))
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
