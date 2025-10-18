use crate::common::db::TestMysqlDatabase;
use crate::common::discovery;
use switchgear_service::components::discovery::db::DbDiscoveryBackendStore;
use switchgear_testing::services::IntegrationTestServices;
use uuid::Uuid;

async fn create_mysql_store() -> (DbDiscoveryBackendStore, TestMysqlDatabase) {
    let db_name = format!(
        "test_discovery_{}",
        Uuid::new_v4().to_string().replace("-", "")
    );
    let services = IntegrationTestServices::create().unwrap();
    eprintln!("services: {:?}", services);
    let db = TestMysqlDatabase::new(db_name, services.mysql());

    let store = DbDiscoveryBackendStore::connect(db.connection_url(), 5)
        .await
        .unwrap();

    store.migrate_up().await.unwrap();

    (store, db)
}

#[tokio::test]
async fn test_mysql_post_new_backend_returns_address() {
    let (store, _guard) = create_mysql_store().await;
    discovery::test_post_new_backend_returns_address(store).await;
}

#[tokio::test]
async fn test_mysql_get_returns_correct_backends() {
    let (store, _guard) = create_mysql_store().await;
    discovery::test_get_returns_correct_backends(store).await;
}

#[tokio::test]
async fn test_mysql_delete_removes_and_returns_backends() {
    let (store, _guard) = create_mysql_store().await;
    discovery::test_delete_removes_target(store).await;
}

#[tokio::test]
async fn test_mysql_put_new_backend_returns_true() {
    let (store, _guard) = create_mysql_store().await;
    discovery::test_put_new_backend_returns_true(store).await;
}

#[tokio::test]
async fn test_mysql_put_existing_backend_updates_and_returns_false() {
    let (store, _guard) = create_mysql_store().await;
    discovery::test_put_existing_backend_updates_and_returns_false(store).await;
}
