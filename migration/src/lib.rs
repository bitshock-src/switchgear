pub use sea_orm_migration::prelude::*;

pub const DISCOVERY_BACKEND_GET_ALL_ETAG_ID: i32 = 1;

mod m20220101_000001_create_table;
mod m20250724_182058_create_table;

pub struct DiscoveryBackendMigrator;

#[async_trait::async_trait]
impl MigratorTrait for DiscoveryBackendMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(
            m20220101_000001_create_table::DiscoveryBackendMigration,
        )]
    }
}

pub struct OfferMigrator;

#[async_trait::async_trait]
impl MigratorTrait for OfferMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20250724_182058_create_table::OfferMigration)]
    }
}
