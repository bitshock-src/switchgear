use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Feature: Server handles secrets files
/// Scenario: Server startup succeeds with valid secrets file
#[tokio::test]
async fn test_server_startup_succeeds_with_valid_secrets_file() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let server1 = "server1";
    let config_path = manifest_dir.join("config/memory-basic.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // Use the good secrets file
    let secrets_path = manifest_dir.join("config/persistence-secrets.env");
    ctx.set_secrets_path(server1, secrets_path.into())
        .expect("assert");

    // Background steps
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, "cln")
        .await
        .expect("assert");
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_a_success_log_should_be_displayed(&mut ctx, "secrets loaded successfully")
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Server handles secrets files
/// Scenario: Server startup fails with missing secrets file
#[tokio::test]
async fn test_server_startup_fails_with_missing_secrets_file() {
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

    // Set a fake/non-existent secrets file path
    let fake_secrets_path = manifest_dir.join("config/nonexistent-secrets-file.env");
    ctx.set_secrets_path(server1, fake_secrets_path.into())
        .expect("assert");

    // Background steps
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_fail_to_start(&mut ctx)
        .await
        .expect("assert");
    step_then_an_error_message_should_be_displayed(&mut ctx, "error reading secrets file")
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_a_non_zero_code(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Server resumes state after restart
/// Scenario: Server startup fails with invalid secrets file
#[tokio::test]
async fn test_server_startup_fails_with_invalid_secrets_file() {
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

    // Create an invalid secrets file with random characters
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let invalid_secrets_path = temp_dir.path().join("invalid-secrets.env");
    fs::write(&invalid_secrets_path, "invalid@#$%content&*()")
        .expect("Failed to write invalid secrets file");

    ctx.set_secrets_path(server1, invalid_secrets_path.into())
        .expect("assert");

    // Background steps
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_fail_to_start(&mut ctx)
        .await
        .expect("assert");
    step_then_an_error_message_should_be_displayed(&mut ctx, "error parsing secrets file")
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_a_non_zero_code(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Server resumes state after restart
/// Scenario: Server startup fails with missing secret in file
#[tokio::test]
async fn test_server_startup_fails_with_missing_secret_in_file() {
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

    // Create a valid .env file but with missing required secrets
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let incomplete_secrets_path = temp_dir.path().join("incomplete-secrets.env");
    fs::write(
        &incomplete_secrets_path,
        "SOME_KEY=some_value\nANOTHER_KEY=another_value\n",
    )
    .expect("Failed to write incomplete secrets file");

    ctx.set_secrets_path(server1, incomplete_secrets_path.into())
        .expect("assert");

    // Background steps
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_fail_to_start(&mut ctx)
        .await
        .expect("assert");
    step_then_an_error_message_should_be_displayed(&mut ctx, "Invalid uri or missing secrets")
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_a_non_zero_code(&mut ctx)
        .await
        .expect("assert");
}
