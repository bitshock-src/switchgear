use crate::common::{mock_service, offer};
use anyhow::anyhow;
use std::path::PathBuf;
use std::time::Duration;
use switchgear_components::offer::http::HttpOfferStore;
use switchgear_service_api::offer::HttpOfferClient;

async fn create_http_store() -> (HttpOfferStore, mock_service::TestService) {
    let _ = rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

    let ports_path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let test_service = mock_service::TestService::start(&ports_path).await.unwrap();
    let base_url = test_service.offer_base_url();

    let store = HttpOfferStore::create(
        base_url,
        Duration::from_secs(10),
        Duration::from_secs(10),
        &[],
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
async fn test_http_post_offer_with_missing_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_post_offer_with_missing_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_put_offer_with_missing_metadata() {
    let (store, service) = create_http_store().await;
    offer::test_put_offer_with_missing_metadata(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_delete_metadata_with_referencing_offers() {
    let (store, service) = create_http_store().await;
    offer::test_delete_metadata_with_referencing_offers(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_health() {
    let (store, service) = create_http_store().await;
    store.health().await.unwrap();
    service.shutdown().await;
}
