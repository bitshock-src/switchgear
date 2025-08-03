use switchgear_service::components::discovery::memory::MemoryDiscoveryBackendStore;

use crate::common::discovery;

#[tokio::test]
async fn test_memory_post_new_backend_returns_address() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_post_new_backend_returns_address(store).await;
}

#[tokio::test]
async fn test_memory_get_returns_correct_backends() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_get_returns_correct_backends(store).await;
}

#[tokio::test]
async fn test_memory_delete_removes_and_returns_backends() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_delete_removes_target(store).await;
}

#[tokio::test]
async fn test_memory_put_new_backend_returns_true() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_put_new_backend_returns_true(store).await;
}

#[tokio::test]
async fn test_memory_put_existing_backend_updates_and_returns_false() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_put_existing_backend_updates_and_returns_false(store).await;
}
