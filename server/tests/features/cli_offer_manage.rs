use crate::common::context::cli::CliContext;
use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;

use crate::common::context::server::CertificateLocation;

/// Feature: Offer CLI management
/// Scenario: Generate offer JSON
#[tokio::test]
async fn test_offer_new() {
    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_new(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_valid_offer_json_should_be_output_to_stdout(&mut cli_ctx)
        .await
        .expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Generate offer JSON with output file
#[tokio::test]
async fn test_offer_new_with_output() {
    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_new_with_output(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_json_file_should_exist(&mut cli_ctx)
        .await
        .expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Load a new offer
#[tokio::test]
async fn test_offer_post() {
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
    step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario Outline: Get an offer
#[tokio::test]
async fn test_offer_get() {
    // Test all three root certificate location methods
    let certificate_locations = vec![
        CertificateLocation::Arg,    // --trusted-roots CLI flag
        CertificateLocation::Env,    // DISCOVERY_STORE_HTTP_TRUSTED_ROOTS env var
        CertificateLocation::Native, // SSL_CERT_FILE env var
    ];

    for certificate_location in certificate_locations {
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

        // Setup - load offer
        step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
            .await
            .expect("assert");
        let expected_offer = extract_offer(&cli_ctx).await.expect("assert");
        step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, certificate_location.clone())
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");

        // Scenario steps
        step_when_i_run_swgr_offer_get(
            &mut ctx,
            &mut cli_ctx,
            &expected_offer.id,
            certificate_location,
        )
        .await
        .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");
        step_then_offer_details_should_be_output(&mut cli_ctx, &expected_offer)
            .await
            .expect("assert");

        ctx.stop_all_servers().expect("assert");
    }
}

/// Feature: Offer CLI management
/// Scenario Outline: Get all offers with parameters
#[tokio::test]
async fn test_offer_get_all() {
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

    let mut expected_offers = vec![];
    for _ in 0..10 {
        step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
            .await
            .expect("assert");

        let offer = extract_offer(&cli_ctx).await.expect("assert");
        expected_offers.push(offer);

        step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");

        // make sure timestamps are different
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let parameter_combinations = vec![
        (None, None),
        (Some(1), None),
        (None, Some(5)),
        (Some(1), Some(1)),
    ];

    for (start, count) in parameter_combinations {
        let start_index = start.unwrap_or(0);
        let end_index = count
            .map(|c| start_index + c)
            .unwrap_or(expected_offers.len());
        cli_ctx.reset();
        step_when_i_run_swgr_offer_get_all(&mut ctx, &mut cli_ctx, start, count)
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");

        step_then_all_offers_should_be_output(
            &mut cli_ctx,
            &expected_offers[start_index..end_index],
        )
        .await
        .expect("assert");
    }

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Get all offers with count exceeding limit
#[tokio::test]
async fn test_offer_get_all_bounds_error() {
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

    // Setup - load 10 offers
    for _ in 0..10 {
        step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
            .await
            .expect("assert");
        step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");
    }

    // Scenario steps
    step_when_i_run_swgr_offer_get_all(&mut ctx, &mut cli_ctx, None, Some(101))
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_a_user_error_message_should_be_shown(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Update an offer
#[tokio::test]
async fn test_offer_put() {
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

    // Setup - load offer
    step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let offer_id = extract_offer_id(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_and_updated_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_put(&mut ctx, &mut cli_ctx, &offer_id)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_get(&mut ctx, &mut cli_ctx, &offer_id, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_should_contain_the_updated_data(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Delete an offer
#[tokio::test]
async fn test_offer_delete() {
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

    // Setup - load offer
    step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let offer_id = extract_offer_id(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_delete(&mut ctx, &mut cli_ctx, &offer_id)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_get(&mut ctx, &mut cli_ctx, &offer_id, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Generate offer metadata JSON
#[tokio::test]
async fn test_offer_metadata_new() {
    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_metadata_new(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_valid_offer_metadata_json_should_be_output_to_stdout(&mut cli_ctx)
        .await
        .expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Generate offer metadata JSON with output file
#[tokio::test]
async fn test_offer_metadata_new_with_output() {
    let mut cli_ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_metadata_new_with_output(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_metadata_json_file_should_exist(&mut cli_ctx)
        .await
        .expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Load new offer metadata
#[tokio::test]
async fn test_offer_metadata_post() {
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
    step_given_a_valid_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_metadata_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario Outline: Get offer metadata
#[tokio::test]
async fn test_offer_metadata_get() {
    // Test all three root certificate location methods
    let certificate_locations = vec![
        CertificateLocation::Arg,    // --trusted-roots CLI flag
        CertificateLocation::Env,    // DISCOVERY_STORE_HTTP_TRUSTED_ROOTS env var
        CertificateLocation::Native, // SSL_CERT_FILE env var
    ];

    for certificate_location in certificate_locations {
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

        // Setup - load metadata
        step_given_a_valid_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
            .await
            .expect("assert");
        let expected_metadata = extract_metadata(&cli_ctx).await.expect("assert");
        step_when_i_run_swgr_offer_metadata_post(
            &mut ctx,
            &mut cli_ctx,
            certificate_location.clone(),
        )
        .await
        .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");

        // Scenario steps
        step_when_i_run_swgr_offer_metadata_get(
            &mut ctx,
            &mut cli_ctx,
            &expected_metadata.id,
            certificate_location,
        )
        .await
        .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");
        step_then_offer_metadata_details_should_be_output(&mut cli_ctx, &expected_metadata)
            .await
            .expect("assert");

        ctx.stop_all_servers().expect("assert");
    }
}

/// Feature: Offer CLI management
/// Scenario Outline: Get all offer metadata with parameters
#[tokio::test]
async fn test_offer_metadata_get_all() {
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

    // Setup - load 10 metadata
    let mut expected_metadata = vec![];
    for _ in 0..10 {
        step_given_a_valid_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
            .await
            .expect("assert");

        let metadata = extract_metadata(&cli_ctx).await.expect("assert");
        expected_metadata.push(metadata);

        step_when_i_run_swgr_offer_metadata_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");

        // make sure timestamps are different
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let parameter_combinations = vec![
        (None, None),
        (Some(1), None),
        (None, Some(5)),
        (Some(1), Some(1)),
    ];

    for (start, count) in parameter_combinations {
        let start_index = start.unwrap_or(0);
        let end_index = count
            .map(|c| start_index + c)
            .unwrap_or(expected_metadata.len());
        cli_ctx.reset();
        step_when_i_run_swgr_offer_metadata_get_all(&mut ctx, &mut cli_ctx, start, count)
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");

        step_then_all_offer_metadata_should_be_output(
            &mut cli_ctx,
            &expected_metadata[start_index..end_index],
        )
        .await
        .expect("assert");
    }

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Get all offer metadata with count exceeding limit
#[tokio::test]
async fn test_offer_metadata_get_all_bounds_error() {
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

    // Setup - load 10 metadata
    for _ in 0..10 {
        step_given_a_valid_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
            .await
            .expect("assert");
        step_when_i_run_swgr_offer_metadata_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
            .await
            .expect("assert");
        step_then_the_command_should_succeed(&mut cli_ctx)
            .await
            .expect("assert");
    }

    // Scenario steps
    step_when_i_run_swgr_offer_metadata_get_all(&mut ctx, &mut cli_ctx, None, Some(101))
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_a_user_error_message_should_be_shown(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Update offer metadata
#[tokio::test]
async fn test_offer_metadata_put() {
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

    // Setup - load metadata
    step_given_a_valid_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let metadata_id = extract_metadata_id(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_offer_metadata_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_and_updated_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_metadata_put(&mut ctx, &mut cli_ctx, &metadata_id)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_metadata_get(
        &mut ctx,
        &mut cli_ctx,
        &metadata_id,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_metadata_should_contain_the_updated_data(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Delete offer metadata
#[tokio::test]
async fn test_offer_metadata_delete() {
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

    // Setup - load metadata
    step_given_a_valid_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let metadata_id = extract_metadata_id(&cli_ctx).await.expect("assert");
    step_when_i_run_swgr_offer_metadata_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_offer_metadata_delete(&mut ctx, &mut cli_ctx, &metadata_id)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_metadata_get(
        &mut ctx,
        &mut cli_ctx,
        &metadata_id,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_metadata_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Attempt to post offer with non-existent metadata
#[tokio::test]
async fn test_offer_post_invalid_metadata() {
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
    step_given_an_offer_json_with_non_existent_metadata_id_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_a_user_error_message_should_be_shown(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Attempt to delete metadata referenced by offer
#[tokio::test]
async fn test_offer_metadata_delete_referenced() {
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
    step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let metadata_id = extract_metadata_id_from_offer(&cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_metadata_delete(&mut ctx, &mut cli_ctx, &metadata_id)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_a_user_error_message_should_be_shown(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Get a non-existent offer returns error
#[tokio::test]
async fn test_offer_get_error() {
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
    step_when_i_run_swgr_offer_get_for_non_existent_offer(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Delete a non-existent offer returns error
#[tokio::test]
async fn test_offer_delete_error() {
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
    step_when_i_run_swgr_offer_delete_for_non_existent_offer(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Post a duplicate offer returns conflict error
#[tokio::test]
async fn test_offer_post_conflict() {
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

    // Setup - load offer
    step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps - post same offer again
    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
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

/// Feature: Offer CLI management
/// Scenario: Get a non-existent offer metadata returns error
#[tokio::test]
async fn test_offer_metadata_get_error() {
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
    step_when_i_run_swgr_offer_metadata_get_for_non_existent_metadata(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_metadata_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Delete a non-existent offer metadata returns error
#[tokio::test]
async fn test_offer_metadata_delete_error() {
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
    step_when_i_run_swgr_offer_metadata_delete_for_non_existent_metadata(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_offer_metadata_should_not_be_found(&mut cli_ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Feature: Offer CLI management
/// Scenario: Post a duplicate offer metadata returns conflict error
#[tokio::test]
async fn test_offer_metadata_post_conflict() {
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

    // Setup - load metadata
    step_given_a_valid_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_metadata_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // Scenario steps - post same metadata again
    step_when_i_run_swgr_offer_metadata_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
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
