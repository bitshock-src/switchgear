use crate::FEATURE_TEST_CONFIG_PATH;
use crate::common::context::global::GlobalContext;
use crate::common::context::{Protocol, Service};
use crate::common::step_functions::*;
use std::path::PathBuf;

/// Feature: Server handles secrets files
/// Scenario: Server startup fails when a per-store value secret file is missing on disk
///
/// Only the offer service is enabled so its `SecretStore::create` failure
/// surfaces without the discovery store attempting a database connection
/// against a non-existent host.
#[tokio::test]
async fn test_server_startup_fails_with_missing_value_secret_file() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let server1 = "server1";
    let config_path = manifest_dir.join("config/persistence-with-secrets.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    ctx.set_mysql_username_file(
        server1,
        Some(manifest_dir.join("config/persistence-offer-mysql-username")),
    )
    .expect("assert");
    ctx.set_mysql_password_file(
        server1,
        Some(manifest_dir.join("config/nonexistent-mysql-password")),
    )
    .expect("assert");

    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    step_when_i_start_the_lnurl_server_with_enablement_flags(&mut ctx, &[Service::Offer])
        .await
        .expect("assert");
    step_then_the_server_should_fail_to_start(&mut ctx)
        .await
        .expect("assert");
    step_then_an_error_message_should_be_displayed(&mut ctx, "opening secret file")
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_a_non_zero_code(&mut ctx)
        .await
        .expect("assert");
}
