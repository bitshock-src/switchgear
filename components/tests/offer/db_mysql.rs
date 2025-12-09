use crate::common::offer;
use anyhow::anyhow;
use switchgear_components::offer::db::DbOfferStore;
use switchgear_testing::db::TestMysqlDatabase;
use switchgear_testing::services::IntegrationTestServices;
use uuid::Uuid;

async fn create_mysql_store() -> (DbOfferStore, TestMysqlDatabase) {
    let _ = rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

    let db_name = format!(
        "test_discovery_{}",
        Uuid::new_v4().to_string().replace("-", "")
    );
    let services = IntegrationTestServices::new();

    let db = TestMysqlDatabase::new("root", &db_name, services.mysql(), false, None);

    let store = DbOfferStore::connect(db.connection_url(), 5).await.unwrap();
    store.migrate_up().await.unwrap();
    (store, db)
}

#[tokio::test]
async fn test_mysql_get_nonexistent_offer() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_get_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_mysql_post_new_offer() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_post_new_offer(store).await;
}

#[tokio::test]
async fn test_mysql_post_existing_offer() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_post_existing_offer(store).await;
}

#[tokio::test]
async fn test_mysql_put_new_offer() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_put_new_offer(store).await;
}

#[tokio::test]
async fn test_mysql_put_existing_offer() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_put_existing_offer(store).await;
}

#[tokio::test]
async fn test_mysql_delete_existing_offer() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_delete_existing_offer(store).await;
}

#[tokio::test]
async fn test_mysql_delete_nonexistent_offer() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_delete_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_mysql_get_offers() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_get_offers(store).await;
}

#[tokio::test]
async fn test_mysql_get_nonexistent_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_get_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_post_new_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_post_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_post_existing_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_post_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_put_new_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_put_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_put_existing_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_put_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_delete_existing_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_delete_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_delete_nonexistent_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_delete_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_get_all_offer_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_get_all_offer_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_post_offer_with_missing_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_post_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_put_offer_with_missing_metadata() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_put_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_mysql_delete_metadata_with_referencing_offers() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_delete_metadata_with_referencing_offers(store).await;
}
