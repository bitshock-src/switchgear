use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};
use crate::di::inject::injectors::service::tls::load_server_x509_credentials;
use crate::di::inject::injectors::store::discovery::DiscoveryStoreInjector;
use anyhow::{anyhow, Context};
use jsonwebtoken::DecodingKey;
use log::{info, warn};
use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use switchgear_components::axum::middleware::logger::ClfLogger;
use switchgear_service::{DiscoveryService, DiscoveryState};

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
    ) -> anyhow::Result<Option<Pin<Box<dyn Future<Output = std::io::Result<()>>>>>> {
        if !self.enablement.discovery_enabled() {
            return Ok(None);
        }

        let service_config = self
            .config
            .get()
            .discovery_service
            .as_ref()
            .ok_or_else(|| anyhow!("discover service enabled but has no config"))?;

        let store = match self.store_injector.get().await? {
            None => {
                return Err(anyhow::anyhow!("discover service enabled but has no store"));
            }
            Some(s) => s,
        };

        let listener = TcpListener::bind(service_config.address).with_context(|| {
            format!(
                "binding TCP listener for discovery service to address {}",
                service_config.address
            )
        })?;
        let local_addr = listener
            .local_addr()
            .with_context(|| "verifying discovery service address")?;

        let acceptor = if let Some(tls) = &service_config.tls {
            let acceptor = load_server_x509_credentials(tls).with_context(|| {
                format!(
                    "loading tls certificate for discovery service {}",
                    service_config.address
                )
            })?;
            info!("discovery service with TLS, listening on: {local_addr}");
            Some(acceptor)
        } else {
            warn!("discovery service missing TLS, listening on: {local_addr}",);
            None
        };

        let auth_authority_pem = std::fs::read(service_config.auth_authority.as_path())
            .with_context(|| {
                format!(
                    "reading auth authority from: {}",
                    service_config.auth_authority.to_string_lossy()
                )
            })?;
        let auth_authority = DecodingKey::from_ec_pem(&auth_authority_pem).with_context(|| {
            format!(
                "decoding auth authority from: {}",
                service_config.auth_authority.to_string_lossy()
            )
        })?;

        let router = DiscoveryService::router(DiscoveryState::new(store, auth_authority))
            .layer(ClfLogger::new("discovery"))
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
