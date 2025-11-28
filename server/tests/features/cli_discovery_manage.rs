use crate::common::context::cli::CliContext;
use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;

use crate::common::context::server::CertificateLocation;

/// Feature: Discovery CLI management
/// Scenario Outline: Generate cln-grpc backend JSON
#[tokio::test]
async fn test_discovery_new_cln_grpc() {
    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_new(&mut cli_ctx, "cln-grpc")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_valid_backend_json_should_be_output_to_stdout(&mut cli_ctx)
        .await
        .expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario Outline: Generate lnd-grpc backend JSON
#[tokio::test]
async fn test_discovery_new_lnd_grpc() {
    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_new(&mut cli_ctx, "lnd-grpc")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_valid_backend_json_should_be_output_to_stdout(&mut cli_ctx)
        .await
        .expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Generate backend JSON with output file
#[tokio::test]
async fn test_discovery_new_with_output() {
    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_new_with_output(&mut cli_ctx, "lnd-grpc")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    let expected_backend = extract_backend(&cli_ctx).await.expect("assert");
    step_then_the_backend_json_file_should_exist(&mut cli_ctx, &expected_backend)
        .await
        .expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Load a new backend
#[tokio::test]
async fn test_discovery_post() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Scenario steps
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Post a duplicate backend returns conflict error
#[tokio::test]
async fn test_discovery_post_conflict() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps - post same backend again
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_a_conflict_message_should_be_shown(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: List all backends
#[tokio::test]
async fn test_discovery_ls() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let expected_backend = extract_backend(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_ls(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_backend_list_should_be_output(&mut cli_ctx, &[expected_backend])
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario Outline: Get a backend
#[tokio::test]
async fn test_discovery_get() {
    let root_locations = vec![
        CertificateLocation::Arg,
        CertificateLocation::Env,
        CertificateLocation::Native,
    ];

    for root_location in root_locations {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
        let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
            Some(ctx) => ctx,
            None => return,
        };

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

        let mut cli_ctx = CliContext::create().expect("assert");

        // Background
        step_given_the_swgr_cli_is_available(&mut cli_ctx)
            .await
            .expect("assert");

        // Start server
        step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

        // Setup - load backend
        step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
            .await
            .expect("assert");
        let expected_backend = extract_backend(&cli_ctx).await.expect("assert");
        step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, root_location.clone())
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");

        // Scenario steps
        step_when_i_run_swgr_discovery_get(
            &mut ctx,
            &mut cli_ctx,
            &expected_backend.address.encoded(),
            root_location,
        )
        .await
        .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");
        step_then_backend_details_should_be_output(&mut cli_ctx, &expected_backend)
            .await
            .expect("assert");

        ctx.stop_all_servers().expect("assert");
    }
}

/// Feature: Discovery CLI management
/// Scenario: Get all backends
#[tokio::test]
async fn test_discovery_get_all() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let expected_backend = extract_backend(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_get_all(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_all_backends_should_be_output(&mut cli_ctx, &expected_backend)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Update a backend
#[tokio::test]
async fn test_discovery_put() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let backend_address = extract_backend_address(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_and_updated_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_put(&mut ctx, &mut cli_ctx, &backend_address)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_get(
        &mut ctx,
        &mut cli_ctx,
        &backend_address,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_contain_the_updated_data(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Delete a backend
#[tokio::test]
async fn test_discovery_delete() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let backend_address = extract_backend_address(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_delete(&mut ctx, &mut cli_ctx, &backend_address)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_get(
        &mut ctx,
        &mut cli_ctx,
        &backend_address,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Patch a backend
#[tokio::test]
async fn test_discovery_patch() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let backend_address = extract_backend_address(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_and_backend_patch_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_patch(&mut ctx, &mut cli_ctx, &backend_address)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_get(
        &mut ctx,
        &mut cli_ctx,
        &backend_address,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_contain_the_patched_data(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Enable a backend
#[tokio::test]
async fn test_discovery_enable() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let backend_address = extract_backend_address(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps - disable first then enable
    step_when_i_run_swgr_discovery_disable(&mut ctx, &mut cli_ctx, &backend_address)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_enable(&mut ctx, &mut cli_ctx, &backend_address)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_get(
        &mut ctx,
        &mut cli_ctx,
        &backend_address,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_be_enabled(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Disable a backend
#[tokio::test]
async fn test_discovery_disable() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - load backend
    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let backend_address = extract_backend_address(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_disable(&mut ctx, &mut cli_ctx, &backend_address)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_get(
        &mut ctx,
        &mut cli_ctx,
        &backend_address,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_be_disabled(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Get a non-existent backend returns error
#[tokio::test]
async fn test_discovery_get_error() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Scenario steps
    step_when_i_run_swgr_discovery_get_for_non_existent_backend(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Patch a non-existent backend returns error
#[tokio::test]
async fn test_discovery_patch_error() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Setup - create patch JSON
    step_and_backend_patch_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_discovery_patch_for_non_existent_backend(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Enable a non-existent backend returns error
#[tokio::test]
async fn test_discovery_enable_error() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Scenario steps
    step_when_i_run_swgr_discovery_enable_for_non_existent_backend(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Disable a non-existent backend returns error
#[tokio::test]
async fn test_discovery_disable_error() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Scenario steps
    step_when_i_run_swgr_discovery_disable_for_non_existent_backend(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Discovery CLI management
/// Scenario: Delete a non-existent backend returns error
#[tokio::test]
async fn test_discovery_delete_error() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

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

    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Start server
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
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

    // Scenario steps
    step_when_i_run_swgr_discovery_delete_for_non_existent_backend(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_backend_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}
