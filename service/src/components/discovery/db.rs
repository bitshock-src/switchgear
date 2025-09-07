use crate::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
    DiscoveryBackendSparse, DiscoveryBackendStore,
};
use crate::api::service::ServiceErrorSource;
use crate::components::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, FromJsonQueryResult,
    QueryFilter, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use switchgear_migration::MigratorTrait;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
pub struct DiscoveryBackendPartitions(BTreeSet<String>);

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "discovery_backend")]
pub struct Model {
    #[sea_orm(column_type = "JsonBinary")]
    pub partitions: DiscoveryBackendPartitions,
    #[sea_orm(primary_key, auto_increment = false)]
    pub address: String,
    pub address_type: String,
    pub weight: i32,
    pub enabled: bool,
    pub implementation: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug)]
pub struct DbDiscoveryBackendStore {
    db: DatabaseConnection,
}

impl DbDiscoveryBackendStore {
    pub async fn connect(
        url: &str,
        max_connections: u32,
    ) -> Result<Self, DiscoveryBackendStoreError> {
        let mut opt = sea_orm::ConnectOptions::new(url);
        opt.max_connections(max_connections);
        let db = Database::connect(opt.clone()).await.map_err(|e| {
            DiscoveryBackendStoreError::from_db(
                ServiceErrorSource::Internal,
                format!("connecting to database with {opt:?}",),
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

    fn address_type_from_address(address: &DiscoveryBackendAddress) -> &'static str {
        match address {
            DiscoveryBackendAddress::PublicKey(_) => "publicKey",
            DiscoveryBackendAddress::Url(_) => "url",
        }
    }

    fn model_to_domain(model: Model) -> Result<DiscoveryBackend, DiscoveryBackendStoreError> {
        let address = Self::parse_address(&model.address, &model.address_type)?;
        let implementation: DiscoveryBackendImplementation =
            serde_json::from_value(model.implementation.clone()).map_err(|e| {
                DiscoveryBackendStoreError::json_serialization_error(
                    ServiceErrorSource::Internal,
                    "deserializing implementation from database",
                    e,
                )
            })?;

        Ok(DiscoveryBackend {
            address,
            backend: DiscoveryBackendSparse {
                partitions: model.partitions.0,
                weight: model.weight as usize,
                enabled: model.enabled,
                implementation,
            },
        })
    }

    fn parse_address(
        address_str: &str,
        address_type: &str,
    ) -> Result<DiscoveryBackendAddress, DiscoveryBackendStoreError> {
        match address_type {
            "publicKey" => address_str
                .parse()
                .map(DiscoveryBackendAddress::PublicKey)
                .map_err(|e| {
                    DiscoveryBackendStoreError::internal_error(
                        ServiceErrorSource::Internal,
                        "parsing public key from database",
                        format!("Invalid public key: {e}"),
                    )
                }),
            "url" => address_str
                .parse()
                .map(DiscoveryBackendAddress::Url)
                .map_err(|e| {
                    DiscoveryBackendStoreError::internal_error(
                        ServiceErrorSource::Internal,
                        "parsing URL from database",
                        format!("Invalid URL: {e}"),
                    )
                }),
            _ => Err(DiscoveryBackendStoreError::internal_error(
                ServiceErrorSource::Internal,
                "parsing address from database",
                format!("Unknown address type: {address_type}"),
            )),
        }
    }
}

#[async_trait]
impl DiscoveryBackendStore for DbDiscoveryBackendStore {
    type Error = DiscoveryBackendStoreError;

    async fn get(
        &self,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let address_str = addr.to_string();

        let result = Entity::find_by_id(address_str.clone())
            .one(&self.db)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("fetching backend for address {address_str}",),
                    e,
                )
            })?;

        match result {
            Some(model) => Ok(Some(Self::model_to_domain(model)?)),
            None => Ok(None),
        }
    }

    async fn get_all(&self) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let models = Entity::find().all(&self.db).await.map_err(|e| {
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
        Ok(backends)
    }

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error> {
        let address_str = backend.address.to_string();
        let address_type = Self::address_type_from_address(&backend.address);

        let implementation_json =
            serde_json::to_value(&backend.backend.implementation).map_err(|e| {
                DiscoveryBackendStoreError::json_serialization_error(
                    ServiceErrorSource::Internal,
                    "serializing implementation for database",
                    e,
                )
            })?;

        let now = Utc::now();
        let active_model = ActiveModel {
            partitions: Set(DiscoveryBackendPartitions(backend.backend.partitions)),
            address: Set(address_str.clone()),
            address_type: Set(address_type.to_string()),
            weight: Set(backend.backend.weight as i32),
            enabled: Set(backend.backend.enabled),
            implementation: Set(implementation_json),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };

        let result = active_model.insert(&self.db).await;

        match result {
            Ok(_) => Ok(Some(backend.address)),
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
                format!("inserting backend for address {address_str}"),
                e,
            )),
        }
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let address_str = backend.address.to_string();
        let address_type = Self::address_type_from_address(&backend.address);

        let implementation_json =
            serde_json::to_value(&backend.backend.implementation).map_err(|e| {
                DiscoveryBackendStoreError::json_serialization_error(
                    ServiceErrorSource::Internal,
                    "serializing implementation for database",
                    e,
                )
            })?;

        let now = Utc::now();
        let future_timestamp = now + chrono::Duration::seconds(1);

        let active_model = ActiveModel {
            partitions: Set(DiscoveryBackendPartitions(backend.backend.partitions)),
            address: Set(address_str.clone()),
            address_type: Set(address_type.to_string()),
            weight: Set(backend.backend.weight as i32),
            enabled: Set(backend.backend.enabled),
            implementation: Set(implementation_json),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };

        Entity::insert(active_model)
            .on_conflict(
                OnConflict::columns([Column::Address])
                    .update_columns([Column::Weight, Column::Enabled, Column::Implementation])
                    .value(Column::UpdatedAt, Expr::val(future_timestamp))
                    .to_owned(),
            )
            .exec(&self.db)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("upserting backend for address {address_str}"),
                    e,
                )
            })?;

        // Fetch only the timestamps to compare
        let result = Entity::find()
            .filter(Column::Address.eq(&address_str))
            .select_only()
            .column(Column::CreatedAt)
            .column(Column::UpdatedAt)
            .into_tuple::<(DateTimeWithTimeZone, DateTimeWithTimeZone)>()
            .one(&self.db)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("fetching backend after upsert for address {address_str}"),
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

    async fn delete(&self, addr: &DiscoveryBackendAddress) -> Result<bool, Self::Error> {
        let address_str = addr.to_string();

        let delete_result = Entity::delete_by_id(address_str.clone())
            .exec(&self.db)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("deleting backend for address {address_str}",),
                    e,
                )
            })?;

        Ok(delete_result.rows_affected > 0)
    }
}
