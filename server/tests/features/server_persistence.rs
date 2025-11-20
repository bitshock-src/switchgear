use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;
use switchgear_testing::credentials::RegTestLnNodeType;

#[tokio::test]
async fn test_complete_persistence_lifecycle_sqlite_sqlite() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };
    let server1 = "server1";
    let config_path = manifest_dir.join("config/sqlite-persistent.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // First server instance: Start server and create persistent data
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Setup specific backend and create data to persist
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Cln)
        .await
        .expect("assert");
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    // Shutdown first instance
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Second server instance: Verify data persistence
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    // Test that persisted offer and backend still work
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
        .await
        .expect("assert");

    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await
        .expect("assert");

    // Shutdown second instance
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
}

#[tokio::test]
async fn test_backend_data_loss_with_offer_persistence_sqlite_sqlite() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
        Some(ctx) => ctx,
        None => return,
    };
    let server1 = "server1";
    let config_path = manifest_dir.join("config/sqlite-persistent.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // Create and persist data
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Cln)
        .await
        .expect("assert");
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single")
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
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");

    // Delete only backend storage, keep offer storage
    step_when_i_delete_the_persistent_backend_storage_files(&mut ctx, true, false)
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
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    // Offer should exist but backend should be missing, causing invoice failure
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
        .await
        .expect("assert");
    step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(&mut ctx, "single", &Protocol::Https).await.expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");
}
