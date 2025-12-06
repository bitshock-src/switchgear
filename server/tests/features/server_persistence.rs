use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::cmp::PartialEq;
use std::path::PathBuf;
use switchgear_testing::credentials::db::{DbCredentials, TestDatabase};
use switchgear_testing::db::{TestMysqlDatabase, TestPostgresDatabase};
use uuid::Uuid;

#[tokio::test]
async fn test_complete_persistence_lifecycle_sqlite() {
    test_complete_persistence_lifecycle_impl(DbType::Sqlite, DbUriType::Full).await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_mysql() {
    test_complete_persistence_lifecycle_impl(
        DbType::Mysql {
            ssl: DbSslType::None,
        },
        DbUriType::Full,
    )
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_mysql_ssl() {
    test_complete_persistence_lifecycle_impl(
        DbType::Mysql {
            ssl: DbSslType::Parameter,
        },
        DbUriType::Full,
    )
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_mysql_ssl_native() {
    test_complete_persistence_lifecycle_impl(
        DbType::Mysql {
            ssl: DbSslType::Native,
        },
        DbUriType::Full,
    )
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_postgresql() {
    test_complete_persistence_lifecycle_impl(
        DbType::Postgresql {
            ssl: DbSslType::None,
        },
        DbUriType::Full,
    )
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_postgresql_ssl() {
    test_complete_persistence_lifecycle_impl(
        DbType::Postgresql {
            ssl: DbSslType::Parameter,
        },
        DbUriType::Full,
    )
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_postgresql_ssl_native() {
    test_complete_persistence_lifecycle_impl(
        DbType::Postgresql {
            ssl: DbSslType::Native,
        },
        DbUriType::Full,
    )
    .await;
}

#[tokio::test]
async fn test_complete_persistence_lifecycle_with_secrets() {
    test_complete_persistence_lifecycle_impl(
        DbType::PostgresqlAndMysql,
        DbUriType::AddressNameWithSecrets,
    )
    .await;
}

#[tokio::test]
async fn test_backend_data_loss_with_offer_persistence_sqlite_sqlite() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
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

    step_given_the_payee_has_a_lightning_node_available(&mut ctx, "cln")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbType {
    Sqlite,
    Mysql { ssl: DbSslType },
    Postgresql { ssl: DbSslType },
    PostgresqlAndMysql,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbUriType {
    Full,
    AddressNameWithSecrets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbSslType {
    None,
    Parameter,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataStoreActivations {
    All,
    Discovery,
    Offer,
}

async fn test_complete_persistence_lifecycle_impl(db_type: DbType, db_uri_type: DbUriType) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);

    let db_credentials = DbCredentials::create().expect("assert");
    let db = db_credentials.get_databases().expect("assert");

    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
    let server1 = "server1";
    let config_path = match db_uri_type {
        DbUriType::Full => manifest_dir.join("config/persistence.yaml"),
        DbUriType::AddressNameWithSecrets => {
            manifest_dir.join("config/persistence-with-secrets.yaml")
        }
    };

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
        DbType::Mysql { ssl } => (
            Some(
                install_mysql_databases(
                    &mut ctx,
                    DataStoreActivations::All,
                    server1,
                    &db.mysql,
                    DbUriType::Full,
                    *ssl,
                )
                .expect("assert"),
            ),
            None,
        ),
        DbType::Postgresql { ssl } => (
            None,
            Some(
                install_postgres_databases(
                    &mut ctx,
                    DataStoreActivations::All,
                    server1,
                    &db.postgres,
                    DbUriType::Full,
                    *ssl,
                )
                .expect("assert"),
            ),
        ),

        DbType::PostgresqlAndMysql => (
            Some(
                install_mysql_databases(
                    &mut ctx,
                    DataStoreActivations::Offer,
                    server1,
                    &db.mysql,
                    DbUriType::AddressNameWithSecrets,
                    DbSslType::None,
                )
                .expect("assert"),
            ),
            Some(
                install_postgres_databases(
                    &mut ctx,
                    DataStoreActivations::Discovery,
                    server1,
                    &db.postgres,
                    DbUriType::AddressNameWithSecrets,
                    DbSslType::None,
                )
                .expect("assert"),
            ),
        ),
    };

    ctx.activate_server(server1);

    if db_uri_type == DbUriType::AddressNameWithSecrets {
        let secrets_path = manifest_dir.join("config/persistence-secrets.env");
        ctx.set_secrets_path(server1, secrets_path.into())
            .expect("assert");
    }

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
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, "cln")
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

fn install_mysql_databases(
    ctx: &mut GlobalContext,
    activations: DataStoreActivations,
    server: &str,
    db: &TestDatabase,
    db_uri_type: DbUriType,
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
        "root",
        &format!("discovery_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    let offer_db = TestMysqlDatabase::new(
        "root",
        &format!("offer_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    match db_uri_type {
        DbUriType::Full => {
            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Discovery
            {
                ctx.set_discovery_store_database_uri(
                    server,
                    discovery_db.connection_url().to_string(),
                )?;
            }

            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Offer
            {
                ctx.set_offer_store_database_uri(server, offer_db.connection_url().to_string())?;
            }
        }
        DbUriType::AddressNameWithSecrets => {
            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Discovery
            {
                ctx.set_discovery_store_database_uri(
                    server,
                    format!(
                        "{}/{}",
                        discovery_db.address(),
                        discovery_db.database_name()
                    ),
                )?;
            }

            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Offer
            {
                ctx.set_offer_store_database_uri(
                    server,
                    format!("{}/{}", offer_db.address(), offer_db.database_name()),
                )?;
            }
        }
    }

    Ok((discovery_db, offer_db))
}

fn install_postgres_databases(
    ctx: &mut GlobalContext,
    activations: DataStoreActivations,
    server: &str,
    db: &TestDatabase,
    db_uri_type: DbUriType,
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
        "postgres",
        &format!("discovery_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    let offer_db = TestPostgresDatabase::new(
        "postgres",
        &format!("offer_{}", Uuid::new_v4().to_string().replace("-", "")),
        &db.address,
        ssl != DbSslType::None,
        cert_path,
    );

    match db_uri_type {
        DbUriType::Full => {
            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Discovery
            {
                ctx.set_discovery_store_database_uri(
                    server,
                    discovery_db.connection_url().to_string(),
                )?;
            }

            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Offer
            {
                ctx.set_offer_store_database_uri(server, offer_db.connection_url().to_string())?;
            }
        }
        DbUriType::AddressNameWithSecrets => {
            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Discovery
            {
                ctx.set_discovery_store_database_uri(
                    server,
                    format!(
                        "{}/{}",
                        discovery_db.address(),
                        discovery_db.database_name()
                    ),
                )?;
            }

            if activations == DataStoreActivations::All
                || activations == DataStoreActivations::Offer
            {
                ctx.set_offer_store_database_uri(
                    server,
                    format!("{}/{}", offer_db.address(), offer_db.database_name()),
                )?;
            }
        }
    }

    Ok((discovery_db, offer_db))
}
