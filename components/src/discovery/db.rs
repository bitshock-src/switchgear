use crate::discovery::error::DefaultDiscoveryBackendStoreError;
use crate::metrics::db::{DbTarget, record_db_operation};
use crate::secrets::SecretStore;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, DatabaseConnectionType,
    EntityTrait, ExprTrait, FromJsonQueryResult, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};
use secp256k1::PublicKey;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_migration::{DISCOVERY_BACKEND_GET_ALL_ETAG_ID, MigratorTrait};
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendSparse, DiscoveryBackendStore,
    DiscoveryBackends,
};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
pub struct DiscoveryBackendPartitions(BTreeSet<String>);

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "discovery_backend")]
pub struct Model {
    #[sea_orm(column_type = "JsonBinary")]
    pub partitions: DiscoveryBackendPartitions,
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Vec<u8>,
    pub name: Option<String>,
    pub weight: i32,
    pub enabled: bool,
    pub implementation: Vec<u8>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

pub mod etag {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
    #[sea_orm(table_name = "discovery_backend_etag")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: i32,
        pub value: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Clone)]
struct CredentialRotation {
    inner: Arc<CredentialRotationInner>,
}

struct CredentialRotationInner {
    db: DatabaseConnection,
    uri_template: String,
    secrets: SecretStore,
    last_applied: Mutex<SecretString>,
    shutdown_tx: Mutex<Option<watch::Sender<bool>>>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for CredentialRotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialRotation").finish_non_exhaustive()
    }
}

