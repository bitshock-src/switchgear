use crate::common::{discovery, service};
use anyhow::anyhow;
use std::path::PathBuf;
use std::time::Duration;
use switchgear_service::api::discovery::HttpDiscoveryBackendClient;
use switchgear_service::components::discovery::http::HttpDiscoveryBackendStore;

async fn create_http_store() -> (HttpDiscoveryBackendStore, service::TestService) {
    let _ = rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

    let ports_path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let test_service = service::TestService::start(&ports_path).await.unwrap();
    let base_url = test_service.discovery_base_url();

    let store = HttpDiscoveryBackendStore::create(
        base_url,
        Duration::from_secs(10),
        Duration::from_secs(10),
        &[],
        test_service.discovery_authorization.clone(),
    )
    .unwrap();
    (store, test_service)
}

#[tokio::test]
async fn test_http_post_new_backend_returns_address() {
    let (store, service) = create_http_store().await;
    discovery::test_post_new_backend_returns_address(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_get_returns_correct_backends() {
    let (store, service) = create_http_store().await;
    discovery::test_get_returns_correct_backends(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_delete_removes_and_returns_backends() {
    let (store, service) = create_http_store().await;
    discovery::test_delete_removes_target(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_put_new_backend_returns_true() {
    let (store, service) = create_http_store().await;
    discovery::test_put_new_backend_returns_true(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_put_existing_backend_updates_and_returns_false() {
    let (store, service) = create_http_store().await;
    discovery::test_put_existing_backend_updates_and_returns_false(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_test_patch_backend() {
    let (store, service) = create_http_store().await;
    discovery::test_patch_backend(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_test_patch_missing_backend() {
    let (store, service) = create_http_store().await;
    discovery::test_patch_missing_backend(store).await;
    service.shutdown().await;
}

#[tokio::test]
async fn test_http_health() {
    let (store, service) = create_http_store().await;
    store.health().await.unwrap();
    service.shutdown().await;
}
