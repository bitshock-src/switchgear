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
                    .col(string(DiscoveryBackend::Partition).not_null())
                    .col(string(DiscoveryBackend::Address).not_null())
                    .col(string(DiscoveryBackend::AddressType).not_null())
                    .col(integer(DiscoveryBackend::Weight).not_null())
                    .col(boolean(DiscoveryBackend::Enabled).not_null())
                    .col(json(DiscoveryBackend::Implementation).not_null())
                    .col(timestamp_with_time_zone(DiscoveryBackend::CreatedAt).not_null())
                    .col(timestamp_with_time_zone(DiscoveryBackend::UpdatedAt).not_null())
                    .primary_key(
                        Index::create()
                            .col(DiscoveryBackend::Partition)
                            .col(DiscoveryBackend::Address),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DiscoveryBackend::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum DiscoveryBackend {
    Table,
    Partition,
    Address,
    AddressType,
    Weight,
    Enabled,
    Implementation,
    CreatedAt,
    UpdatedAt,
}
