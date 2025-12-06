use crate::common::offer;
use anyhow::anyhow;
use switchgear_components::offer::db::DbOfferStore;
use switchgear_testing::db::TestPostgresDatabase;
use switchgear_testing::services::IntegrationTestServices;
use uuid::Uuid;

async fn create_postgres_store() -> (DbOfferStore, TestPostgresDatabase) {
    let _ = rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

    let db_name = format!("test_offer_{}", Uuid::new_v4().to_string().replace("-", ""));
    let services = IntegrationTestServices::new();

    let db = TestPostgresDatabase::new("postgres", &db_name, services.postgres(), false, None);

    let store = DbOfferStore::connect(db.connection_url(), 5).await.unwrap();
    store.migrate_up().await.unwrap();
    (store, db)
}

#[tokio::test]
async fn test_postgres_get_nonexistent_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_postgres_post_new_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_new_offer(store).await;
}

#[tokio::test]
async fn test_postgres_post_existing_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_existing_offer(store).await;
}

#[tokio::test]
async fn test_postgres_put_new_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_new_offer(store).await;
}

#[tokio::test]
async fn test_postgres_put_existing_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_existing_offer(store).await;
}

#[tokio::test]
async fn test_postgres_delete_existing_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_existing_offer(store).await;
}

#[tokio::test]
async fn test_postgres_delete_nonexistent_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_postgres_get_offers() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_offers(store).await;
}

#[tokio::test]
async fn test_postgres_get_nonexistent_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_post_new_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_post_existing_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_put_new_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_put_existing_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_delete_existing_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_delete_nonexistent_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_get_all_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_all_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_offer_provider_successful_retrieval() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_offer_provider_successful_retrieval(store).await;
}

#[tokio::test]
async fn test_postgres_offer_provider_offer_not_found() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_offer_provider_offer_not_found(store).await;
}

#[tokio::test]
async fn test_postgres_offer_provider_metadata_not_found() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_offer_provider_metadata_not_found_or_foreign_key_constraint(store).await;
}

#[tokio::test]
async fn test_postgres_offer_provider_hash_consistency() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_offer_provider_hash_consistency(store).await;
}

#[tokio::test]
async fn test_postgres_offer_provider_different_metadata_different_hashes() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_offer_provider_different_metadata_different_hashes(store).await;
}

#[tokio::test]
async fn test_postgres_offer_provider_valid_current_offer_returns_some() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_offer_provider_valid_current_offer_returns_some(store).await;
}

#[tokio::test]
async fn test_postgres_post_offer_with_missing_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_put_offer_with_missing_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_delete_metadata_with_referencing_offers() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_metadata_with_referencing_offers(store).await;
}
