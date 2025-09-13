use crate::common::db::TestMysqlDatabase;
use crate::common::offer;
use switchgear_service::components::offer::db::DbOfferStore;
use uuid::Uuid;

async fn create_mysql_store() -> (DbOfferStore, TestMysqlDatabase) {
    let db_name = format!(
        "test_discovery_{}",
        Uuid::new_v4().to_string().replace("-", "")
    );
    let db = TestMysqlDatabase::new(db_name, 3306);

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
async fn test_mysql_offer_provider_successful_retrieval() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_offer_provider_successful_retrieval(store).await;
}

#[tokio::test]
async fn test_mysql_offer_provider_offer_not_found() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_offer_provider_offer_not_found(store).await;
}

#[tokio::test]
async fn test_mysql_offer_provider_metadata_not_found() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_offer_provider_metadata_not_found_or_foreign_key_constraint(store).await;
}

#[tokio::test]
async fn test_mysql_offer_provider_hash_consistency() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_offer_provider_hash_consistency(store).await;
}

#[tokio::test]
async fn test_mysql_offer_provider_different_metadata_different_hashes() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_offer_provider_different_metadata_different_hashes(store).await;
}

#[tokio::test]
async fn test_mysql_offer_provider_valid_current_offer_returns_some() {
    let (store, _guard) = create_mysql_store().await;
    offer::test_offer_provider_valid_current_offer_returns_some(store).await;
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
