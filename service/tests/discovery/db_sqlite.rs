use std::path::Path;
use switchgear_service::components::discovery::db::DbDiscoveryBackendStore;
use tempfile::TempDir;

use crate::common::discovery;

async fn create_sqlite_store(path: &Path) -> DbDiscoveryBackendStore {
    let path = path.join("db.sqlite");
    let store = DbDiscoveryBackendStore::connect(
        &format!("sqlite://{}?mode=rwc", path.to_string_lossy()),
        5,
    )
    .await
    .unwrap();
    store.migrate_up().await.unwrap();
    store
}

#[tokio::test]
async fn test_sqlite_post_new_backend_returns_address() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    discovery::test_post_new_backend_returns_address(store).await;
}

#[tokio::test]
async fn test_sqlite_get_returns_correct_backends() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    discovery::test_get_returns_correct_backends(store).await;
}

#[tokio::test]
async fn test_sqlite_delete_removes_and_returns_backends() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    discovery::test_delete_removes_target(store).await;
}

#[tokio::test]
async fn test_sqlite_put_new_backend_returns_true() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    discovery::test_put_new_backend_returns_true(store).await;
}

#[tokio::test]
async fn test_sqlite_put_existing_backend_updates_and_returns_false() {
    let t = TempDir::new().unwrap();
    let store = create_sqlite_store(t.path()).await;
    discovery::test_put_existing_backend_updates_and_returns_false(store).await;
}
