use crate::discovery::db::Column;
use crate::offer::db_orm::prelude::*;
use crate::offer::db_orm::{offer_metadata_table, offer_record_table};
use crate::offer::error::OfferStoreError;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Set,
};
use sha2::{Digest, Sha256};
use switchgear_migration::OnConflict;
use switchgear_migration::{Expr, MigratorTrait};
use switchgear_service_api::lnurl::LnUrlOfferMetadata;
use switchgear_service_api::offer::{
    Offer, OfferMetadata, OfferMetadataSparse, OfferMetadataStore, OfferProvider, OfferRecord,
    OfferRecordSparse, OfferStore,
};
use switchgear_service_api::service::ServiceErrorSource;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct DbOfferStore {
    db: DatabaseConnection,
}

impl DbOfferStore {
    pub async fn connect(uri: &str, max_connections: u32) -> Result<Self, OfferStoreError> {
        let mut opt = sea_orm::ConnectOptions::new(uri);
        opt.max_connections(max_connections);
        let db = Database::connect(opt).await.map_err(|e| {
            OfferStoreError::from_db(
                ServiceErrorSource::Internal,
                "connecting to offer database",
                e,
            )
        })?;

        Ok(Self::from_db(db))
    }

    pub async fn migrate_up(&self) -> Result<(), OfferStoreError> {
        switchgear_migration::OfferMigrator::up(&self.db, None)
            .await
            .map_err(|e| {
                OfferStoreError::from_db(ServiceErrorSource::Internal, "migrating database up", e)
            })?;
        Ok(())
    }

    pub async fn migrate_down(&self) -> Result<(), OfferStoreError> {
        switchgear_migration::OfferMigrator::down(&self.db, None)
            .await
            .map_err(|e| {
                OfferStoreError::from_db(ServiceErrorSource::Internal, "migrating database down", e)
            })?;
        Ok(())
    }

    pub fn from_db(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl OfferStore for DbOfferStore {
    type Error = OfferStoreError;

    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferRecord>, Self::Error> {
        let model = OfferRecordTable::find_by_id((partition.to_string(), *id))
            .one(&self.db)
            .await
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("getting offer for partition {partition} id {id}"),
                    e,
                )
            })?;

        match model {
            Some(model) => Ok(Some(OfferRecord {
                partition: model.partition,
                id: model.id,
                offer: OfferRecordSparse {
                    max_sendable: model.max_sendable as u64,
                    min_sendable: model.min_sendable as u64,
                    metadata_id: model.metadata_id,
                    timestamp: model.timestamp.into(),
                    expires: model.expires.map(|dt| dt.into()),
                },
            })),
            None => Ok(None),
        }
    }

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
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("getting offers for partition {partition}"),
                    e,
                )
            })?;

        let mut offers = Vec::new();
        for model in models {
            offers.push(OfferRecord {
                partition: model.partition,
                id: model.id,
                offer: OfferRecordSparse {
                    max_sendable: model.max_sendable as u64,
                    min_sendable: model.min_sendable as u64,
                    metadata_id: model.metadata_id,
                    timestamp: model.timestamp.into(),
                    expires: model.expires.map(|dt| dt.into()),
                },
            });
        }

        Ok(offers)
    }

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
            // PostgreSQL unique constraint violation
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_unique_violation() => Ok(None),
            // SQLite unique constraint violation
            Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_unique_violation() => Ok(None),
            // Foreign key constraint violation (metadata_id doesn't exist)
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_foreign_key_violation() => Err(OfferStoreError::invalid_input_error(
                format!("post offer {offer:?}"),
                format!(
                    "metadata {} not found for offer {}",
                    offer.offer.metadata_id, offer.id
                ),
            )),
            Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_foreign_key_violation() => Err(OfferStoreError::invalid_input_error(
                format!("post offer {offer:?}"),
                format!(
                    "metadata {} not found for offer {}",
                    offer.offer.metadata_id, offer.id
                ),
            )),
            Err(e) => Err(OfferStoreError::from_db(
                ServiceErrorSource::Internal,
                format!(
                    "inserting offer for partition {} id {}",
                    offer.partition, offer.id
                ),
                e,
            )),
        }
    }

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
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_foreign_key_violation() => {
                return Err(OfferStoreError::invalid_input_error(
                    format!("put offer {offer:?}"),
                    format!(
                        "metadata {} not found for offer {}",
                        offer.offer.metadata_id, offer.id
                    ),
                ));
            }
            Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_foreign_key_violation() => {
                return Err(OfferStoreError::invalid_input_error(
                    format!("put offer {offer:?}"),
                    format!(
                        "metadata {} not found for offer {}",
                        offer.offer.metadata_id, offer.id
                    ),
                ));
            }
            Err(e) => {
                return Err(OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!(
                        "upserting offer for partition {} id {}",
                        offer.partition, offer.id
                    ),
                    e,
                ));
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
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!(
                        "fetching offer after upsert for partition {} id {}",
                        offer.partition, offer.id
                    ),
                    e,
                )
            })?
            .ok_or_else(|| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    "upsert succeeded but record not found",
                    sea_orm::DbErr::RecordNotFound(
                        "Record should exist after successful upsert".to_string(),
                    ),
                )
            })?;

        // Compare timestamps to determine if it was insert (true) or update (false)
        Ok(result.0 == result.1)
    }

    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let result = OfferRecordTable::delete_by_id((partition.to_string(), *id))
            .exec(&self.db)
            .await
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("deleting offer for partition {partition} id {id}"),
                    e,
                )
            })?;

        Ok(result.rows_affected > 0)
    }
}

