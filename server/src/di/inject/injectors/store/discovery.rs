use crate::config::DiscoveryStoreConfig;
use crate::di::delegates::DiscoveryBackendStoreDelegate;
use crate::di::error::DiError;
use crate::di::inject::injectors::config::ServerConfigInjector;
use crate::di::inject::injectors::store::tls::load_server_certificate;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use switchgear_components::discovery::db::DbDiscoveryBackendStore;
use switchgear_components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_components::discovery::memory::MemoryDiscoveryBackendStore;
use switchgear_components::secrets::SecretStore;
use switchgear_error::ChainedContext;

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

    pub async fn get(&self) -> Result<Option<DiscoveryBackendStoreDelegate>, DiError> {
        if let Some(b) = self.singleton.borrow().as_ref() {
            return Ok(b.clone());
        }
        self.inject().await
    }

    async fn inject(&self) -> Result<Option<DiscoveryBackendStoreDelegate>, DiError> {
        let store_config = self
            .config
            .get()
            .store
            .as_ref()
            .ok_or_else(|| DiError::internal("discover store enabled but has no config"))?;
        let store_config = store_config
            .discover
            .as_ref()
            .ok_or_else(|| DiError::internal("discover store enabled but has no config"))?;

        let store = match store_config {
            DiscoveryStoreConfig::Database {
                database_uri,
                max_connections,
                connect_timeout_secs,
                acquire_timeout_secs,
                secrets,
            } => {
                let secret_store = secrets.as_ref().map(SecretStore::create);

                let store = DbDiscoveryBackendStore::connect(
                    database_uri,
                    secret_store.as_ref(),
                    *max_connections,
                    *connect_timeout_secs,
                    *acquire_timeout_secs,
                )
                .await
                .chained_context("connecting discovery database", None)?;
                store
                    .migrate_up()
                    .await
                    .chained_context("migrating discovery database", None)?;
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
                secrets,
            } => {
                let trusted_roots = load_server_certificate(trusted_roots.as_deref())
                    .chained_context("loading server certificate for http discovery store", None)?;
                let secret_store = SecretStore::create(secrets);
                DiscoveryBackendStoreDelegate::Http(
                    HttpDiscoveryBackendStore::create(
                        base_url,
                        Duration::from_secs_f64(*total_timeout),
                        Duration::from_secs_f64(*connect_timeout),
                        &trusted_roots,
                        secret_store,
                        authorization.clone(),
                    )
                    .chained_context("creating http client for discovery store", None)?,
                )
            }
        };

        *self.singleton.borrow_mut() = Some(Some(store.clone()));
        Ok(Some(store))
    }
}
