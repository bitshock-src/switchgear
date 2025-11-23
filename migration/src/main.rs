use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() {
    cli::run_cli(switchgear_migration::DiscoveryBackendMigrator).await;
    cli::run_cli(switchgear_migration::OfferMigrator).await;
}
