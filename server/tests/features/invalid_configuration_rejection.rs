/*!
 * Invalid Configuration Rejection Integration Tests
 *
 * These tests correspond to the Gherkin feature file but are implemented as
 * standalone Rust integration tests without cucumber framework dependencies.
 *
 * Feature file: invalid-configuration-rejection.feature
 *
 * Each step function is clearly mapped to its Gherkin equivalent for easy
 * maintenance and understanding.
 */

use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;
// =============================================================================
// INTEGRATION TESTS - Execute scenarios inline like cucumber would
// =============================================================================

/// Feature: Configuration validation
/// Scenario: Invalid configuration file is rejected
#[tokio::test]
async fn test_configuration_validation_invalid_scenario() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
    let server1 = "server1";
    let config_path = manifest_dir.join("tests/features/common/config/invalid-config.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // Background steps
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario steps
    step_given_an_invalid_configuration_file_exists(&mut ctx)
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_fail_to_start(&mut ctx)
        .await
        .expect("assert");
    step_then_an_error_message_should_be_displayed(&mut ctx, "parsing YAML configuration")
        .await
        .expect("assert");
    step_then_the_error_message_should_contain_configuration_parsing_details(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_a_non_zero_code(&mut ctx)
        .await
        .expect("assert");
}