#[async_trait]
impl OfferMetadataStore for DbOfferStore {
    type Error = OfferStoreError;

    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferMetadata>, Self::Error> {
        let model = OfferMetadataTable::find_by_id((partition.to_string(), *id))
            .one(&self.db)
            .await
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("getting metadata for partition {partition} id {id}"),
                    e,
                )
            })?;

        match model {
            Some(model) => {
                let metadata = serde_json::from_value(model.metadata).map_err(|e| {
                    OfferStoreError::serialization_error(
                        ServiceErrorSource::Internal,
                        format!("deserializing metadata for partition {partition} id {id}",),
                        e,
                    )
                })?;

                Ok(Some(OfferMetadata {
                    id: model.id,
                    partition: model.partition,
                    metadata,
                }))
            }
            None => Ok(None),
        }
    }

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
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("getting all metadata for partition {partition}"),
                    e,
                )
            })?;

        let mut metadata_list = Vec::new();
        for model in models {
            let metadata = serde_json::from_value(model.metadata).map_err(|e| {
                OfferStoreError::serialization_error(
                    ServiceErrorSource::Internal,
                    format!(
                        "deserializing metadata for partition {} id {}",
                        partition, model.id
                    ),
                    e,
                )
            })?;

            metadata_list.push(OfferMetadata {
                id: model.id,
                partition: model.partition,
                metadata,
            });
        }

        Ok(metadata_list)
    }

    async fn post_metadata(&self, offer: OfferMetadata) -> Result<Option<Uuid>, Self::Error> {
        let metadata_json = serde_json::to_value(&offer.metadata).map_err(|e| {
            OfferStoreError::serialization_error(
                ServiceErrorSource::Internal,
                format!(
                    "serializing metadata for partition {} id {}",
                    offer.partition, offer.id
                ),
                e,
            )
        })?;

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
            // PostgreSQL unique constraint violation
            Err(sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_unique_violation() => Ok(None),
            // SQLite unique constraint violation
            Err(sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                db_err,
            )))) if db_err.is_unique_violation() => Ok(None),
            Err(e) => Err(OfferStoreError::from_db(
                ServiceErrorSource::Internal,
                format!(
                    "inserting metadata for partition {} id {}",
                    offer.partition, offer.id
                ),
                e,
            )),
        }
    }

    async fn put_metadata(&self, offer: OfferMetadata) -> Result<bool, Self::Error> {
        let metadata_json = serde_json::to_value(&offer.metadata).map_err(|e| {
            OfferStoreError::serialization_error(
                ServiceErrorSource::Internal,
                format!(
                    "serializing metadata for partition {} id {}",
                    offer.partition, offer.id
                ),
                e,
            )
        })?;

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
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!(
                        "upserting metadata for partition {} id {}",
                        offer.partition, offer.id
                    ),
                    e,
                )
            })?;

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
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!(
                        "fetching metadata after upsert for partition {} id {}",
                        offer.partition, offer.id
                    ),
                    e,
                )
            })?
            .ok_or_else(|| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    "upsert succeeded but record not found",
                    sea_orm::DbErr::RecordNotFound(
                        "Record should exist after successful upsert".to_string(),
                    ),
                )
            })?;

        // Compare timestamps to determine if it was insert (true) or update (false)
        Ok(result.0 == result.1)
    }

    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let result = OfferMetadataTable::delete_by_id((partition.to_string(), *id))
            .exec(&self.db)
            .await
            .map_err(|e| match e {
                sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
                    db_err,
                ))) if db_err.is_foreign_key_violation()
                    // sqlite
                    || db_err.code().as_deref() == Some("1811") =>
                {
                    OfferStoreError::invalid_input_error(
                        format!("deleting metadata for partition {partition} id {id}"),
                        format!("metadata {} is referenced by existing offers", id),
                    )
                }
                _ => OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("deleting metadata for partition {partition} id {id}"),
                    e,
                ),
            })?;

        Ok(result.rows_affected > 0)
    }
}

#[async_trait]
impl OfferProvider for DbOfferStore {
    type Error = OfferStoreError;

    async fn offer(
        &self,
        _hostname: &str,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<Offer>, Self::Error> {
        let result = OfferRecordTable::find_by_id((partition.to_string(), *id))
            .find_also_related(OfferMetadataTable)
            .one(&self.db)
            .await
            .map_err(|e| {
                OfferStoreError::from_db(
                    ServiceErrorSource::Internal,
                    format!("getting offer with metadata for partition {partition} id {id}",),
                    e,
                )
            })?;

        let (offer_model, metadata_model) = match result {
            Some((offer, Some(metadata))) => (offer, metadata),
            _ => return Ok(None),
        };

        let metadata_sparse: OfferMetadataSparse = serde_json::from_value(metadata_model.metadata)
            .map_err(|e| {
                OfferStoreError::serialization_error(
                    ServiceErrorSource::Internal,
                    format!("deserializing metadata for offer {id} in partition {partition}",),
                    e,
                )
            })?;

        let lnurl_metadata = LnUrlOfferMetadata(metadata_sparse);
        let metadata_json_string = serde_json::to_string(&lnurl_metadata).map_err(|e| {
            OfferStoreError::serialization_error(
                ServiceErrorSource::Internal,
                format!("serializing metadata for offer {id} in partition {partition}",),
                e,
            )
        })?;

        let mut hasher = Sha256::new();
        hasher.update(metadata_json_string.as_bytes());
        let metadata_json_hash = hasher.finalize().into();

        Ok(Some(Offer {
            partition: offer_model.partition,
            id: offer_model.id,
            max_sendable: offer_model.max_sendable as u64,
            min_sendable: offer_model.min_sendable as u64,
            metadata_json_string,
            metadata_json_hash,
            timestamp: offer_model.timestamp.with_timezone(&Utc),
            expires: offer_model.expires.map(|dt| dt.with_timezone(&Utc)),
        }))
    }
}
