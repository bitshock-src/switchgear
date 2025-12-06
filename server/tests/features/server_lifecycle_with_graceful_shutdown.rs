/*!
 * Server Lifecycle with Graceful Shutdown Integration Tests
 *
 * These tests correspond to the Gherkin feature file but are implemented as
 * standalone Rust integration tests without cucumber framework dependencies.
 *
 * Feature file: server-lifecycle-with-graceful-shutdown.feature
 *
 * Each step function is clearly mapped to its Gherkin equivalent for easy
 * maintenance and understanding.
 */

use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::context::Service;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use anyhow::Result;
use std::path::PathBuf;
// =============================================================================
// INTEGRATION TESTS - Execute scenarios inline like cucumber would
// =============================================================================

/// Feature: Server starts and shuts down cleanly with signal
/// Scenario Outline: Start server and shutdown with sigterm
#[tokio::test]
async fn server_lifecycle_sigint_scenario() {
    signal_server(sysinfo::Signal::Interrupt)
        .await
        .expect("assert");
}

/// Feature: Server starts and shuts down cleanly with signal
/// Scenario Outline: Start server and shutdown with sigterm
#[tokio::test]
async fn server_lifecycle_sigterm_scenario() {
    signal_server(sysinfo::Signal::Term).await.expect("assert");
}

async fn signal_server(signal: sysinfo::Signal) -> Result<()> {
    let server1 = "server1";

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
    let config_path = manifest_dir.join("config/memory-basic.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )?;
    ctx.activate_server(server1);

    // Background steps
    step_given_the_server_is_not_already_running(&mut ctx).await?;

    // Scenario steps
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx).await?;
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx).await?;
    step_then_the_server_should_start_successfully(&mut ctx).await?;
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx).await?;
    step_when_i_send_a_signal_to_the_server_process(&mut ctx, signal).await?;
    step_then_the_server_should_stop_accepting_new_connections(&mut ctx, Service::LnUrl).await?;
    step_then_the_server_should_exit_with_code_0(&mut ctx).await?;
    step_then_no_error_logs_should_be_present(&mut ctx).await?;

    Ok(())
}
