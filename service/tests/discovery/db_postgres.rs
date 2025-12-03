use crate::common::discovery;
use anyhow::anyhow;
use switchgear_service::components::discovery::db::DbDiscoveryBackendStore;
use switchgear_testing::db::TestPostgresDatabase;
use switchgear_testing::services::IntegrationTestServices;
use uuid::Uuid;

async fn create_postgres_store() -> Option<(DbDiscoveryBackendStore, TestPostgresDatabase)> {
    let _ = rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

    let db_name = format!(
        "test_discovery_{}",
        Uuid::new_v4().to_string().replace("-", "")
    );
    let services = IntegrationTestServices::create().unwrap();

    let postgres = match services.postgres() {
        None => return None,
        Some(v) => v,
    };
    let db = TestPostgresDatabase::new(db_name, postgres, false, None);

    let store = DbDiscoveryBackendStore::connect(db.connection_url(), 5)
        .await
        .unwrap();

    store.migrate_up().await.unwrap();

    Some((store, db))
}

#[tokio::test]
async fn test_postgres_post_new_backend_returns_address() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_post_new_backend_returns_address(store).await;
}

#[tokio::test]
async fn test_postgres_get_returns_correct_backends() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_get_returns_correct_backends(store).await;
}

#[tokio::test]
async fn test_postgres_delete_removes_and_returns_backends() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_delete_removes_target(store).await;
}

#[tokio::test]
async fn test_postgres_put_new_backend_returns_true() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_put_new_backend_returns_true(store).await;
}

#[tokio::test]
async fn test_postgres_put_existing_backend_updates_and_returns_false() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_put_existing_backend_updates_and_returns_false(store).await;
}

#[tokio::test]
async fn test_postgres_test_patch_backend() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_patch_backend(store).await;
}

#[tokio::test]
async fn test_postgres_test_patch_missing_backend() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_patch_missing_backend(store).await;
}

#[tokio::test]
async fn test_postgres_etag_changes_on_mutations_get_all() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_etag_changes_on_mutations_get_all(store).await;
}

#[tokio::test]
async fn test_postgres_etag_conditional_get_all() {
    let (store, _guard) = match create_postgres_store().await {
        None => return,
        Some(v) => v,
    };
    discovery::test_etag_conditional_get_all(store).await;
}
