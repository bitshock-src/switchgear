use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct OfferMigration;

#[async_trait::async_trait]
impl MigrationTrait for OfferMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OfferMetadataTable::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(OfferMetadataTable::Id).uuid().not_null())
                    .col(
                        ColumnDef::new(OfferMetadataTable::Partition)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OfferMetadataTable::Metadata)
                            .json()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OfferMetadataTable::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(OfferMetadataTable::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(OfferMetadataTable::Partition)
                            .col(OfferMetadataTable::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(OfferRecordTable::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(OfferRecordTable::Id).uuid().not_null())
                    .col(
                        ColumnDef::new(OfferRecordTable::Partition)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OfferRecordTable::MaxSendable)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OfferRecordTable::MinSendable)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OfferRecordTable::MetadataId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OfferRecordTable::Timestamp)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OfferRecordTable::Expires)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(OfferRecordTable::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(OfferRecordTable::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(OfferRecordTable::Partition)
                            .col(OfferRecordTable::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                OfferRecordTable::Table,
                                (OfferRecordTable::Partition, OfferRecordTable::MetadataId),
                            )
                            .to(
                                OfferMetadataTable::Table,
                                (OfferRecordTable::Partition, OfferMetadataTable::Id),
                            )
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop OfferRecord table first (has foreign key reference)
        manager
            .drop_table(Table::drop().table(OfferRecordTable::Table).to_owned())
            .await?;

        // Drop OfferMetadata table
        manager
            .drop_table(Table::drop().table(OfferMetadataTable::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum OfferRecordTable {
    Table,
    Id,
    Partition,
    MaxSendable,
    MinSendable,
    MetadataId,
    Timestamp,
    Expires,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum OfferMetadataTable {
    Table,
    Id,
    Partition,
    Metadata,
    CreatedAt,
    UpdatedAt,
}
