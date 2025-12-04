use crate::config::DiscoveryStoreConfig;
use crate::di::delegates::DiscoveryBackendStoreDelegate;
use crate::di::inject::injectors::config::ServerConfigInjector;
use crate::di::inject::injectors::store::tls::load_server_certificate;
use anyhow::{anyhow, Context};
use std::cell::RefCell;
use std::rc::Rc;
use std::str::from_utf8;
use std::time::Duration;
use switchgear_service::components::discovery::db::DbDiscoveryBackendStore;
use switchgear_service::components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_service::components::discovery::memory::MemoryDiscoveryBackendStore;

#[derive(Clone)]
pub struct DiscoveryStoreInjector {
    config: ServerConfigInjector,
    singleton: Rc<RefCell<Option<Option<DiscoveryBackendStoreDelegate>>>>,
}

impl DiscoveryStoreInjector {
    pub fn new(config: ServerConfigInjector) -> Self {
        Self {
            config,
            singleton: Default::default(),
        }
    }

    pub async fn get(&self) -> anyhow::Result<Option<DiscoveryBackendStoreDelegate>> {
        if let Some(b) = self.singleton.borrow().as_ref() {
            return Ok(b.clone());
        }
        self.inject().await
    }

    async fn inject(&self) -> anyhow::Result<Option<DiscoveryBackendStoreDelegate>> {
        let store_config = self
            .config
            .get()
            .store
            .as_ref()
            .ok_or_else(|| anyhow!("discover store enabled but has no config"))?;
        let store_config = store_config
            .discover
            .as_ref()
            .ok_or_else(|| anyhow!("discover store enabled but has no config"))?;

        let store = match store_config {
            DiscoveryStoreConfig::Database {
                database_uri,
                max_connections,
            } => {
                let database_uri = strfmt::strfmt(database_uri, self.config.secrets()).map_err(|_| {
                    anyhow!(
                        "Error while inserting secrets for discovery database connection uri. Invalid uri or missing secrets in: {}",
                        database_uri
                    )
                })?;

                let store =
                    DbDiscoveryBackendStore::connect(&database_uri, *max_connections).await?;
                store.migrate_up().await?;
                DiscoveryBackendStoreDelegate::Database(store)
            }
            DiscoveryStoreConfig::Memory => {
                DiscoveryBackendStoreDelegate::Memory(MemoryDiscoveryBackendStore::new())
            }
            DiscoveryStoreConfig::Http {
                base_url,
                connect_timeout_secs: connect_timeout,
                total_timeout_secs: total_timeout,
                trusted_roots,
                authorization,
            } => {
                let trusted_roots = load_server_certificate(trusted_roots.as_deref())
                    .with_context(|| "loading server certificate for http discovery store")?;
                let authorization_token =
                    std::fs::read(authorization.as_path()).with_context(|| {
                        format!(
                            "reading authorization token for http discovery store from: {}",
                            authorization.to_string_lossy()
                        )
                    })?;
                let authorization_token = from_utf8(&authorization_token).with_context(|| {
                    format!(
                        "parsing authorization token for http discovery store from: {}",
                        authorization.to_string_lossy()
                    )
                })?;
                DiscoveryBackendStoreDelegate::Http(
                    HttpDiscoveryBackendStore::create(
                        base_url,
                        Duration::from_secs_f64(*total_timeout),
                        Duration::from_secs_f64(*connect_timeout),
                        &trusted_roots,
                        authorization_token.to_string(),
                    )
                    .with_context(|| "creating http client for discovery store")?,
                )
            }
        };

        *self.singleton.borrow_mut() = Some(Some(store.clone()));
        Ok(Some(store))
    }
}
