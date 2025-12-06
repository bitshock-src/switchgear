use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};

use crate::di::inject::injectors::service::tls::load_server_x509_credentials;
use crate::di::inject::injectors::store::offer::OfferStoreInjector;
use anyhow::{anyhow, Context};
use jsonwebtoken::DecodingKey;
use log::{info, warn};
use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use switchgear_components::axum::middleware::logger::ClfLogger;
use switchgear_service::{OfferService, OfferState};

pub struct OfferServiceInjector {
    config: ServerConfigInjector,
    enablement: ServiceEnablementInjector,
    store_injector: OfferStoreInjector,
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
        }
    }

    pub async fn connect(
        &self,
    ) -> anyhow::Result<Option<Pin<Box<dyn Future<Output = std::io::Result<()>>>>>> {
        if !self.enablement.offer_enabled() {
            return Ok(None);
        }

        let service_config = self
            .config
            .get()
            .offer_service
            .as_ref()
            .ok_or_else(|| anyhow!("offer service enabled but has no config"))?;

        let store = match self.store_injector.get().await? {
            None => {
                return Err(anyhow::anyhow!("offer service enabled but has no store"));
            }
            Some(s) => s,
        };

        let listener = TcpListener::bind(service_config.address).with_context(|| {
            format!(
                "binding TCP listener for offer service to address {}",
                service_config.address
            )
        })?;
        let local_addr = listener
            .local_addr()
            .with_context(|| "verifying offer service address")?;

        let acceptor = if let Some(tls) = &service_config.tls {
            let acceptor = load_server_x509_credentials(tls).with_context(|| {
                format!(
                    "loading tls certificate for offer service {}",
                    service_config.address
                )
            })?;
            info!("offer service with TLS, listening on: {local_addr}");
            Some(acceptor)
        } else {
            warn!("offer service missing TLS, listening on: {local_addr}");
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

        let router = OfferService::router(OfferState::new(
            store.clone(),
            store,
            auth_authority,
            service_config.max_page_size,
        ))
        .layer(ClfLogger::new("offer"))
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
