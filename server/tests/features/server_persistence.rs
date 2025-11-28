use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::cmp::PartialEq;
use std::path::PathBuf;
use switchgear_testing::credentials::db::{DbCredentials, TestDatabase};
use switchgear_testing::credentials::lightning::RegTestLnNodeType;
use switchgear_testing::db::{TestMysqlDatabase, TestPostgresDatabase};
use uuid::Uuid;

#[tokio::test]
async fn test_complete_persistence_lifecycle_sqlite() {
    test_complete_persistence_lifecycle_impl(DbType::Sqlite).await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_mysql() {
    test_complete_persistence_lifecycle_impl(DbType::Mysql {
        ssl: DbSslType::None,
    })
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_mysql_ssl() {
    test_complete_persistence_lifecycle_impl(DbType::Mysql {
        ssl: DbSslType::Parameter,
    })
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_mysql_ssl_native() {
    test_complete_persistence_lifecycle_impl(DbType::Mysql {
        ssl: DbSslType::Native,
    })
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_postgresql() {
    test_complete_persistence_lifecycle_impl(DbType::Postgresql {
        ssl: DbSslType::None,
    })
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_postgresql_ssl() {
    test_complete_persistence_lifecycle_impl(DbType::Postgresql {
        ssl: DbSslType::Parameter,
    })
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_postgresql_ssl_native() {
    test_complete_persistence_lifecycle_impl(DbType::Postgresql {
        ssl: DbSslType::Native,
    })
    .await;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbType {
    Sqlite,
    Mysql { ssl: DbSslType },
    Postgresql { ssl: DbSslType },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbSslType {
    None,
    Parameter,
    Native,
}

async fn test_complete_persistence_lifecycle_impl(db_type: DbType) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);

    let db_credentials = DbCredentials::create().expect("assert");
    let db = db_credentials.get_databases().expect("assert");

    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };
    let server1 = "server1";
    let config_path = manifest_dir.join("config/persistence.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");

    let _db_guard = match &db_type {
        DbType::Sqlite => (None, None),
        DbType::Mysql { ssl } => match &db.mysql {
            None => {
                eprintln!("MySQL database not available, skipping test");
                return;
            }
            Some(db) => (
                Some(install_mysql_databases(&mut ctx, server1, db, *ssl).expect("assert")),
                None,
            ),
        },
        DbType::Postgresql { ssl } => match &db.postgres {
            None => {
                eprintln!("PostgreSQL database not available, skipping test");
                return;
            }
            Some(db) => (
                None,
                Some(install_postgres_databases(&mut ctx, server1, db, *ssl).expect("assert")),
            ),
        },
    };

    ctx.activate_server(server1);

    // First server instance: Start server and create persistent data
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Setup specific backend and create data to persist
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Cln)
        .await
        .expect("assert");
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", true)
        .await
        .expect("assert");

    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    // Shutdown first instance
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Second server instance: Verify data persistence
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Test that persisted offer and backend still work
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    // Shutdown second instance
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
}

#[tokio::test]
async fn test_backend_data_loss_with_offer_persistence_sqlite_sqlite() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };
    let server1 = "server1";
    let config_path = manifest_dir.join("config/persistence.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // Create and persist data
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Cln)
        .await
        .expect("assert");
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", true)
        .await
        .expect("assert");
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");

    // Delete only backend storage, keep offer storage
    step_when_i_delete_the_persistent_backend_storage_files(&mut ctx, true, false)
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    // Offer should exist but backend should be missing, causing invoice failure
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(&mut ctx, "single", &Protocol::Https).await.expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
}

fn install_mysql_databases(
    ctx: &mut GlobalContext,
    server: &str,
    db: &TestDatabase,
    ssl: DbSslType,
) -> anyhow::Result<(TestMysqlDatabase, TestMysqlDatabase)> {
    if ssl == DbSslType::Native {
        ctx.set_certificate_location(
            server,
            CertificateLocation::NativePath(db.ca_cert_path.to_string_lossy().to_string()),
        )?;
    }
    let cert_path = if ssl == DbSslType::Parameter {
        Some(db.ca_cert_path.as_path())
    } else {
        None
    };

    let discovery_db = TestMysqlDatabase::new(
        format!("discovery_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    let offer_db = TestMysqlDatabase::new(
        format!("offer_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    ctx.set_discovery_store_database_url(server, discovery_db.connection_url().to_string())?;

    ctx.set_offer_store_database_url(server, offer_db.connection_url().to_string())?;

    Ok((discovery_db, offer_db))
}

fn install_postgres_databases(
    ctx: &mut GlobalContext,
    server: &str,
    db: &TestDatabase,
    ssl: DbSslType,
) -> anyhow::Result<(TestPostgresDatabase, TestPostgresDatabase)> {
    if ssl == DbSslType::Native {
        ctx.set_certificate_location(
            server,
            CertificateLocation::NativePath(db.ca_cert_path.to_string_lossy().to_string()),
        )?;
    }
    let cert_path = if ssl == DbSslType::Parameter {
        Some(db.ca_cert_path.as_path())
    } else {
        None
    };

    let discovery_db = TestPostgresDatabase::new(
        format!("discovery_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    let offer_db = TestPostgresDatabase::new(
        format!("offer_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    ctx.set_discovery_store_database_url(server, discovery_db.connection_url().to_string())?;

    ctx.set_offer_store_database_url(server, offer_db.connection_url().to_string())?;

    Ok((discovery_db, offer_db))
}
