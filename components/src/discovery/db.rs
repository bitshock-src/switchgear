use crate::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, FromJsonQueryResult,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use secp256k1::PublicKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use switchgear_migration::{MigratorTrait, DISCOVERY_BACKEND_GET_ALL_ETAG_ID};
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendSparse, DiscoveryBackendStore,
    DiscoveryBackends,
};
use switchgear_service_api::service::ServiceErrorSource;

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

#[derive(Clone, Debug)]
pub struct DbDiscoveryBackendStore {
    db: DatabaseConnection,
}

impl DbDiscoveryBackendStore {
    pub async fn connect(
        uri: &str,
        max_connections: u32,
    ) -> Result<Self, DiscoveryBackendStoreError> {
        let mut opt = sea_orm::ConnectOptions::new(uri);
        opt.max_connections(max_connections);
        let db = Database::connect(opt).await.map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                "connecting to discovery backend database",
                e,
            )
        })?;

        Ok(Self::from_db(db))
    }

    pub async fn migrate_up(&self) -> Result<(), DiscoveryBackendStoreError> {
        switchgear_migration::DiscoveryBackendMigrator::up(&self.db, None)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    "migrating database up",
                    e,
                )
            })?;
        Ok(())
    }

    pub async fn migrate_down(&self) -> Result<(), DiscoveryBackendStoreError> {
        switchgear_migration::DiscoveryBackendMigrator::down(&self.db, None)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    "migrating database down",
                    e,
                )
            })?;
        Ok(())
    }

    pub fn from_db(db: DatabaseConnection) -> Self {
        Self { db }
    }

    fn model_to_domain(model: Model) -> Result<DiscoveryBackend, DiscoveryBackendStoreError> {
        Ok(DiscoveryBackend {
            public_key: PublicKey::from_slice(&model.id).map_err(|e| {
                DiscoveryBackendStoreError::internal_error(
                    ServiceErrorSource::Internal,
                    format!("deserializing public key {:?} from database", model.id),
                    format!("deserializing failure: {e}"),
                )
            })?,
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
    type Error = DiscoveryBackendStoreError;

    async fn get(&self, public_key: &PublicKey) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let result = Entity::find_by_id(public_key.serialize())
            .one(&self.db)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("fetching backend for public key {public_key}",),
                    e,
                )
            })?;

        match result {
            Some(model) => Ok(Some(Self::model_to_domain(model)?)),
            None => Ok(None),
        }
    }

    async fn get_all(&self, request_etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        let response_etag = etag::Entity::find_by_id(DISCOVERY_BACKEND_GET_ALL_ETAG_ID)
            .one(&self.db)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    "fetching etag value",
                    e,
                )
            })?
            .map(|e| e.value as u64)
            .unwrap_or(0);

        if request_etag == Some(response_etag) {
            Ok(DiscoveryBackends {
                etag: response_etag,
                backends: None,
            })
        } else {
            let models = Entity::find()
                .order_by_asc(Column::CreatedAt)
                .order_by_asc(Column::Id)
                .all(&self.db)
                .await
                .map_err(|e| {
                    DiscoveryBackendStoreError::from_db(
                        ServiceErrorSource::Internal,
                        "fetching all backends",
                        e,
                    )
                })?;

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

        let (insert_result, etag_result) = self
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
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_tx(
                    ServiceErrorSource::Internal,
                    "post transaction",
                    e,
                )
            })?;

        etag_result.transpose().map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                "incrementing etag value",
                e,
            )
        })?;

        match insert_result {
            Ok(_) => Ok(Some(backend.public_key)),
            // PostgreSQL unique constraint violation
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_unique_violation() => Ok(None),
            // SQLite unique constraint violation
            Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_unique_violation() => Ok(None),
            Err(e) => Err(DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                format!("inserting backend for public key {}", backend.public_key),
                e,
            )),
        }
    }

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

        let (upsert_result, fetch_result, etag_result) = self
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
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_tx(
                    ServiceErrorSource::Internal,
                    "put transaction",
                    e,
                )
            })?;

        upsert_result.map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                format!("upserting backend for public key {}", backend.public_key),
                e,
            )
        })?;

        etag_result.transpose().map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                "incrementing etag value",
                e,
            )
        })?;

        let result = fetch_result
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!(
                        "fetching backend after upsert for public key {}",
                        backend.public_key
                    ),
                    e,
                )
            })?
            .ok_or_else(|| {
                DiscoveryBackendStoreError::internal_error(
                    ServiceErrorSource::Internal,
                    "upsert succeeded but record not found",
                    "Record should exist after successful upsert".to_string(),
                )
            })?;

        // Compare timestamps to determine if it was insert (true) or update (false)
        Ok(result.0 == result.1)
    }

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

        let (patch_result, etag_result) = self
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
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_tx(
                    ServiceErrorSource::Internal,
                    "patch transaction",
                    e,
                )
            })?;

        etag_result.transpose().map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                "incrementing etag value",
                e,
            )
        })?;

        let result = patch_result.map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                format!("patching backend for public key {}", backend.public_key),
                e,
            )
        })?;

        Ok(result.rows_affected > 0)
    }

    async fn delete(&self, public_key: &PublicKey) -> Result<bool, Self::Error> {
        let id = public_key.serialize();

        let (delete_result, etag_result) = self
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
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_tx(
                    ServiceErrorSource::Internal,
                    "delete transaction",
                    e,
                )
            })?;

        etag_result.transpose().map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                "incrementing etag value",
                e,
            )
        })?;

        let result = delete_result.map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                format!("deleting backend for public key {public_key}"),
                e,
            )
        })?;

        Ok(result.rows_affected > 0)
    }
}
