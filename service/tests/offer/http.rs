use crate::common::{offer, service};
use std::path::PathBuf;
use std::time::Duration;
use switchgear_service::api::offer::HttpOfferClient;
use switchgear_service::components::offer::http::HttpOfferStore;

async fn create_http_store() -> (HttpOfferStore, service::TestService) {
    let ports_path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let test_service = service::TestService::start(&ports_path).await.unwrap();
    let base_url = test_service.offer_base_url();

    let store = HttpOfferStore::create(
        base_url.parse().unwrap(),
        Duration::from_secs(10),
        Duration::from_secs(10),
        vec![],
        test_service.offer_authorization.clone(),
    )
    .unwrap();

    (store, test_service)
}

#[tokio::test]
async fn test_http_get_nonexistent_offer() {
    let (store, service) = create_http_store().await;
    offer::test_get_nonexistent_offer(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_post_new_offer() {
    let (store, service) = create_http_store().await;
    offer::test_post_new_offer(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_post_existing_offer() {
    let (store, service) = create_http_store().await;
    offer::test_post_existing_offer(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_put_new_offer() {
    let (store, service) = create_http_store().await;
    offer::test_put_new_offer(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_put_existing_offer() {
    let (store, service) = create_http_store().await;
    offer::test_put_existing_offer(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_delete_existing_offer() {
    let (store, service) = create_http_store().await;
    offer::test_delete_existing_offer(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_delete_nonexistent_offer() {
    let (store, service) = create_http_store().await;
    offer::test_delete_nonexistent_offer(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_get_offers() {
    let (store, service) = create_http_store().await;
    offer::test_get_offers(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_get_nonexistent_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_get_nonexistent_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_post_new_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_post_new_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_post_existing_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_post_existing_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_put_new_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_put_new_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_put_existing_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_put_existing_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_delete_existing_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_delete_existing_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_delete_nonexistent_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_delete_nonexistent_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_get_all_offer_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_get_all_offer_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_offer_provider_successful_retrieval() {
    let (store, service) = create_http_store().await;
    offer::test_offer_provider_successful_retrieval(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_offer_provider_offer_not_found() {
    let (store, service) = create_http_store().await;
    offer::test_offer_provider_offer_not_found(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_offer_provider_metadata_not_found() {
    let (store, service) = create_http_store().await;
    offer::test_offer_provider_metadata_not_found_or_foreign_key_constraint(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_offer_provider_hash_consistency() {
    let (store, service) = create_http_store().await;
    offer::test_offer_provider_hash_consistency(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_offer_provider_different_metadata_different_hashes() {
    let (store, service) = create_http_store().await;
    offer::test_offer_provider_different_metadata_different_hashes(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_offer_provider_valid_current_offer_returns_some() {
    let (store, service) = create_http_store().await;
    offer::test_offer_provider_valid_current_offer_returns_some(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_health() {
    let (store, service) = create_http_store().await;
    store.health().await.unwrap();
    service.shutdown().await;
}
