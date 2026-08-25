use crate::discovery::db::Column;
use crate::offer::db_orm::prelude::*;
use crate::offer::db_orm::{offer_metadata_table, offer_record_table};
use crate::offer::error::DefaultOfferStoreError;
use crate::secrets::SecretStore;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ColumnTrait, Database, DatabaseConnection, DatabaseConnectionType, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use secrecy::{ExposeSecret, SecretString};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_migration::OnConflict;
use switchgear_migration::{Expr, MigratorTrait};
use switchgear_service_api::offer::{
    OfferMetadata, OfferMetadataStore, OfferRecord, OfferRecordSparse, OfferStore,
};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

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
    ) -> Result<Self, DefaultOfferStoreError> {
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
                            tracing::warn!(error = %e, "offer DSN rotation refresh failed");
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
                return Err(DefaultOfferStoreError::message(
                    ErrorOrigin::Internal,
                    "storing offer DSN rotation task handle",
                ));
            };
            *join = Some(handle);
        }
        Ok(Self { inner })
    }

    fn refresh(inner: &CredentialRotationInner) -> Result<(), DefaultOfferStoreError> {
        let new = inner
            .secrets
            .replace(&inner.uri_template)
            .chained_context("re-resolving offer database URI secrets", None)?;

        let Ok(mut last) = inner.last_applied.lock() else {
            return Err(DefaultOfferStoreError::message(
                ErrorOrigin::Internal,
                "reading last applied offer DSN for rotation",
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

    async fn shutdown(&self) -> Result<(), DefaultOfferStoreError> {
        let tx = {
            let Ok(mut shutdown_tx) = self.inner.shutdown_tx.lock() else {
                return Err(DefaultOfferStoreError::message(
                    ErrorOrigin::Internal,
                    "signalling offer DSN rotation task to stop",
                ));
            };
            shutdown_tx.take()
        };
        if let Some(tx) = tx {
            let _ = tx.send(true);
        }

        let handle = {
            let Ok(mut join) = self.inner.join.lock() else {
                return Err(DefaultOfferStoreError::message(
                    ErrorOrigin::Internal,
                    "taking offer DSN rotation task handle for join",
                ));
            };
            join.take()
        };

        if let Some(h) = handle
            && let Err(e) = h.await
        {
            tracing::warn!(error = %e, "offer DSN rotation task join failed");
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DbOfferStore {
    db: DatabaseConnection,
    rotation: Option<CredentialRotation>,
}

impl DbOfferStore {
    #[tracing::instrument(skip_all)]
    pub async fn connect(
        uri: &str,
        secrets: Option<&SecretStore>,
        max_connections: u32,
        connect_timeout_secs: f64,
        acquire_timeout_secs: f64,
    ) -> Result<Self, DefaultOfferStoreError> {
        let resolved = match secrets {
            Some(s) => s
                .replace(uri)
                .chained_context("resolving offer database URI secrets", None)?,
            None => SecretString::from(uri.to_owned()),
        };
        let mut opt = sea_orm::ConnectOptions::new(resolved.expose_secret());
        opt.max_connections(max_connections)
            .connect_timeout(Duration::from_secs_f64(connect_timeout_secs))
            .acquire_timeout(Duration::from_secs_f64(acquire_timeout_secs));
        let db = Database::connect(opt)
            .await
            .foreign_context("connecting to offer database", ErrorOrigin::Internal)?;

        let rotation = secrets
            .map(|s| CredentialRotation::spawn(db.clone(), uri.to_owned(), s.clone(), resolved))
            .transpose()?;

        Ok(Self { db, rotation })
    }

    #[tracing::instrument(skip_all)]
    pub async fn migrate_up(&self) -> Result<(), DefaultOfferStoreError> {
        switchgear_migration::OfferMigrator::up(&self.db, None)
            .await
            .foreign_context("migrating database up", ErrorOrigin::Internal)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn migrate_down(&self) -> Result<(), DefaultOfferStoreError> {
        switchgear_migration::OfferMigrator::down(&self.db, None)
            .await
            .foreign_context("migrating database down", ErrorOrigin::Internal)?;
        Ok(())
    }
}

#[async_trait]
impl OfferStore for DbOfferStore {
    type Error = DefaultOfferStoreError;

    #[tracing::instrument(skip_all)]
    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
        sparse: Option<bool>,
    ) -> Result<Option<OfferRecord>, Self::Error> {
        let sparse = sparse.unwrap_or(true);

        let result = OfferRecordTable::find_by_id((partition.to_string(), *id))
            .find_also_related(OfferMetadataTable)
            .one(&self.db)
            .await
            .with_foreign_context(
                || format!("getting offer with metadata for partition {partition} id {id}"),
                ErrorOrigin::Internal,
            )?;

        let (offer_model, metadata_model) = match (result, sparse) {
            (Some((offer, Some(metadata))), false) => {
                let metadata = serde_json::from_value(metadata.metadata).with_foreign_context(
                    || format!("deserializing metadata for partition {partition} id {id}"),
                    ErrorOrigin::Internal,
                )?;
                (offer, Some(metadata))
            }
            (Some((offer, _)), true) => (offer, None),
            _ => return Ok(None),
        };

        Ok(Some(OfferRecord {
            partition: offer_model.partition,
            id: offer_model.id,
            offer: OfferRecordSparse {
                max_sendable: offer_model.max_sendable as u64,
                min_sendable: offer_model.min_sendable as u64,
                metadata_id: offer_model.metadata_id,
                metadata: metadata_model,
                timestamp: offer_model.timestamp.into(),
                expires: offer_model.expires.map(|dt| dt.into()),
            },
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn get_offers(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferRecord>, Self::Error> {
        let models = OfferRecordTable::find()
            .filter(offer_record_table::Column::Partition.eq(partition))
            .order_by_asc(offer_record_table::Column::CreatedAt)
            .order_by_asc(offer_record_table::Column::Id)
            .offset(start as u64)
            .limit(count as u64)
            .all(&self.db)
            .await
            .with_foreign_context(
                || format!("getting offers for partition {partition}"),
                ErrorOrigin::Internal,
            )?;

        let mut offers = Vec::new();
        for model in models {
            offers.push(OfferRecord {
                partition: model.partition,
                id: model.id,
                offer: OfferRecordSparse {
                    max_sendable: model.max_sendable as u64,
                    min_sendable: model.min_sendable as u64,
                    metadata_id: model.metadata_id,
                    metadata: None,
                    timestamp: model.timestamp.into(),
                    expires: model.expires.map(|dt| dt.into()),
                },
            });
        }

        Ok(offers)
    }

    #[tracing::instrument(skip_all)]
    async fn post_offer(&self, offer: OfferRecord) -> Result<Option<Uuid>, Self::Error> {
        let now = Utc::now();
        let active_model = offer_record_table::ActiveModel {
            id: Set(offer.id),
            partition: Set(offer.partition.clone()),
            max_sendable: Set(offer.offer.max_sendable as i64),
            min_sendable: Set(offer.offer.min_sendable as i64),
            metadata_id: Set(offer.offer.metadata_id),
            timestamp: Set(offer.offer.timestamp.into()),
            expires: Set(offer.offer.expires.map(|dt| dt.into())),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };

        match OfferRecordTable::insert(active_model).exec(&self.db).await {
            Ok(_) => Ok(Some(offer.id)),
            // Unique constraint violation (Postgres via Query, SQLite via Exec)
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
            | Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
                if matches!(sqlx_err.as_ref(), sqlx::Error::Database(db_err) if db_err.is_unique_violation()) =>
            {
                Ok(None)
            }
            // Foreign key constraint violation (metadata_id doesn't exist)
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
            | Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
                if matches!(sqlx_err.as_ref(), sqlx::Error::Database(db_err) if db_err.is_foreign_key_violation()) =>
            {
                Err(DefaultOfferStoreError::invalid_input_error(
                    format!("post offer {offer:?}"),
                    format!(
                        "metadata {} not found for offer {}",
                        offer.offer.metadata_id, offer.id
                    ),
                ))
            }
            Err(e) => Err(e).foreign_context(
                format!(
                    "inserting offer for partition {} id {}",
                    offer.partition, offer.id
                ),
                ErrorOrigin::Internal,
            ),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn put_offer(&self, offer: OfferRecord) -> Result<bool, Self::Error> {
        let now = Utc::now();
        let future_timestamp = now + chrono::Duration::seconds(1);

        let active_model = offer_record_table::ActiveModel {
            id: Set(offer.id),
            partition: Set(offer.partition.clone()),
            max_sendable: Set(offer.offer.max_sendable as i64),
            min_sendable: Set(offer.offer.min_sendable as i64),
            metadata_id: Set(offer.offer.metadata_id),
            timestamp: Set(offer.offer.timestamp.into()),
            expires: Set(offer.offer.expires.map(|dt| dt.into())),
            created_at: Set(now.into()), // Set for initial insert
            updated_at: Set(now.into()),
        };

        let _result = match OfferRecordTable::insert(active_model)
            .on_conflict(
                OnConflict::columns([
                    offer_record_table::Column::Partition,
                    offer_record_table::Column::Id,
                ])
                .update_columns([
                    offer_record_table::Column::MaxSendable,
                    offer_record_table::Column::MinSendable,
                    offer_record_table::Column::MetadataId,
                    offer_record_table::Column::Timestamp,
                    offer_record_table::Column::Expires,
                ])
                .value(Column::UpdatedAt, Expr::val(future_timestamp))
                .to_owned(),
            )
            .exec(&self.db)
            .await
        {
            Ok(result) => result,
            // Foreign key constraint violation (metadata_id doesn't exist)
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
            | Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
                if matches!(sqlx_err.as_ref(), sqlx::Error::Database(db_err) if db_err.is_foreign_key_violation()) =>
            {
                return Err(DefaultOfferStoreError::invalid_input_error(
                    format!("put offer {offer:?}"),
                    format!(
                        "metadata {} not found for offer {}",
                        offer.offer.metadata_id, offer.id
                    ),
                ));
            }
            Err(e) => {
                return Err(e).foreign_context(
                    format!(
                        "upserting offer for partition {} id {}",
                        offer.partition, offer.id
                    ),
                    ErrorOrigin::Internal,
                );
            }
        };

        // Fetch only the timestamps to compare
        let result = OfferRecordTable::find()
            .filter(offer_record_table::Column::Partition.eq(offer.partition.clone()))
            .filter(offer_record_table::Column::Id.eq(offer.id))
            .select_only()
            .column(offer_record_table::Column::CreatedAt)
            .column(offer_record_table::Column::UpdatedAt)
            .into_tuple::<(
                chrono::DateTime<chrono::FixedOffset>,
                chrono::DateTime<chrono::FixedOffset>,
            )>()
            .one(&self.db)
            .await
            .with_foreign_context(
                || {
                    format!(
                        "fetching offer after upsert for partition {} id {}",
                        offer.partition, offer.id
                    )
                },
                ErrorOrigin::Internal,
            )?
            .ok_or_else(|| {
                DefaultOfferStoreError::message(
                    ErrorOrigin::Internal,
                    "upsert succeeded but record not found",
                )
            })?;

        // Compare timestamps to determine if it was insert (true) or update (false)
        Ok(result.0 == result.1)
    }

    #[tracing::instrument(skip_all)]
    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let result = OfferRecordTable::delete_by_id((partition.to_string(), *id))
            .exec(&self.db)
            .await
            .with_foreign_context(
                || format!("deleting offer for partition {partition} id {id}"),
                ErrorOrigin::Internal,
            )?;

        Ok(result.rows_affected > 0)
    }

    async fn disconnect(&self) -> Result<(), Self::Error> {
        if let Some(r) = &self.rotation {
            r.shutdown().await?;
        }
        Ok(())
    }
}

#[async_trait]
impl OfferMetadataStore for DbOfferStore {
    type Error = DefaultOfferStoreError;

    #[tracing::instrument(skip_all)]
    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferMetadata>, Self::Error> {
        let model = OfferMetadataTable::find_by_id((partition.to_string(), *id))
            .one(&self.db)
            .await
            .with_foreign_context(
                || format!("getting metadata for partition {partition} id {id}"),
                ErrorOrigin::Internal,
            )?;

        match model {
            Some(model) => {
                let metadata = serde_json::from_value(model.metadata).with_foreign_context(
                    || format!("deserializing metadata for partition {partition} id {id}"),
                    ErrorOrigin::Internal,
                )?;

                Ok(Some(OfferMetadata {
                    id: model.id,
                    partition: model.partition,
                    metadata,
                }))
            }
            None => Ok(None),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_all_metadata(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferMetadata>, Self::Error> {
        let models = OfferMetadataTable::find()
            .filter(offer_metadata_table::Column::Partition.eq(partition))
            .order_by_asc(offer_metadata_table::Column::CreatedAt)
            .order_by_asc(offer_metadata_table::Column::Id)
            .offset(start as u64)
            .limit(count as u64)
            .all(&self.db)
            .await
            .with_foreign_context(
                || format!("getting all metadata for partition {partition}"),
                ErrorOrigin::Internal,
            )?;

        let mut metadata_list = Vec::new();
        for model in models {
            let metadata = serde_json::from_value(model.metadata).with_foreign_context(
                || {
                    format!(
                        "deserializing metadata for partition {} id {}",
                        partition, model.id
                    )
                },
                ErrorOrigin::Internal,
            )?;

            metadata_list.push(OfferMetadata {
                id: model.id,
                partition: model.partition,
                metadata,
            });
        }

        Ok(metadata_list)
    }

    #[tracing::instrument(skip_all)]
    async fn post_metadata(&self, offer: OfferMetadata) -> Result<Option<Uuid>, Self::Error> {
        let metadata_json = serde_json::to_value(&offer.metadata).with_foreign_context(
            || {
                format!(
                    "serializing metadata for partition {} id {}",
                    offer.partition, offer.id
                )
            },
            ErrorOrigin::Internal,
        )?;

        let now = Utc::now();
        let active_model = offer_metadata_table::ActiveModel {
            id: Set(offer.id),
            partition: Set(offer.partition.clone()),
            metadata: Set(metadata_json),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };

        match OfferMetadataTable::insert(active_model)
            .exec(&self.db)
            .await
        {
            Ok(_) => Ok(Some(offer.id)),
            // Unique constraint violation (Postgres via Query, SQLite via Exec)
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
            | Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
                if matches!(sqlx_err.as_ref(), sqlx::Error::Database(db_err) if db_err.is_unique_violation()) =>
            {
                Ok(None)
            }
            Err(e) => Err(e).foreign_context(
                format!(
                    "inserting metadata for partition {} id {}",
                    offer.partition, offer.id
                ),
                ErrorOrigin::Internal,
            ),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn put_metadata(&self, offer: OfferMetadata) -> Result<bool, Self::Error> {
        let metadata_json = serde_json::to_value(&offer.metadata).with_foreign_context(
            || {
                format!(
                    "serializing metadata for partition {} id {}",
                    offer.partition, offer.id
                )
            },
            ErrorOrigin::Internal,
        )?;

        let now = Utc::now();
        let future_timestamp = now + chrono::Duration::seconds(1);

        let active_model = offer_metadata_table::ActiveModel {
            id: Set(offer.id),
            partition: Set(offer.partition.clone()),
            metadata: Set(metadata_json),
            created_at: Set(now.into()), // Set for initial insert
            updated_at: Set(now.into()),
        };

        let _result = OfferMetadataTable::insert(active_model)
            .on_conflict(
                OnConflict::columns([
                    offer_metadata_table::Column::Partition,
                    offer_metadata_table::Column::Id,
                ])
                .update_columns([offer_metadata_table::Column::Metadata])
                .value(Column::UpdatedAt, Expr::val(future_timestamp))
                .to_owned(),
            )
            .exec(&self.db)
            .await
            .with_foreign_context(
                || {
                    format!(
                        "upserting metadata for partition {} id {}",
                        offer.partition, offer.id
                    )
                },
                ErrorOrigin::Internal,
            )?;

        // Fetch only the timestamps to compare
        let result = OfferMetadataTable::find()
            .filter(offer_metadata_table::Column::Partition.eq(offer.partition.clone()))
            .filter(offer_metadata_table::Column::Id.eq(offer.id))
            .select_only()
            .column(offer_metadata_table::Column::CreatedAt)
            .column(offer_metadata_table::Column::UpdatedAt)
            .into_tuple::<(
                chrono::DateTime<chrono::FixedOffset>,
                chrono::DateTime<chrono::FixedOffset>,
            )>()
            .one(&self.db)
            .await
            .with_foreign_context(
                || {
                    format!(
                        "fetching metadata after upsert for partition {} id {}",
                        offer.partition, offer.id
                    )
                },
                ErrorOrigin::Internal,
            )?
            .ok_or_else(|| {
                DefaultOfferStoreError::message(
                    ErrorOrigin::Internal,
                    "upsert succeeded but record not found",
                )
            })?;

        // Compare timestamps to determine if it was insert (true) or update (false)
        Ok(result.0 == result.1)
    }

    #[tracing::instrument(skip_all)]
    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let result = match OfferMetadataTable::delete_by_id((partition.to_string(), *id))
            .exec(&self.db)
            .await
        {
            Ok(result) => result,
            Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx_err)))
                if matches!(
                    sqlx_err.as_ref(),
                    sqlx::Error::Database(db_err)
                        // sqlite error code 1811 = SQLITE_CONSTRAINT_TRIGGER
                        if db_err.is_foreign_key_violation() || db_err.code().as_deref() == Some("1811")
                ) =>
            {
                return Err(DefaultOfferStoreError::invalid_input_error(
                    format!("deleting metadata for partition {partition} id {id}"),
                    format!("metadata {} is referenced by existing offers", id),
                ));
            }
            Err(e) => {
                return Err(e).with_foreign_context(
                    || format!("deleting metadata for partition {partition} id {id}"),
                    ErrorOrigin::Internal,
                );
            }
        };

        Ok(result.rows_affected > 0)
    }
}
