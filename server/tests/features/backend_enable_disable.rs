use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::test]
async fn test_complete_backend_lifecycle_management() {
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

    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_given_the_payee_has_access_to_both_cln_and_lnd_lightning_nodes(&mut ctx)
        .await
        .expect("assert");

    // Start the server
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    // Complete the background setup
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");
    step_and_the_payee_has_created_an_offer_linked_to_both_lightning_nodes(&mut ctx)
        .await
        .expect("assert");
    // Register both backends individually
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "lnd", true)
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "cln", true)
        .await
        .expect("assert");

    // 1. Given the payer can generate invoices successfully
    step_given_the_payer_can_generate_invoices_successfully(&mut ctx)
        .await
        .expect("assert");

    // 2. When the admin disables the first backend
    step_when_the_admin_disables_the_first_backend(&mut ctx)
        .await
        .expect("assert");

    // 3. Then the payer can still get invoices
    step_then_the_payer_can_still_generate_invoices(&mut ctx)
        .await
        .expect("assert");

    // 4. When the admin disables the second backend
    step_when_the_admin_disables_the_second_backend(&mut ctx)
        .await
        .expect("assert");

    // 5. Then the payer cannot get invoices
    step_then_the_payer_cannot_generate_invoices(&mut ctx, Duration::from_secs(2))
        .await
        .expect("assert");

    // 6. When the admin enables any backend
    step_when_the_admin_enables_any_backend(&mut ctx)
        .await
        .expect("assert");

    // 7. Then the payer can again get invoices
    step_then_the_payer_can_again_generate_invoices(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}
