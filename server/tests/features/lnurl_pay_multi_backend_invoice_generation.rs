use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;
use switchgear_testing::credentials::lightning::RegTestLnNodeType;

#[tokio::test]
async fn test_no_backends_no_invoices_for_either_offer() {
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

    // Given steps - server setup
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_given_two_payees_each_have_their_own_lightning_node(&mut ctx)
        .await
        .expect("assert");

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

    // Given: Create offers but don't register any backends
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "first")
        .await
        .expect("assert");
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "second")
        .await
        .expect("assert");

    // But: No backends are registered
    // Wait longer for the system to recognize no backends are available
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    // When: Try to request invoices from both offers
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "first")
        .await
        .expect("assert");
    step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(
        &mut ctx, "first",
        &Protocol::Https,
    )
    .await.expect("assert");

    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "second")
        .await
        .expect("assert");
    step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(
        &mut ctx, "second",
        &Protocol::Https,
    )
    .await.expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_two_backends_both_offers_generate_invoices() {
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

    // Given steps - server setup
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    setup_two_payees_with_node_types(&mut ctx, RegTestLnNodeType::Lnd, RegTestLnNodeType::Cln)
        .await
        .expect("assert");

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

    // Given: Both payees create offers (LND for first, CLN for second)
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "first")
        .await
        .expect("assert");
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "second")
        .await
        .expect("assert");

    // And: Both nodes are registered as backends
    step_and_both_nodes_are_registered_as_separate_backends(&mut ctx)
        .await
        .expect("assert");

    // When: Request invoice from first payee's offer
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "first")
        .await
        .expect("assert");
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "first",
        &Protocol::Https,
    )
    .await
    .expect("assert");

    // Then: Should receive valid invoice
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "first")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "first")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "first")
        .await
        .expect("assert");

    // Clear received invoice for next test
    // Clear received invoices
    if let Some(offer_request) = ctx.get_offer_request_mut("first", "offer") {
        offer_request.received_invoice = None;
    }
    if let Some(offer_request) = ctx.get_offer_request_mut("second", "offer") {
        offer_request.received_invoice = None;
    }

    // When: Request invoice from second payee's offer
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "second")
        .await
        .expect("assert");
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "second",
        &Protocol::Https,
    )
    .await
    .expect("assert");

    // Then: Should receive valid invoice
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "second")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "second")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "second")
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}
