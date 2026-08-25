use crate::common::offer;
use anyhow::anyhow;
use switchgear_components::offer::db::DbOfferStore;
use switchgear_testing::db::TestPostgresDatabase;
use switchgear_testing::services::IntegrationTestServices;
use uuid::Uuid;

const CONNECT_TIMEOUT: f64 = 5.0;
const ACQUIRE_TIMEOUT: f64 = 10.0;

async fn create_postgres_store() -> (DbOfferStore, TestPostgresDatabase) {
    let _ = rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

    let db_name = format!("test_offer_{}", Uuid::new_v4().to_string().replace("-", ""));
    let services = IntegrationTestServices::new();

    let db = TestPostgresDatabase::new("postgres", &db_name, services.postgres(), false, None);

    let store = DbOfferStore::connect(
        db.connection_url(),
        Default::default(),
        5,
        CONNECT_TIMEOUT,
        ACQUIRE_TIMEOUT,
    )
    .await
    .unwrap();
    store.migrate_up().await.unwrap();
    (store, db)
}

#[tokio::test]
async fn test_postgres_get_nonexistent_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_postgres_post_new_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_new_offer(store).await;
}

#[tokio::test]
async fn test_postgres_post_existing_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_existing_offer(store).await;
}

#[tokio::test]
async fn test_postgres_put_new_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_new_offer(store).await;
}

#[tokio::test]
async fn test_postgres_put_existing_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_existing_offer(store).await;
}

#[tokio::test]
async fn test_postgres_delete_existing_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_existing_offer(store).await;
}

#[tokio::test]
async fn test_postgres_delete_nonexistent_offer() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_postgres_get_offers() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_offers(store).await;
}

#[tokio::test]
async fn test_postgres_get_nonexistent_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_post_new_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_post_existing_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_put_new_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_put_existing_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_delete_existing_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_delete_nonexistent_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_get_all_offer_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_get_all_offer_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_post_offer_with_missing_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_post_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_put_offer_with_missing_metadata() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_put_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_postgres_delete_metadata_with_referencing_offers() {
    let (store, _guard) = create_postgres_store().await;
    offer::test_delete_metadata_with_referencing_offers(store).await;
}

mod rotate_credentials {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;
    use switchgear_components::offer::db::DbOfferStore;
    use switchgear_components::secrets::{SecretStore, SecretStoreConfig, SecretStoreSecretConfig};
    use switchgear_service_api::offer::OfferStore;
    use switchgear_testing::db::TestPostgresDatabase;
    use switchgear_testing::services::IntegrationTestServices;
    use tempfile::TempDir;
    use uuid::Uuid;

    const CONNECT_TIMEOUT: f64 = 5.0;
    const ACQUIRE_TIMEOUT: f64 = 10.0;

    fn write_secret(dir: &TempDir, name: &str, contents: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[tokio::test]
    async fn rotate_credentials_loop_runs_and_disconnects_postgres() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let services = IntegrationTestServices::new();
        let db_name = format!("test_rotate_{}", Uuid::new_v4().simple());
        let _db_guard =
            TestPostgresDatabase::new("postgres", &db_name, services.postgres(), false, None);
        let after_at = _db_guard.connection_url().split_once('@').unwrap().1;

        let secret_dir = TempDir::new().unwrap();
        let user_path = write_secret(&secret_dir, "user", "postgres");
        let pw_path = write_secret(&secret_dir, "password", "postgres");

        let cfg = SecretStoreConfig {
            ttl: 0.3,
            secrets: BTreeMap::from([
                (
                    "PGUSER".into(),
                    SecretStoreSecretConfig {
                        path: user_path.clone(),
                    },
                ),
                (
                    "PGPASSWORD".into(),
                    SecretStoreSecretConfig {
                        path: pw_path.clone(),
                    },
                ),
            ]),
        };
        let secrets = SecretStore::create(&cfg);

        let template = format!("postgres://{{{{PGUSER}}}}:{{{{PGPASSWORD}}}}@{after_at}");
        let store = DbOfferStore::connect(
            &template,
            Some(&secrets),
            5,
            CONNECT_TIMEOUT,
            ACQUIRE_TIMEOUT,
        )
        .await
        .unwrap();
        store.migrate_up().await.unwrap();

        let _ = store.get_offers("default", 0, 1).await.unwrap();

        fs::write(&pw_path, "definitely-wrong").unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        fs::write(&pw_path, "postgres").unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = store.get_offers("default", 0, 1).await.unwrap();

        store.disconnect().await.unwrap();
        store.disconnect().await.unwrap();
    }
}
