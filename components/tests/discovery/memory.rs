use switchgear_components::discovery::memory::MemoryDiscoveryBackendStore;

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

#[tokio::test]
async fn test_memory_test_patch_backend() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_patch_backend(store).await;
}

#[tokio::test]
async fn test_memory_test_patch_missing_backend() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_patch_missing_backend(store).await;
}

#[tokio::test]
async fn test_memory_etag_changes_on_mutations_get_all() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_etag_changes_on_mutations_get_all(store).await;
}

#[tokio::test]
async fn test_memory_etag_conditional_get_all() {
    let store = MemoryDiscoveryBackendStore::default();
    discovery::test_etag_conditional_get_all(store).await;
}

/*
assertion `left != right` failed: etag should change after patching backend
  left: 13741475936623907471
 right: 13741475936623907471
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- db_postgres::test_postgres_etag_changes_on_mutations_get_all stdout ----

thread 'db_postgres::test_postgres_etag_changes_on_mutations_get_all' (66230028) panicked at service/tests/discovery/../common/discovery.rs:316:5:
assertion `left != right` failed: etag should change after patching backend
  left: 11861561600054707174
 right: 11861561600054707174

---- db_sqlite::test_sqlite_etag_changes_on_mutations_get_all stdout ----

thread 'db_sqlite::test_sqlite_etag_changes_on_mutations_get_all' (66230220) panicked at service/tests/discovery/../common/discovery.rs:316:5:
assertion `left != right` failed: etag should change after patching backend
  left: 7753351019460804631
 right: 7753351019460804631
 */
