use crate::common::context::cli::CliContext;
use crate::common::step_functions::*;

/// Feature: Service token CLI functionality
/// Scenario Outline: Generate ECDSA key pair for discovery service
#[tokio::test]
async fn test_token_key_generation_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_key(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_public_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_private_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Generate ECDSA key pair for offer service
#[tokio::test]
async fn test_token_key_generation_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_key(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_public_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_private_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint discovery service token with existing key
#[tokio::test]
async fn test_token_mint_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup: Generate key first
    step_given_a_valid_ecdsa_private_key_exists(&mut ctx, "discovery")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_a_valid_token_should_be_output_to_stdout(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint offer service token with existing key
#[tokio::test]
async fn test_token_mint_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup: Generate key first
    step_given_a_valid_ecdsa_private_key_exists(&mut ctx, "offer")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_a_valid_token_should_be_output_to_stdout(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint discovery service token with output file
#[tokio::test]
async fn test_token_mint_with_output_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_private_key_exists(&mut ctx, "discovery")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint_with_output(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_token_file_should_exist(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint offer service token with output file
#[tokio::test]
async fn test_token_mint_with_output_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_private_key_exists(&mut ctx, "offer")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint_with_output(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_token_file_should_exist(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint discovery service token with new key
#[tokio::test]
async fn test_token_mint_all_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint_all(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_public_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_private_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_a_valid_token_should_be_output_to_stdout(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint offer service token with new key
#[tokio::test]
async fn test_token_mint_all_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint_all(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_public_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_private_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_a_valid_token_should_be_output_to_stdout(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint discovery service token with new key and output file
#[tokio::test]
async fn test_token_mint_all_with_output_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint_all_with_output(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_public_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_private_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_token_file_should_exist(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Mint offer service token with new key and output file
#[tokio::test]
async fn test_token_mint_all_with_output_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_mint_all_with_output(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_public_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_private_key_file_should_exist(&mut ctx)
        .await
        .expect("assert");
    step_then_the_token_file_should_exist(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Verify discovery service token from stdin
#[tokio::test]
async fn test_token_verify_from_stdin_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_public_key_exists(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_given_a_valid_service_token_exists(&mut ctx, "discovery")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_verify_with_stdin(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_verification_output_should_be_valid(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Verify offer service token from stdin
#[tokio::test]
async fn test_token_verify_from_stdin_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_public_key_exists(&mut ctx, "offer")
        .await
        .expect("assert");
    step_given_a_valid_service_token_exists(&mut ctx, "offer")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_verify_with_stdin(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_verification_output_should_be_valid(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Verify discovery service token from file
#[tokio::test]
async fn test_token_verify_from_file_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_public_key_exists(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_given_a_valid_service_token_file_exists(&mut ctx, "discovery")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_verify_with_file(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_verification_output_should_be_valid(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Verify offer service token from file
#[tokio::test]
async fn test_token_verify_from_file_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_public_key_exists(&mut ctx, "offer")
        .await
        .expect("assert");
    step_given_a_valid_service_token_file_exists(&mut ctx, "offer")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_verify_with_file(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut ctx)
        .await
        .expect("assert");
    step_then_the_verification_output_should_be_valid(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Verify invalid discovery service token
#[tokio::test]
async fn test_token_verify_invalid_discovery() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_public_key_exists(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_given_an_invalid_service_token_exists(&mut ctx, "discovery")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_verify_with_stdin(&mut ctx, "discovery")
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut ctx)
        .await
        .expect("assert");
    step_then_an_error_message_should_be_shown(&mut ctx)
        .await
        .expect("assert");
}

/// Feature: Service token CLI functionality
/// Scenario Outline: Verify invalid offer service token
#[tokio::test]
async fn test_token_verify_invalid_offer() {
    let mut ctx = CliContext::create().expect("assert");

    // Background
    step_given_the_swgr_cli_is_available(&mut ctx)
        .await
        .expect("assert");

    // Setup
    step_given_a_valid_ecdsa_public_key_exists(&mut ctx, "offer")
        .await
        .expect("assert");
    step_given_an_invalid_service_token_exists(&mut ctx, "offer")
        .await
        .expect("assert");

    // Scenario steps
    step_when_i_run_swgr_service_token_verify_with_stdin(&mut ctx, "offer")
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut ctx)
        .await
        .expect("assert");
    step_then_an_error_message_should_be_shown(&mut ctx)
        .await
        .expect("assert");
}
