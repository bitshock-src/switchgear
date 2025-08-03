use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;
use switchgear_testing::credentials::RegTestLnNodeType;

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_cln_lightning_offer_using_http() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };

    let server1 = "server1";
    let config_path = manifest_dir.join("config/memory-basic-no-tls.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Http,
        Protocol::Http,
        Protocol::Http,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // When: Start the server
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");

    // Then: Verify server starts successfully
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Background: Verify LNURL server is running
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Given: Specific backend type
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Cln)
        .await
        .expect("assert");

    // When: Payee setup steps
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Payer requests offer using specified protocol
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");

    // Then: Verify offer properties
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Request invoice using specified protocol
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Http,
    )
    .await
    .expect("assert");

    // Then: Verify invoice
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_cln_lightning_offer_using_https() {
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

    // When: Start the server
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");

    // Then: Verify server starts successfully
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Background: Verify LNURL server is running
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Given: Specific backend type
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Cln)
        .await
        .expect("assert");

    // When: Payee setup steps
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Payer requests offer using specified protocol
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");

    // Then: Verify offer properties
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Request invoice using specified protocol
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");

    // Then: Verify invoice
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_lnd_lightning_offer_using_http() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };
    let server1 = "server1";
    let config_path = manifest_dir.join("config/memory-basic-no-tls.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Http,
        Protocol::Http,
        Protocol::Http,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // When: Start the server
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");

    // Then: Verify server starts successfully
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Background: Verify LNURL server is running
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Given: Specific backend type
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Lnd)
        .await
        .expect("assert");

    // When: Payee setup steps
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Payer requests offer using specified protocol
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");

    // Then: Verify offer properties
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Request invoice using specified protocol
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Http,
    )
    .await
    .expect("assert");

    // Then: Verify invoice
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_lnd_lightning_offer_using_https() {
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

    // When: Start the server
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");

    // Then: Verify server starts successfully
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Background: Verify LNURL server is running
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Given: Specific backend type
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Lnd)
        .await
        .expect("assert");

    // When: Payee setup steps
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Payer requests offer using specified protocol
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");

    // Then: Verify offer properties
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    // When: Request invoice using specified protocol
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");

    // Then: Verify invoice
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}
