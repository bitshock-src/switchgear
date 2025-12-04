use crate::DISCOVERY_BACKEND_GET_ALL_ETAG_ID;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct DiscoveryBackendMigration;

#[async_trait::async_trait]
impl MigrationTrait for DiscoveryBackendMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DiscoveryBackend::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DiscoveryBackend::Partitions)
                            .json_binary()
                            .not_null(),
                    )
                    .col(binary_len(DiscoveryBackend::Id, 33).not_null())
                    .col(string_null(DiscoveryBackend::Name))
                    .col(integer(DiscoveryBackend::Weight).not_null())
                    .col(boolean(DiscoveryBackend::Enabled).not_null())
                    .col(
                        ColumnDef::new(DiscoveryBackend::Implementation)
                            .json_binary()
                            .not_null(),
                    )
                    .col(timestamp_with_time_zone(DiscoveryBackend::CreatedAt).not_null())
                    .col(timestamp_with_time_zone(DiscoveryBackend::UpdatedAt).not_null())
                    .primary_key(Index::create().col(DiscoveryBackend::Id))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(DiscoveryBackendEtag::Table)
                    .if_not_exists()
                    .col(integer(DiscoveryBackendEtag::Id).not_null())
                    .col(big_integer(DiscoveryBackendEtag::Value).not_null())
                    .primary_key(Index::create().col(DiscoveryBackendEtag::Id))
                    .to_owned(),
            )
            .await?;

        let insert_stmt = Query::insert()
            .into_table(DiscoveryBackendEtag::Table)
            .columns([DiscoveryBackendEtag::Id, DiscoveryBackendEtag::Value])
            .values([DISCOVERY_BACKEND_GET_ALL_ETAG_ID.into(), 0.into()])
            .map_err(|e| DbErr::Custom(e.to_string()))?
            .to_owned();

        manager.exec_stmt(insert_stmt).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DiscoveryBackendEtag::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(DiscoveryBackend::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum DiscoveryBackend {
    Table,
    Partitions,
    Id,
    Name,
    Weight,
    Enabled,
    Implementation,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum DiscoveryBackendEtag {
    Table,
    Id,
    Value,
}
