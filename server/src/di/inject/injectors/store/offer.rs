use crate::config::OfferStoreConfig;
use crate::di::delegates::OfferStoreDelegate;
use crate::di::error::DiError;
use crate::di::inject::injectors::config::ServerConfigInjector;
use crate::di::inject::injectors::store::tls::load_server_certificate;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use switchgear_components::offer::db::DbOfferStore;
use switchgear_components::offer::http::HttpOfferStore;
use switchgear_components::offer::memory::MemoryOfferStore;
use switchgear_components::secrets::SecretStore;
use switchgear_error::ChainedContext;

#[derive(Clone)]
pub struct OfferStoreInjector {
    config: ServerConfigInjector,
    singleton: Rc<RefCell<Option<Option<OfferStoreDelegate>>>>,
}

impl OfferStoreInjector {
    pub fn new(config: ServerConfigInjector) -> Self {
        Self {
            config,
            singleton: Default::default(),
        }
    }

    pub async fn get(&self) -> Result<Option<OfferStoreDelegate>, DiError> {
        if let Some(b) = self.singleton.borrow().as_ref() {
            return Ok(b.clone());
        }
        self.inject().await
    }

    async fn inject(&self) -> Result<Option<OfferStoreDelegate>, DiError> {
        let store_config = self
            .config
            .get()
            .store
            .as_ref()
            .ok_or_else(|| DiError::internal("offer store enabled but has no config"))?;
        let store_config = store_config
            .offer
            .as_ref()
            .ok_or_else(|| DiError::internal("offer store enabled but has no config"))?;

        let store = match store_config {
            OfferStoreConfig::Database {
                database_uri,
                max_connections,
                connect_timeout_secs,
                acquire_timeout_secs,
                secrets,
            } => {
                let secret_store = secrets.as_ref().map(SecretStore::create);

                let store = DbOfferStore::connect(
                    database_uri,
                    secret_store.as_ref(),
                    *max_connections,
                    *connect_timeout_secs,
                    *acquire_timeout_secs,
                )
                .await
                .chained_context("connecting offer database", None)?;
                store
                    .migrate_up()
                    .await
                    .chained_context("migrating offer database", None)?;
                OfferStoreDelegate::Database(store)
            }
            OfferStoreConfig::Memory => OfferStoreDelegate::Memory(MemoryOfferStore::new()),
            OfferStoreConfig::Http {
                base_url,
                connect_timeout_secs: connect_timeout,
                total_timeout_secs: total_timeout,
                trusted_roots,
                authorization,
                secrets,
            } => {
                let trusted_roots = load_server_certificate(trusted_roots.as_deref())
                    .chained_context("loading server certificates for http offer store", None)?;
                let secret_store = SecretStore::create(secrets);
                OfferStoreDelegate::Http(
                    HttpOfferStore::create(
                        base_url,
                        Duration::from_secs_f64(*total_timeout),
                        Duration::from_secs_f64(*connect_timeout),
                        &trusted_roots,
                        secret_store,
                        authorization.clone(),
                    )
                    .chained_context("creating http client for offer store", None)?,
                )
            }
        };

        *self.singleton.borrow_mut() = Some(Some(store.clone()));
        Ok(Some(store))
    }
}
