use std::path::Path;
use switchgear_service::components::offer::db::DbOfferStore;
use tempfile::TempDir;

use crate::common::offer;

async fn create_sqlite_store(path: &Path) -> DbOfferStore {
    let path = path.join("db.sqlite");
    let store = DbOfferStore::connect(&format!("sqlite://{}?mode=rwc", path.to_string_lossy()), 5)
        .await
        .unwrap();
    store.migrate_up().await.unwrap();
    store
}

#[tokio::test]
async fn test_sqlite_get_nonexistent_offer() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_get_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_sqlite_post_new_offer() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_post_new_offer(store).await;
}

#[tokio::test]
async fn test_sqlite_post_existing_offer() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_post_existing_offer(store).await;
}

#[tokio::test]
async fn test_sqlite_put_new_offer() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_put_new_offer(store).await;
}

#[tokio::test]
async fn test_sqlite_put_existing_offer() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_put_existing_offer(store).await;
}

#[tokio::test]
async fn test_sqlite_delete_existing_offer() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_delete_existing_offer(store).await;
}

#[tokio::test]
async fn test_sqlite_delete_nonexistent_offer() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_delete_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_sqlite_get_offers() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_get_offers(store).await;
}

#[tokio::test]
async fn test_sqlite_get_nonexistent_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_get_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_post_new_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_post_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_post_existing_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_post_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_put_new_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_put_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_put_existing_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_put_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_delete_existing_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_delete_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_delete_nonexistent_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_delete_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_get_all_offer_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_get_all_offer_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_offer_provider_successful_retrieval() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_offer_provider_successful_retrieval(store).await;
}

#[tokio::test]
async fn test_sqlite_offer_provider_offer_not_found() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_offer_provider_offer_not_found(store).await;
}

#[tokio::test]
async fn test_sqlite_offer_provider_metadata_not_found() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_offer_provider_metadata_not_found_or_foreign_key_constraint(store).await;
}

#[tokio::test]
async fn test_sqlite_offer_provider_hash_consistency() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_offer_provider_hash_consistency(store).await;
}

#[tokio::test]
async fn test_sqlite_offer_provider_different_metadata_different_hashes() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_offer_provider_different_metadata_different_hashes(store).await;
}

#[tokio::test]
async fn test_sqlite_offer_provider_valid_current_offer_returns_some() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_offer_provider_valid_current_offer_returns_some(store).await;
}

#[tokio::test]
async fn test_sqlite_post_offer_with_missing_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_post_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_sqlite_put_offer_with_missing_metadata() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    offer::test_put_offer_with_missing_metadata(store).await;
}
