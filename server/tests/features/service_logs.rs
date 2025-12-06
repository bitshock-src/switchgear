use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use rand::Rng;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use std::path::PathBuf;

#[tokio::test]
async fn test_service_health_check_logging() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, "cln")
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
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

    // Stop server and capture logs
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Assert logs after server stopped
    step_and_the_server_logs_should_contain_health_check_requests_for_all_services(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_service_operation_request_logging() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, "cln")
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
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

    // Stop server and capture logs
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Assert logs after server stopped
    step_and_the_server_logs_should_contain_backend_registration_requests(&mut ctx)
        .await
        .expect("assert");
    step_and_the_server_logs_should_contain_offer_retrieval_requests(&mut ctx)
        .await
        .expect("assert");
    step_and_the_server_logs_should_contain_invoice_generation_requests(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_error_conditions_are_properly_logged() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_when_i_request_an_offer_from_a_non_existent_partition(&mut ctx)
        .await
        .expect("assert");
    step_when_i_request_an_invoice_for_a_non_existent_offer(&mut ctx)
        .await
        .expect("assert");

    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();

    let missing_backend_private_key =
        SecretKey::from_byte_array(rng.gen::<[u8; 32]>()).expect("assert");
    let missing_backend_public_key =
        PublicKey::from_secret_key(&secp, &missing_backend_private_key);

    step_when_i_try_to_get_a_missing_backend(&mut ctx, &missing_backend_public_key)
        .await
        .expect("assert");

    // Stop server and capture logs
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Assert logs after server stopped
    step_and_the_server_logs_should_contain_404_error_responses(&mut ctx)
        .await
        .expect("assert");
    step_and_the_server_logs_should_contain_invalid_offer_error_responses(&mut ctx)
        .await
        .expect("assert");
    let invalid_backend_patterns = [
        "clf::discovery",
        &format!("GET /discovery/{}", missing_backend_public_key),
        "HTTP/1.1 404",
        " WARN ",
    ];
    step_and_the_server_logs_should_contain_invalid_backend_get_errors(
        &mut ctx,
        &invalid_backend_patterns,
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}