impl CredentialRotation {
    fn spawn(
        db: DatabaseConnection,
        uri_template: String,
        secrets: SecretStore,
        initial: SecretString,
    ) -> Result<Self, DefaultDiscoveryBackendStoreError> {
        let (tx, rx) = watch::channel(false);
        let inner = Arc::new(CredentialRotationInner {
            db,
            uri_template,
            secrets,
            last_applied: Mutex::new(initial),
            shutdown_tx: Mutex::new(Some(tx)),
            join: Mutex::new(None),
        });
        let inner_task = inner.clone();
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(inner_task.secrets.ttl() / 3);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut rx = rx;
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = Self::refresh(&inner_task) {
                            tracing::warn!(error = %e, "discovery DSN rotation refresh failed");
                        }
                    }
                    _ = rx.changed() => {
                        if *rx.borrow() { return; }
                    }
                }
            }
        });
        {
            let Ok(mut join) = inner.join.lock() else {
                return Err(DefaultDiscoveryBackendStoreError::message(
                    ErrorOrigin::Internal,
                    "storing discovery DSN rotation task handle",
                ));
            };
            *join = Some(handle);
        }
        Ok(Self { inner })
    }

    fn refresh(inner: &CredentialRotationInner) -> Result<(), DefaultDiscoveryBackendStoreError> {
        let new = inner
            .secrets
            .replace(&inner.uri_template)
            .chained_context("re-resolving discovery database URI secrets", None)?;

        let Ok(mut last) = inner.last_applied.lock() else {
            return Err(DefaultDiscoveryBackendStoreError::message(
                ErrorOrigin::Internal,
                "reading last applied discovery DSN for rotation",
            ));
        };
        if last.expose_secret() == new.expose_secret() {
            return Ok(());
        }

        match &inner.db.inner {
            DatabaseConnectionType::SqlxPostgresPoolConnection(_) => {
                let opts = sqlx::postgres::PgConnectOptions::from_str(new.expose_secret())
                    .foreign_context("parsing rotated postgres DSN", ErrorOrigin::Internal)?;
                inner
                    .db
                    .get_postgres_connection_pool()
                    .set_connect_options(opts);
            }
            DatabaseConnectionType::SqlxMySqlPoolConnection(_) => {
                let opts = sqlx::mysql::MySqlConnectOptions::from_str(new.expose_secret())
                    .foreign_context("parsing rotated mysql DSN", ErrorOrigin::Internal)?;
                inner
                    .db
                    .get_mysql_connection_pool()
                    .set_connect_options(opts);
            }
            _ => {}
        }
        *last = new;
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), DefaultDiscoveryBackendStoreError> {
        let tx = {
            let Ok(mut shutdown_tx) = self.inner.shutdown_tx.lock() else {
                return Err(DefaultDiscoveryBackendStoreError::message(
                    ErrorOrigin::Internal,
                    "signalling discovery DSN rotation task to stop",
                ));
            };
            shutdown_tx.take()
        };
        if let Some(tx) = tx {
            let _ = tx.send(true);
        }

        let handle = {
            let Ok(mut join) = self.inner.join.lock() else {
                return Err(DefaultDiscoveryBackendStoreError::message(
                    ErrorOrigin::Internal,
                    "taking discovery DSN rotation task handle for join",
                ));
            };
            join.take()
        };

        if let Some(h) = handle
            && let Err(e) = h.await
        {
            tracing::warn!(error = %e, "discovery DSN rotation task join failed");
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DbDiscoveryBackendStore {
    db: DatabaseConnection,
    db_target: DbTarget,
    rotation: Option<CredentialRotation>,
}

impl DbDiscoveryBackendStore {
    pub async fn connect(
        uri: &str,
        secrets: Option<&SecretStore>,
        max_connections: u32,
        connect_timeout_secs: f64,
        acquire_timeout_secs: f64,
    ) -> Result<Self, DefaultDiscoveryBackendStoreError> {
        let resolved = match secrets {
            Some(s) => s
                .replace(uri)
                .chained_context("resolving discovery database URI secrets", None)?,
            None => SecretString::from(uri.to_owned()),
        };
        let mut opt = sea_orm::ConnectOptions::new(resolved.expose_secret());
        opt.max_connections(max_connections)
            .connect_timeout(Duration::from_secs_f64(connect_timeout_secs))
            .acquire_timeout(Duration::from_secs_f64(acquire_timeout_secs));
        let db = Database::connect(opt).await.foreign_context(
            "connecting to discovery backend database",
            ErrorOrigin::Internal,
        )?;

        let db_target = crate::metrics::db::db_target(&db);

        let rotation = secrets
            .map(|s| CredentialRotation::spawn(db.clone(), uri.to_owned(), s.clone(), resolved))
            .transpose()?;

        Ok(Self {
            db,
            db_target,
            rotation,
        })
    }

    pub async fn migrate_up(&self) -> Result<(), DefaultDiscoveryBackendStoreError> {
        switchgear_migration::DiscoveryBackendMigrator::up(&self.db, None)
            .await
            .foreign_context("migrating database up", ErrorOrigin::Internal)?;
        Ok(())
    }

    pub async fn migrate_down(&self) -> Result<(), DefaultDiscoveryBackendStoreError> {
        switchgear_migration::DiscoveryBackendMigrator::down(&self.db, None)
            .await
            .foreign_context("migrating database down", ErrorOrigin::Internal)?;
        Ok(())
    }

    fn model_to_domain(
        model: Model,
    ) -> Result<DiscoveryBackend, DefaultDiscoveryBackendStoreError> {
        Ok(DiscoveryBackend {
            public_key: PublicKey::from_slice(&model.id).with_foreign_context(
                || format!("deserializing public key {:?} from database", model.id),
                ErrorOrigin::Internal,
            )?,
            backend: DiscoveryBackendSparse {
                name: model.name,
                partitions: model.partitions.0,
                weight: model.weight as usize,
                enabled: model.enabled,
                implementation: model.implementation,
            },
        })
    }
}

#[async_trait]
impl DiscoveryBackendStore for DbDiscoveryBackendStore {
    type Error = DefaultDiscoveryBackendStoreError;

    #[tracing::instrument(skip_all)]
    async fn get(&self, public_key: &PublicKey) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let started = Instant::now();
        let result = Entity::find_by_id(public_key.serialize())
            .one(&self.db)
            .await;
        record_db_operation(
            started.elapsed(),
            &self.db_target,
            "discovery_backend",
            "get",
            &result,
        );
        let result = result.with_foreign_context(
            || format!("fetching backend for public key {public_key}"),
            ErrorOrigin::Internal,
        )?;

        match result {
            Some(model) => Ok(Some(Self::model_to_domain(model)?)),
            None => Ok(None),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_all(&self, request_etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        let started = Instant::now();
        let response_etag = etag::Entity::find_by_id(DISCOVERY_BACKEND_GET_ALL_ETAG_ID)
            .one(&self.db)
            .await;
        record_db_operation(
            started.elapsed(),
            &self.db_target,
            "discovery_backend_etag",
            "get_all_etag",
            &response_etag,
        );
        let response_etag = response_etag
            .foreign_context("fetching etag value", ErrorOrigin::Internal)?
            .map(|e| e.value as u64)
            .unwrap_or(0);

        if request_etag == Some(response_etag) {
            Ok(DiscoveryBackends {
                etag: response_etag,
                backends: None,
            })
        } else {
            let started = Instant::now();
            let models = Entity::find()
                .order_by_asc(Column::CreatedAt)
                .order_by_asc(Column::Id)
                .all(&self.db)
                .await;
            record_db_operation(
                started.elapsed(),
                &self.db_target,
                "discovery_backend",
                "get_all_backends",
                &models,
            );
            let models = models.foreign_context("fetching all backends", ErrorOrigin::Internal)?;

            let backends = models
                .into_iter()
                .map(Self::model_to_domain)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DiscoveryBackends {
                etag: response_etag,
                backends: Some(backends),
            })
        }
    }

    #[tracing::instrument(skip_all)]
    async fn post(&self, backend: DiscoveryBackend) -> Result<Option<PublicKey>, Self::Error> {
        let now = Utc::now();
        let active_model = ActiveModel {
            partitions: Set(DiscoveryBackendPartitions(backend.backend.partitions)),
            id: Set(backend.public_key.serialize().to_vec()),
            name: Set(backend.backend.name),
            weight: Set(backend.backend.weight as i32),
            enabled: Set(backend.backend.enabled),
            implementation: Set(backend.backend.implementation),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };

        let started = Instant::now();
        let transaction = self
            .db
            .transaction::<_, (Result<_, _>, Option<Result<_, _>>), sea_orm::DbErr>(|txn| {
                Box::pin(async move {
                    let insert = active_model.insert(txn).await;
                    let etag = if insert.is_ok() {
                        Some(
                            etag::Entity::update_many()
                                .col_expr(
                                    etag::Column::Value,
                                    Expr::col(etag::Column::Value).add(1),
                                )
                                .filter(etag::Column::Id.eq(DISCOVERY_BACKEND_GET_ALL_ETAG_ID))
                                .exec(txn)
                                .await,
                        )
                    } else {
                        None
                    };
                    Ok((insert, etag))
                })
            })
            .await;
        record_db_operation(
            started.elapsed(),
            &self.db_target,
            "discovery_backend",
            "post",
            &transaction,
        );
        let (insert_result, etag_result) =
            transaction.foreign_context("post transaction", ErrorOrigin::Internal)?;

        etag_result
            .transpose()
            .foreign_context("incrementing etag value", ErrorOrigin::Internal)?;

        match insert_result {
            Ok(_) => Ok(Some(backend.public_key)),
            // Unique constraint violation (Postgres via Query, SQLite via Exec)
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
            | Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
                if matches!(sqlx_err.as_ref(), sqlx::Error::Database(db_err) if db_err.is_unique_violation()) =>
            {
                Ok(None)
            }
            Err(e) => Err(e).foreign_context(
                format!("inserting backend for public key {}", backend.public_key),
                ErrorOrigin::Internal,
            ),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let now = Utc::now();
        let future_timestamp = now + chrono::Duration::seconds(1);

        let id = backend.public_key.serialize();
        let active_model = ActiveModel {
            partitions: Set(DiscoveryBackendPartitions(backend.backend.partitions)),
            id: Set(id.to_vec()),
            name: Set(backend.backend.name),
            weight: Set(backend.backend.weight as i32),
            enabled: Set(backend.backend.enabled),
            implementation: Set(backend.backend.implementation),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };

        let started = Instant::now();
        let transaction = self
            .db
            .transaction::<_, (Result<_, _>, Result<_, _>, Option<Result<_, _>>), sea_orm::DbErr>(
                |txn| {
                    Box::pin(async move {
                        let upsert = Entity::insert(active_model)
                            .on_conflict(
                                OnConflict::columns([Column::Id])
                                    .update_columns([
                                        Column::Name,
                                        Column::Weight,
                                        Column::Enabled,
                                        Column::Implementation,
                                    ])
                                    .value(Column::UpdatedAt, Expr::val(future_timestamp))
                                    .to_owned(),
                            )
                            .exec(txn)
                            .await;

                        let timestamps = if upsert.is_ok() {
                            Entity::find()
                                .filter(Column::Id.eq(id.as_slice()))
                                .select_only()
                                .column(Column::CreatedAt)
                                .column(Column::UpdatedAt)
                                .into_tuple::<(DateTimeWithTimeZone, DateTimeWithTimeZone)>()
                                .one(txn)
                                .await
                        } else {
                            Ok(None)
                        };

                        let etag = if timestamps.is_ok() {
                            Some(
                                etag::Entity::update_many()
                                    .col_expr(
                                        etag::Column::Value,
                                        Expr::col(etag::Column::Value).add(1),
                                    )
                                    .filter(etag::Column::Id.eq(DISCOVERY_BACKEND_GET_ALL_ETAG_ID))
                                    .exec(txn)
                                    .await,
                            )
                        } else {
                            None
                        };

                        Ok((upsert, timestamps, etag))
                    })
                },
            )
            .await;
        record_db_operation(
            started.elapsed(),
            &self.db_target,
            "discovery_backend",
            "put",
            &transaction,
        );
        let (upsert_result, fetch_result, etag_result) =
            transaction.foreign_context("put transaction", ErrorOrigin::Internal)?;

        upsert_result.with_foreign_context(
            || format!("upserting backend for public key {}", backend.public_key),
            ErrorOrigin::Internal,
        )?;

        etag_result
            .transpose()
            .foreign_context("incrementing etag value", ErrorOrigin::Internal)?;

        let result = fetch_result
            .with_foreign_context(
                || {
                    format!(
                        "fetching backend after upsert for public key {}",
                        backend.public_key
                    )
                },
                ErrorOrigin::Internal,
            )?
            .ok_or_else(|| {
                DefaultDiscoveryBackendStoreError::message(
                    ErrorOrigin::Internal,
                    "upsert succeeded but record not found: Record should exist after successful upsert",
                )
            })?;

        // Compare timestamps to determine if it was insert (true) or update (false)
        Ok(result.0 == result.1)
    }

    #[tracing::instrument(skip_all)]
    async fn patch(&self, backend: DiscoveryBackendPatch) -> Result<bool, Self::Error> {
        let mut update =
            Entity::update_many().filter(Column::Id.eq(backend.public_key.serialize().as_slice()));

        if let Some(name) = backend.backend.name {
            update = update.col_expr(Column::Name, Expr::value(name));
        }
        if let Some(partitions) = backend.backend.partitions {
            update = update.col_expr(
                Column::Partitions,
                Expr::value(DiscoveryBackendPartitions(partitions)),
            );
        }
        if let Some(weight) = backend.backend.weight {
            update = update.col_expr(Column::Weight, Expr::value(weight as i32));
        }
        if let Some(enabled) = backend.backend.enabled {
            update = update.col_expr(Column::Enabled, Expr::value(enabled));
        }

        update = update.col_expr(Column::UpdatedAt, Expr::value(Utc::now()));

        let started = Instant::now();
        let transaction = self
            .db
            .transaction::<_, _, _>(|txn| {
                Box::pin(async move {
                    let patch = update.exec(txn).await;

                    let etag = if patch
                        .as_ref()
                        .ok()
                        .map(|r| r.rows_affected > 0)
                        .unwrap_or(false)
                    {
                        Some(
                            etag::Entity::update_many()
                                .col_expr(
                                    etag::Column::Value,
                                    Expr::col(etag::Column::Value).add(1),
                                )
                                .filter(etag::Column::Id.eq(DISCOVERY_BACKEND_GET_ALL_ETAG_ID))
                                .exec(txn)
                                .await,
                        )
                    } else {
                        None
                    };

                    Ok((patch, etag))
                })
            })
            .await;
        record_db_operation(
            started.elapsed(),
            &self.db_target,
            "discovery_backend",
            "patch",
            &transaction,
        );
        let (patch_result, etag_result) =
            transaction.foreign_context("patch transaction", ErrorOrigin::Internal)?;

        etag_result
            .transpose()
            .foreign_context("incrementing etag value", ErrorOrigin::Internal)?;

        let result = patch_result.with_foreign_context(
            || format!("patching backend for public key {}", backend.public_key),
            ErrorOrigin::Internal,
        )?;

        Ok(result.rows_affected > 0)
    }

    #[tracing::instrument(skip_all)]
    async fn delete(&self, public_key: &PublicKey) -> Result<bool, Self::Error> {
        let id = public_key.serialize();

        let started = Instant::now();
        let transaction = self
            .db
            .transaction::<_, _, _>(|txn| {
                Box::pin(async move {
                    let delete = Entity::delete_by_id(id).exec(txn).await;

                    let etag = if delete
                        .as_ref()
                        .ok()
                        .map(|r| r.rows_affected > 0)
                        .unwrap_or(false)
                    {
                        Some(
                            etag::Entity::update_many()
                                .col_expr(
                                    etag::Column::Value,
                                    Expr::col(etag::Column::Value).add(1),
                                )
                                .filter(etag::Column::Id.eq(DISCOVERY_BACKEND_GET_ALL_ETAG_ID))
                                .exec(txn)
                                .await,
                        )
                    } else {
                        None
                    };

                    Ok((delete, etag))
                })
            })
            .await;
        record_db_operation(
            started.elapsed(),
            &self.db_target,
            "discovery_backend",
            "delete",
            &transaction,
        );
        let (delete_result, etag_result) =
            transaction.foreign_context("delete transaction", ErrorOrigin::Internal)?;

        etag_result
            .transpose()
            .foreign_context("incrementing etag value", ErrorOrigin::Internal)?;

        let result = delete_result.with_foreign_context(
            || format!("deleting backend for public key {public_key}"),
            ErrorOrigin::Internal,
        )?;

        Ok(result.rows_affected > 0)
    }

    #[tracing::instrument(skip_all)]
    async fn disconnect(&self) -> Result<(), Self::Error> {
        if let Some(r) = &self.rotation {
            r.shutdown().await?;
        }
        Ok(())
    }
}
