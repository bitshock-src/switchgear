use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::context::Service;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;

#[tokio::test]
async fn test_start_all_service() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");

    // Scenario
    step_when_i_start_the_lnurl_server_with_enablement_flags(&mut ctx, &[])
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_offers_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
    step_then_no_error_logs_should_be_present(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_start_only_lnurl_service() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");

    // Scenario
    step_when_i_start_the_lnurl_server_with_enablement_flags(&mut ctx, &[Service::LnUrl])
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_discovery_service_should_not_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_offers_service_should_not_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
    step_then_no_error_logs_should_be_present(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_start_only_backend_discovery_service() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario specific configuration
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");

    // Scenario
    step_when_i_start_the_lnurl_server_with_enablement_flags(&mut ctx, &[Service::Discovery])
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_not_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_offers_service_should_not_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
    step_then_no_error_logs_should_be_present(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_start_only_offers_service() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario specific configuration
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");

    // Scenario
    step_when_i_start_the_lnurl_server_with_enablement_flags(&mut ctx, &[Service::Offer])
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_not_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_discovery_service_should_not_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_offers_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
    step_then_no_error_logs_should_be_present(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}
