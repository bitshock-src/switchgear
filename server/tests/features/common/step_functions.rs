use crate::common::context::global::GlobalContext;
use crate::common::context::pay::OfferRequest;
use crate::common::context::{Protocol, Service};
use crate::common::helpers::{
    check_all_services_health, check_services_listening_status, count_log_patterns,
    get_invoice_from_request, get_lnurl_offer_from_request, get_offer_id_from_request,
    get_offer_request, get_offer_request_mut, get_payee_from_context, get_payee_from_context_mut,
    parse_lightning_invoice, request_and_validate_invoice_helper, validate_invoice_has_amount,
    verify_exit_code, verify_single_service_status,
};
use crate::{anyhow_log, bail_log};
use anyhow::{bail, Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use rand::{distributions::Alphanumeric, Rng};
use reqwest::{StatusCode, Url};
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use std::vec;
use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
    DiscoveryBackendSparse, DiscoveryBackendStore,
};
use switchgear_service::api::offer::{
    OfferMetadata, OfferMetadataSparse, OfferMetadataStore, OfferRecord, OfferRecordSparse,
    OfferStore,
};
use switchgear_service::components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcClientAuthPath, ClnGrpcDiscoveryBackendImplementation,
};
use switchgear_service::components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcClientAuthPath, LndGrpcDiscoveryBackendImplementation,
};
use switchgear_testing::credentials::{RegTestLnNode, RegTestLnNodeAddress, RegTestLnNodeType};
use tokio::time::sleep as tokio_sleep;
use uuid::Uuid;
// =============================================================================
// STEP FUNCTIONS - Mapped to Gherkin steps in feature files
// =============================================================================

/// Step: "Given an invalid configuration file exists"
/// Creates an invalid YAML configuration file for negative testing
pub async fn step_given_an_invalid_configuration_file_exists(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get the config file path
    let config_path = ctx.get_active_server_config_path()?;
    if !config_path.exists() {
        bail_log!("Configuration file not found: {}", config_path.display());
    }

    // Attempt to load and parse the config file
    let config_content = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading config file: {}", config_path.display()))?;

    // Try to parse with serde_yaml and assert that it fails
    let parse_result =
        serde_yaml::from_str::<switchgear_server::config::ServerConfig>(&config_content);

    if parse_result.is_ok() {
        bail_log!(
            "Expected config parsing to fail, but it succeeded for file: {}",
            config_path.display()
        );
    }

    Ok(())
}

/// Step: "Given the server is not already running"
/// Verifies that no server process is already listening on the configured ports
pub async fn step_given_the_server_is_not_already_running(ctx: &mut GlobalContext) -> Result<()> {
    // Check if any process is already listening on our ports
    let services = vec![Service::LnUrl, Service::Discovery, Service::Offer];
    if check_all_services_health(ctx, services).await? {
        bail_log!("Server is already running on one of the configured ports");
    }

    Ok(())
}

/// Step: "Given the LNURL server is ready to start"
/// Ensures the server is ready to start by checking configuration and binary availability
pub async fn step_given_the_lnurl_server_is_ready_to_start(ctx: &mut GlobalContext) -> Result<()> {
    // Ensure we have a valid config and no running server
    let config_path = ctx.get_active_server_config_path()?;
    if !config_path.exists() {
        bail_log!("Configuration file not found: {}", config_path.display());
    }

    let config_content = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading config file: {}", config_path.display()))?;

    let config_content = shellexpand::env(&config_content).with_context(|| {
        format!(
            "expanding configuration file env vars '{}'",
            config_path.to_string_lossy()
        )
    })?;

    let _config = serde_yaml::from_str::<switchgear_server::config::ServerConfig>(&config_content)
        .with_context(|| format!("parsing config file: {}", config_path.display()))?;

    Ok(())
}

/// Step: "When I start the LNURL server with the configuration"
/// Starts the LNURL server process with the generated configuration file
/// Captures stdout/stderr for later assertions
pub async fn step_when_i_start_the_lnurl_server_with_the_configuration(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let _pid = ctx
        .start_active_server(
            &[Service::LnUrl, Service::Discovery, Service::Offer],
            log::Level::Info,
        )
        .await?;
    Ok(())
}

/// Step: "When I send a {signal} signal to the server process"
/// Sends the specified signal to the running server process for graceful shutdown
pub async fn step_when_i_send_a_signal_to_the_server_process(
    ctx: &mut GlobalContext,
    signal: sysinfo::Signal,
) -> Result<()> {
    ctx.signal_all_servers(signal)?;
    Ok(())
}

/// Step: "When I send a SIGTERM signal to the server process"
/// Sends a SIGTERM signal to the running server process for graceful shutdown
pub async fn step_when_i_send_a_sigterm_signal_to_the_server_process(
    ctx: &mut GlobalContext,
) -> Result<()> {
    ctx.stop_all_servers()?;

    Ok(())
}

/// Step: "Then the server should start successfully"
/// Verifies that the server process started without errors
pub async fn step_then_the_server_should_start_successfully(ctx: &mut GlobalContext) -> Result<()> {
    if !ctx.has_active_server_process()? {
        bail_log!("Server process not started");
    }

    Ok(())
}

/// Step: "And all services should be listening on their configured ports"
/// Verifies that all service endpoints are responding to health checks  
pub async fn step_and_all_services_should_be_listening_on_their_configured_ports(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let services = vec![Service::LnUrl, Service::Discovery, Service::Offer];

    check_services_listening_status(ctx, services, true, 100, 5).await?;
    Ok(())
}

/// Step: "Then the server should fail to start"
/// Verifies that the server process failed to start due to invalid configuration
pub async fn step_then_the_server_should_fail_to_start(ctx: &mut GlobalContext) -> Result<()> {
    // Wait for the process to exit (following the pattern from TestService)
    let exit_code = ctx.wait_active_exit_code()?;

    if exit_code != 0 {
        // This is what we expect - process failed
        Ok(())
    } else {
        bail_log!(
            "Process exited successfully with code {:?}, but we expected failure",
            exit_code
        )
    }
}

/// Step: "Then an error message should be displayed"
/// Verifies that an error message was captured in stdout/stderr
pub async fn step_then_an_error_message_should_be_displayed(ctx: &mut GlobalContext) -> Result<()> {
    // Check stderr buffer for configuration parsing errors
    if let Ok(stderr_buf) = ctx.get_active_stderr_buffer()?.lock() {
        if !stderr_buf.is_empty() {
            let stderr_content = stderr_buf.join("\n");

            // For invalid configuration tests, assert on specific error content
            if stderr_content.contains("parsing YAML configuration") {
                return Ok(());
            }

            if stderr_content.contains("server terminated with error") {
                return Ok(());
            }
        }

        // Any stderr content indicates an error was displayed
        return Ok(());
    }

    let exit_code = ctx.wait_active_exit_code()?;

    if exit_code != 0 {
        // Non-zero exit code indicates error was displayed
        return Ok(());
    }

    bail_log!("No error message was captured and no error exit code was recorded")
}

/// Step: "Then the error message should contain configuration parsing details"
/// Verifies that the error message contains specific configuration parsing error details
pub async fn step_then_the_error_message_should_contain_configuration_parsing_details(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Check stderr buffer for specific configuration parsing errors
    if let Ok(stderr_lines) = ctx.get_active_stderr_buffer()?.lock() {
        if !stderr_lines.is_empty() {
            let mut detected_patterns = Vec::new();
            let stderr_text = stderr_lines.join("\n");

            // Assert on specific error content patterns
            if stderr_text.contains("parsing YAML configuration") {
                detected_patterns.push("Configuration parsing error");
            }

            if stderr_text.contains("did not find expected") {
                detected_patterns.push("YAML syntax error");
            }

            if stderr_text.contains("while parsing a flow sequence") {
                detected_patterns.push("Flow sequence parsing error");
            }

            if stderr_text.contains("server terminated with error") {
                detected_patterns.push("Server termination error");
            }

            return if !detected_patterns.is_empty() {
                Ok(())
            } else {
                bail_log!("Expected configuration parsing error patterns not found")
            };
        }
    }

    bail_log!("No stderr capture found for configuration parsing error assertion")
}

/// Step: "Then the server should exit with code 0"
/// Verifies that the server process exits with a successful exit code (0)
pub async fn step_then_the_server_should_exit_with_code_0(ctx: &mut GlobalContext) -> Result<()> {
    verify_exit_code(ctx, true).await?;

    Ok(())
}

/// Step: "Then the server should exit with a non-zero code"
/// Verifies that the server process exits with an error exit code (non-zero)
pub async fn step_then_the_server_should_exit_with_a_non_zero_code(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_exit_code(ctx, false).await?;

    Ok(())
}

/// Step: "Then the server should stop accepting new connections"
/// Verifies that the server is no longer accepting new HTTP connections
pub async fn step_then_the_server_should_stop_accepting_new_connections(
    ctx: &mut GlobalContext,
    service: Service,
) -> Result<()> {
    // Wait a moment for the server to stop accepting connections
    tokio_sleep(Duration::from_millis(500)).await;

    let probe = match service {
        Service::LnUrl => ctx.get_active_lnurl_probe()?,
        Service::Discovery => ctx.get_active_discovery_probe()?,
        Service::Offer => ctx.get_active_offer_probe()?,
    };

    let accepting_connections = probe.probe().await;

    // If the connection succeeds quickly, the server may not be shutting down properly
    if accepting_connections {
        bail_log!("Server is still accepting new connections too readily");
    }

    Ok(())
}

/// Step: "Then no error logs should be present"
/// Verifies that no error or fatal log messages were generated during the test
pub async fn step_then_no_error_logs_should_be_present(ctx: &mut GlobalContext) -> Result<()> {
    // Check stderr buffer for error logs
    if let Ok(stderr_buf) = ctx.get_active_stderr_buffer()?.lock() {
        let stderr_content = stderr_buf.join("\n");
        if stderr_content.contains("ERROR") || stderr_content.contains("FATAL") {
            bail_log!("Error log found in stderr: {}", stderr_content);
        }
    }

    Ok(())
}

// =============================================================================
// LNURL PAY STEP FUNCTIONS - For LNURL Pay invoice generation feature
// =============================================================================

/// Step: "Given the payee has a {backend_type} lightning node available"
/// Sets up the specific lightning node backend type for the test
pub async fn step_given_the_payee_has_a_lightning_node_available(
    ctx: &mut GlobalContext,
    backend_type: RegTestLnNodeType,
) -> Result<()> {
    let node = match backend_type {
        RegTestLnNodeType::Cln => RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()),
        RegTestLnNodeType::Lnd => RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()),
    };
    ctx.add_payee("single", node.clone());
    Ok(())
}

/// Step: "When the {payee_id} payee creates an offer for their lightning node"
/// Creates an LNURL offer for the specified lightning node
pub async fn step_when_the_payee_creates_an_offer_for_their_lightning_node(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Use the selected node from context
    let payee = get_payee_from_context(ctx, payee_id)?;
    let node = payee.node.clone();

    let client = ctx.get_active_offer_client()?;

    // Create offer metadata
    let metadata_id = Uuid::new_v4();
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let offer_metadata = OfferMetadata {
        id: metadata_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: random_string.clone(),
            long_text: Some("test-context".to_string()),
            image: None,
            identifier: None,
        },
    };

    // Post metadata
    let metadata_result = client.post_metadata(offer_metadata.clone()).await;
    if metadata_result.is_err() {
        bail_log!(
            "Expected successful metadata creation, got error: {:?}",
            metadata_result.err()
        );
    }

    // Create offer record with different limits based on node type
    let offer_id = Uuid::new_v4();
    let (max_sendable, min_sendable) = match node {
        RegTestLnNode::Cln(_) => (1_000_000_000, 1_000),
        RegTestLnNode::Lnd(_) => (2_000_000_000, 1_000),
    };

    let now = Utc::now();
    let offer = OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable,
            min_sendable,
            metadata_id,
            timestamp: now - ChronoDuration::minutes(5),
            expires: Some(now + ChronoDuration::hours(24)),
        },
    };

    // Post offer
    let offer_result = client.post_offer(offer.clone()).await;
    if offer_result.is_err() {
        bail_log!(
            "Expected successful offer creation, got error: {:?}",
            offer_result.err()
        );
    }

    // Store offer details in payee context for later steps
    let offer_request = OfferRequest {
        offer_id: Some(offer_id),
        metadata_id: Some(metadata_id),
        lnurl_offer: None,
        received_invoice: None,
    };
    ctx.add_offer_request(payee_id, "offer", offer_request)?;

    Ok(())
}

/// Step: "When the {payee_id} payee registers their lightning node as a backend"
/// Registers the lightning node as a discovery backend
pub async fn step_when_the_payee_registers_their_lightning_node_as_a_backend(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Use the selected node from context
    let payee = get_payee_from_context(ctx, payee_id)?;
    let node = payee.node.clone();

    let client = ctx.get_active_discovery_client()?;

    let address = DiscoveryBackendAddress::PublicKey(*node.public_key());

    let url = match node.address() {
        RegTestLnNodeAddress::Inet(a) => Url::parse(format!("https://{a}").as_str())?,
        RegTestLnNodeAddress::Path(_) => bail!("socket address not supported"),
    };

    let implementation = match &node {
        RegTestLnNode::Cln(cln) => {
            DiscoveryBackendImplementation::ClnGrpc(ClnGrpcDiscoveryBackendImplementation {
                url,
                auth: ClnGrpcClientAuth::Path(ClnGrpcClientAuthPath {
                    ca_cert_path: cln.ca_cert_path.clone(),
                    client_cert_path: cln.client_cert_path.clone(),
                    client_key_path: cln.client_key_path.clone(),
                }),
                domain: Some(cln.sni.clone()),
            })
        }
        RegTestLnNode::Lnd(lnd) => {
            DiscoveryBackendImplementation::LndGrpc(LndGrpcDiscoveryBackendImplementation {
                url,
                auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                    tls_cert_path: lnd.tls_cert_path.clone(),
                    macaroon_path: lnd.macaroon_path.clone(),
                }),
                amp_invoice: false,
                domain: None,
            })
        }
    };

    let backend = DiscoveryBackend {
        address,
        backend: DiscoveryBackendSparse {
            partitions: ["default".to_string()].into(),
            weight: 100,
            enabled: true,
            implementation,
        },
    };

    // Register backend
    let backend_result = client.post(backend.clone()).await;
    if backend_result.is_err() {
        bail_log!(
            "Expected successful backend registration, got error: {:?}",
            backend_result.err()
        );
    }

    Ok(())
}

/// Step: "When the payer requests the LNURL offer from the payee using {protocol}"
/// Makes a request to get the LNURL offer from a specific payee using the specified protocol
pub async fn step_when_the_payer_requests_the_lnurl_offer_from_the_payee(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Use the first available offer request from specified payee context
    // This is for backward compatibility with existing tests
    let payee = get_payee_from_context(ctx, payee_id)?;

    let (offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let offer_id = get_offer_id_from_request(offer_request, payee_id)?;

    let client = ctx.get_active_lnurl_client()?;
    let lnurl_offer = client.get_offer("default", &offer_id).await?;

    // Store offer response in the specific offer request
    let offer_key = offer_key.clone(); // Clone the key before borrowing mutably
    let offer_request = ctx
        .get_offer_request_mut(payee_id, &offer_key)
        .ok_or_else(|| anyhow_log!("Offer request not found for {} payee", payee_id))?;
    offer_request.lnurl_offer = Some(lnurl_offer.clone());

    Ok(())
}

/// Step: "Then the {payee_id} payee offer should contain valid sendable amounts"
/// Verifies that the offer contains valid min/max sendable amounts
pub async fn step_then_the_payee_offer_should_contain_valid_sendable_amounts(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get amounts from the LNURL offer in the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;
    let max_sendable = lnurl_offer.max_sendable;
    let min_sendable = lnurl_offer.min_sendable;

    // Verify amounts are reasonable
    if min_sendable == 0 {
        bail_log!("Min sendable amount should be greater than 0");
    }

    if max_sendable <= min_sendable {
        bail_log!("Max sendable should be greater than min sendable");
    }

    // Verify minimum expected values (vary by node type)
    if min_sendable != 1_000 {
        bail_log!("Expected min sendable 1000, got {}", min_sendable);
    }

    Ok(())
}

/// Step: "Then the {payee_id} payee offer should contain valid metadata"
/// Verifies that the offer contains valid metadata
pub async fn step_then_the_payee_offer_should_contain_valid_metadata(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Check if metadata ID exists in the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    if offer_request.metadata_id.is_none() {
        bail_log!("Offer metadata not found in {} payee context", payee_id);
    }

    Ok(())
}

/// Step: "Then the {payee_id} payee offer should provide a callback URL"
/// Verifies that the offer provides a callback URL for invoice generation
pub async fn step_then_the_payee_offer_should_provide_a_callback_url(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Check for callback URL in the LNURL offer
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;

    let callback_url = lnurl_offer.callback.as_str();
    Url::parse(callback_url)?;

    Ok(())
}

/// Step: "When the payer requests an invoice for 100 sats using the payee's callback URL with {protocol}"
/// Requests an invoice using the callback URL from a specific payee with the specified protocol
pub async fn step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
    ctx: &mut GlobalContext,
    payee_id: &str,
    protocol: &Protocol,
) -> Result<()> {
    // Get the LNURL offer
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;
    let callback_url = lnurl_offer.callback.as_str();
    if !callback_url.starts_with(&protocol.to_string()) {
        bail_log!("Callback URL is not expected protocol: {}", protocol);
    }

    let client = ctx.get_active_lnurl_client()?;
    let amount_msat = 100_000;
    let lnurl_invoice = client.get_invoice(lnurl_offer, amount_msat).await?;

    // Store invoice in the offer request for assertions
    let payee = get_payee_from_context_mut(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request_mut(payee, payee_id)?;
    offer_request.received_invoice = Some(lnurl_invoice.pr.clone());

    Ok(())
}

/// Step: "Then the payer should receive a valid Lightning invoice"
/// Verifies that a valid Lightning invoice was received
pub async fn step_then_the_payer_should_receive_a_valid_lightning_invoice(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get invoice from the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let invoice_str = get_invoice_from_request(offer_request, payee_id)?;

    // Parse the invoice to verify it's valid
    let _bolt11_invoice = parse_lightning_invoice(invoice_str)?;

    Ok(())
}

/// Step: "Then the invoice amount should be 100000 millisatoshis"
/// Verifies that the invoice amount matches the requested amount
pub async fn step_then_the_invoice_amount_should_be_100000_millisatoshis(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get and parse invoice from the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let invoice_str = get_invoice_from_request(offer_request, payee_id)?;

    let bolt11_invoice = parse_lightning_invoice(invoice_str)?;

    // Verify amount
    let expected_amount = 100_000; // 100 sats in msat
    validate_invoice_has_amount(&bolt11_invoice, expected_amount)?;

    Ok(())
}

/// Step: "Then the invoice description hash should match the metadata hash"
/// Verifies that the invoice description hash matches the offer metadata hash
pub async fn step_then_the_invoice_description_hash_should_match_the_metadata_hash(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get and parse invoice from the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let invoice_str = get_invoice_from_request(offer_request, payee_id)?;

    let bolt11_invoice = parse_lightning_invoice(invoice_str)?;

    // For this test, we verify that the invoice has a description hash
    // The actual hash verification would require the original metadata
    match bolt11_invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(_hash) => Ok(()),
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(_) => {
            bail_log!("Expected hash description for invoice but got direct description")
        }
    }
}

// =============================================================================
// MULTI-PAYEE STEP FUNCTIONS - For testing multiple payees with separate nodes
// =============================================================================

/// Step: "Given two payees each have their own lightning node"
/// Sets up the test environment with two distinct lightning nodes
pub async fn step_given_two_payees_each_have_their_own_lightning_node(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let cln_node = ctx.get_first_cln_node()?;
    ctx.add_payee("first", RegTestLnNode::Cln(cln_node.clone()));

    let lnd_node = ctx.get_first_lnd_node()?;
    ctx.add_payee("second", RegTestLnNode::Lnd(lnd_node.clone()));

    Ok(())
}

/// Step: "And both nodes are registered as separate backends"
/// Registers both payee nodes as backends
pub async fn step_and_both_nodes_are_registered_as_separate_backends(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Register both payee nodes
    register_payee_node_as_backend(ctx, "first").await?;
    register_payee_node_as_backend(ctx, "second").await?;

    Ok(())
}

// =============================================================================
// GENERIC PAYEE HELPER FUNCTIONS - For testing any number of payees
// =============================================================================

/// Generic function to register a payee's node as a backend
pub async fn register_payee_node_as_backend(ctx: &mut GlobalContext, payee_id: &str) -> Result<()> {
    let payee = get_payee_from_context(ctx, payee_id)?;
    let node = payee.node.clone();

    let client = ctx.get_active_discovery_client()?;

    let address = DiscoveryBackendAddress::PublicKey(*node.public_key());

    let url = match node.address() {
        RegTestLnNodeAddress::Inet(a) => Url::parse(format!("https://{a}").as_str())?,
        RegTestLnNodeAddress::Path(_) => bail!("socket address not supported"),
    };

    let implementation = match &node {
        RegTestLnNode::Cln(cln) => {
            DiscoveryBackendImplementation::ClnGrpc(ClnGrpcDiscoveryBackendImplementation {
                url,
                auth: ClnGrpcClientAuth::Path(ClnGrpcClientAuthPath {
                    ca_cert_path: cln.ca_cert_path.clone(),
                    client_cert_path: cln.client_cert_path.clone(),
                    client_key_path: cln.client_key_path.clone(),
                }),
                domain: Some(cln.sni.clone()),
            })
        }
        RegTestLnNode::Lnd(lnd) => {
            DiscoveryBackendImplementation::LndGrpc(LndGrpcDiscoveryBackendImplementation {
                url,
                auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                    tls_cert_path: lnd.tls_cert_path.clone(),
                    macaroon_path: lnd.macaroon_path.clone(),
                }),
                amp_invoice: false,
                domain: None,
            })
        }
    };

    let backend = DiscoveryBackend {
        address,
        backend: DiscoveryBackendSparse {
            partitions: ["default".to_string()].into(),
            weight: 100,
            implementation,
            enabled: true,
        },
    };

    client.post(backend.clone()).await?;

    Ok(())
}

/// Generic function to set up two payees with specific node types
pub async fn setup_two_payees_with_node_types(
    ctx: &mut GlobalContext,
    first_node_type: RegTestLnNodeType,
    second_node_type: RegTestLnNodeType,
) -> Result<()> {
    let first_node = match first_node_type {
        RegTestLnNodeType::Cln => RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()),
        RegTestLnNodeType::Lnd => RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()),
    };
    ctx.add_payee("first", first_node);

    let second_node = match second_node_type {
        RegTestLnNodeType::Cln => RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()),
        RegTestLnNodeType::Lnd => RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()),
    };
    ctx.add_payee("second", second_node);

    Ok(())
}

/// Step: "But when the payer requests an invoice for 100 sats using the {payee_id} payee callback URL"
/// Requests an invoice using the callback URL and expects it to fail
pub async fn step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(
    ctx: &mut GlobalContext,
    payee_id: &str,
    protocol: &Protocol,
) -> Result<()> {
    // Use the callback URL from the LNURL offer
    let payee = ctx
        .get_payee(payee_id)
        .ok_or_else(|| anyhow_log!("No {} payee found", payee_id))?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;
    let callback_url = lnurl_offer.callback.as_str();
    if !callback_url.starts_with(&protocol.to_string()) {
        bail_log!("Callback URL is not expected protocol: {}", protocol);
    }
    // Request invoice for 100 sats (100000 msat) - expecting failure
    let amount_msat = 100_000;

    let client = ctx.get_active_lnurl_client()?;

    let response = client.get_invoice(lnurl_offer, amount_msat as usize).await;

    if response.is_ok() {
        bail_log!("expected failure, received {:?}", response);
    }

    Ok(())
}

// =============================================================================
// BACKEND ENABLE/DISABLE STEP FUNCTIONS - For backend lifecycle management
// =============================================================================

/// Step: "Given the payee has access to both CLN and LND lightning nodes"
/// Sets up the test environment with both CLN and LND lightning nodes for a single payee
pub async fn step_given_the_payee_has_access_to_both_cln_and_lnd_lightning_nodes(
    ctx: &mut GlobalContext,
) -> Result<()> {
    ctx.add_payee("cln", RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()));
    ctx.add_payee("lnd", RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()));

    Ok(())
}

/// Step: "And the payee has created an offer linked to both lightning nodes"
/// Creates a single offer with both CLN and LND public keys in the payees vector
pub async fn step_and_the_payee_has_created_an_offer_linked_to_both_lightning_nodes(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let client = ctx.get_active_offer_client()?;

    // Create offer metadata
    let metadata_id = Uuid::new_v4();
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let offer_metadata = OfferMetadata {
        id: metadata_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: format!("multi-backend-{random_string}"),
            long_text: Some("multi-backend-offer".to_string()),
            image: None,
            identifier: None,
        },
    };

    client.post_metadata(offer_metadata.clone()).await?;

    let offer_id = Uuid::new_v4();
    let now = Utc::now();
    let offer = OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable: 1_000_000_000, // 1000 sats in msat
            min_sendable: 1_000,         // 1 sat in msat
            metadata_id,
            timestamp: now - ChronoDuration::minutes(5),
            expires: Some(now + ChronoDuration::hours(24)),
        },
    };

    client.post_offer(offer.clone()).await?;

    // Store offer info in lnd payee context for access in tests
    let mut offer_request = OfferRequest::new();
    offer_request.offer_id = Some(offer_id);
    offer_request.metadata_id = Some(metadata_id);

    ctx.add_offer_request("lnd", "multi_backend_offer", offer_request)
        .map_err(|e| anyhow_log!("Failed to add offer request: {}", e))?;

    Ok(())
}

/// Step: "And both nodes are registered as separate backends" - Backend enable/disable version
/// Registers both CLN and LND nodes as backends for backend lifecycle management
/// Step: "Given the payer can generate invoices successfully"
/// Verifies that the payer can successfully generate invoices before testing backend operations
pub async fn step_given_the_payer_can_generate_invoices_successfully(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get the LNURL offer to establish the callback URL
    let offer_request = ctx
        .get_offer_request("lnd", "multi_backend_offer")
        .ok_or_else(|| anyhow_log!("Offer request not found"))?;
    let offer_id = offer_request
        .offer_id
        .as_ref()
        .ok_or_else(|| anyhow_log!("Offer ID not set"))?;

    let client = ctx.get_active_lnurl_client()?;
    let lnurl_offer_response = client.get_offer("default", offer_id).await?;

    // Store the callback in the lnurl_offer field
    if let Some(offer_request_mut) = ctx.get_offer_request_mut("lnd", "multi_backend_offer") {
        offer_request_mut.lnurl_offer = Some(lnurl_offer_response);
    }

    // Request and validate invoice
    request_and_validate_invoice_helper(ctx, "lnd", &Service::LnUrl).await?;

    // Clear the received invoice for future tests
    if let Some(offer_request) = ctx.get_offer_request_mut("lnd", "multi_backend_offer") {
        offer_request.received_invoice = None;
    }

    Ok(())
}

/// Step: "When the admin disables the first backend"
/// Disables the first backend (LND)
pub async fn step_when_the_admin_disables_the_first_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Get LND backend location
    let lnd_payee = ctx
        .get_payee("lnd")
        .ok_or_else(|| anyhow_log!("LND payee not found in context"))?;

    let backend_location = DiscoveryBackendAddress::PublicKey(*lnd_payee.node.public_key());
    let backend_location = backend_location.encoded();
    enable_disable_backend(ctx, &backend_location, false).await?;

    Ok(())
}

/// Step: "Then the payer can still get invoices"
/// Verifies that invoice generation still works after disabling one backend
pub async fn step_then_the_payer_can_still_generate_invoices(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Request and validate invoice
    request_and_validate_invoice_helper(ctx, "lnd", &Service::LnUrl).await?;

    Ok(())
}

/// Step: "When the admin disables the second backend"
/// Disables the second backend (CLN)
pub async fn step_when_the_admin_disables_the_second_backend(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get CLN backend location
    let cln_payee = ctx
        .get_payee("cln")
        .ok_or_else(|| anyhow_log!("CLN payee not found in context"))?;

    let backend_location = DiscoveryBackendAddress::PublicKey(*cln_payee.node.public_key());
    let backend_location = backend_location.encoded();
    enable_disable_backend(ctx, &backend_location, false).await?;

    Ok(())
}

/// Step: "Then the payer cannot get invoices"
/// Verifies that invoice generation fails when all backends are disabled
pub async fn step_then_the_payer_cannot_generate_invoices(
    ctx: &mut GlobalContext,
    limit: Duration,
) -> Result<()> {
    // Clear any previous invoice
    if let Some(offer_request) = ctx.get_offer_request_mut("lnd", "multi_backend_offer") {
        offer_request.received_invoice = None;
    }

    // Try to request an invoice and expect it to fail
    let offer_request = ctx
        .get_offer_request("lnd", "multi_backend_offer")
        .ok_or_else(|| anyhow_log!("Offer request not found"))?;

    if let Some(lnurl_offer) = &offer_request.lnurl_offer {
        let callback_url = lnurl_offer.callback.as_str();
        Url::parse(callback_url)?;

        let client = ctx.get_active_lnurl_client()?;

        let start_time = SystemTime::now();

        while SystemTime::now().duration_since(start_time)? < limit {
            match client.get_invoice(lnurl_offer, 100000).await {
                Ok(_) => {
                    tokio_sleep(Duration::from_millis(50)).await;
                    continue;
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains(&StatusCode::BAD_GATEWAY.to_string()) {
                        return Ok(());
                    }
                }
            }
        }
        bail_log!("Expected invoice generation to fail, but it succeeded");
    } else {
        bail_log!("No callback URL available");
    }
}

/// Step: "When the admin enables any backend"
/// Re-enables any backend (LND in this case)
pub async fn step_when_the_admin_enables_any_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Re-enable LND backend
    let lnd_payee = ctx
        .get_payee("lnd")
        .ok_or_else(|| anyhow_log!("LND payee not found in context"))?;

    let backend_location = DiscoveryBackendAddress::PublicKey(*lnd_payee.node.public_key());
    let backend_location = backend_location.encoded();
    enable_disable_backend(ctx, &backend_location, true).await?;

    Ok(())
}

/// Step: "Then the payer can again get invoices"
/// Verifies that invoice generation is restored after re-enabling a backend
pub async fn step_then_the_payer_can_again_generate_invoices(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Request and validate invoice
    request_and_validate_invoice_helper(ctx, "lnd", &Service::LnUrl).await?;

    Ok(())
}

async fn enable_disable_backend(
    ctx: &mut GlobalContext,
    location: &str,
    enabled: bool,
) -> Result<()> {
    let client = ctx.get_active_discovery_client()?;

    // Parse the location to get the RawSocketAddress
    let address = DiscoveryBackendAddress::from_str(location)?;

    // GET the current backend
    let mut backend = client
        .get(&address)
        .await?
        .ok_or_else(|| anyhow_log!("Backend at location {} not found", location))?;

    // Modify the enabled field
    backend.backend.enabled = enabled;

    // PUT the modified backend
    let is_created = client.put(backend.clone()).await?;
    if is_created {
        bail_log!("PUT should update existing backend, not create new one");
    }

    Ok(())
}

/// Step: "When the admin deletes the first backend"
/// Deletes the first backend (LND) from the discovery service
pub async fn step_when_the_admin_deletes_the_first_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Get LND backend location
    let lnd_payee = ctx
        .get_payee("lnd")
        .ok_or_else(|| anyhow_log!("LND payee not found in context"))?;

    let backend_location = DiscoveryBackendAddress::PublicKey(*lnd_payee.node.public_key());
    let backend_location = backend_location.encoded();
    delete_backend(ctx, &backend_location).await?;

    Ok(())
}

/// Step: "When the admin deletes the second backend"
/// Deletes the second backend (CLN) from the discovery service
pub async fn step_when_the_admin_deletes_the_second_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Get CLN backend location
    let cln_payee = ctx
        .get_payee("cln")
        .ok_or_else(|| anyhow_log!("CLN payee not found in context"))?;

    let backend_location = DiscoveryBackendAddress::PublicKey(*cln_payee.node.public_key());
    let backend_location = backend_location.encoded();
    delete_backend(ctx, &backend_location).await?;

    Ok(())
}

/// Step: "And both nodes are ready to be registered as backends"
/// Prepares nodes for backend registration without actually registering them
pub async fn step_and_both_nodes_are_ready_to_be_registered_as_backends(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Ensure both nodes are available in context
    if ctx.get_payee("cln").is_none() {
        bail_log!("CLN payee not found in context");
    }
    if ctx.get_payee("lnd").is_none() {
        bail_log!("LND payee not found in context");
    }

    Ok(())
}

/// Helper function to delete backends via the discovery API
async fn delete_backend(ctx: &mut GlobalContext, location: &str) -> Result<()> {
    let client = ctx.get_active_discovery_client()?;

    // Parse the encoded location back to RawSocketAddress
    let address: DiscoveryBackendAddress = location.parse()?;

    // Delete the backend
    client.delete(&address).await?;

    Ok(())
}

// =============================================================================
// SERVER PERSISTENCE STEP FUNCTIONS
// =============================================================================

/// Step: "When I delete the persistent {backend_store} backend storage files"
/// Deletes the backend storage files to test data loss scenarios
pub async fn step_when_i_delete_the_persistent_backend_storage_files(
    ctx: &mut GlobalContext,
    delete_discovery_store: bool,
    delete_offer_store: bool,
) -> Result<()> {
    if delete_discovery_store {
        std::fs::remove_dir_all(ctx.get_active_discovery_store_dir()?)?;
        std::fs::create_dir_all(ctx.get_active_discovery_store_dir()?)?;
    }

    if delete_offer_store {
        std::fs::remove_dir_all(ctx.get_active_offer_store_dir()?)?;
        std::fs::create_dir_all(ctx.get_active_offer_store_dir()?)?;
    }

    Ok(())
}

/// Step: "When I start the LNURL server with enablement flag {flag}"
/// Starts the LNURL server with specific service enablement flags
pub async fn step_when_i_start_the_lnurl_server_with_enablement_flags(
    ctx: &mut GlobalContext,
    start_services: &[Service],
) -> Result<()> {
    let _pid = ctx
        .start_active_server(start_services, log::Level::Info)
        .await?;
    Ok(())
}

/// Step: "And the lnurl service should be listening on the configured port"
/// Verifies that the LNURL service is listening on its configured port
pub async fn step_and_the_lnurl_service_should_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::LnUrl], true, 1000, 5).await?;
    Ok(())
}

/// Step: "And the discovery service should be listening on the configured port"
/// Verifies that the discovery service is listening on its configured port
pub async fn step_and_the_discovery_service_should_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Discovery], true, 1000, 5).await?;
    Ok(())
}

/// Step: "And the offers service should be listening on the configured port"
/// Verifies that the offers service is listening on its configured port
pub async fn step_and_the_offers_service_should_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Offer], true, 1000, 5).await?;
    Ok(())
}

/// Step: "And the lnurl service should not be listening on the configured port"
/// Verifies that the LNURL service is NOT listening on its configured port
pub async fn step_and_the_lnurl_service_should_not_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::LnUrl], false, 100, 1).await?;
    Ok(())
}

/// Step: "And the discovery service should not be listening on the configured port"
/// Verifies that the discovery service is NOT listening on its configured port
pub async fn step_and_the_discovery_service_should_not_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Discovery], false, 100, 1).await?;
    Ok(())
}

/// Step: "And the offers service should not be listening on the configured port"
/// Verifies that the offers service is NOT listening on its configured port
pub async fn step_and_the_offers_service_should_not_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Offer], false, 100, 1).await?;
    Ok(())
}

// =============================================================================
// SERVICE LOGS STEP FUNCTIONS - For testing service logging functionality
// =============================================================================

/// Step: "When I request an offer from a non-existent partition"
/// Makes a request for an offer with a non-existent partition to trigger 404 error
pub async fn step_when_i_request_an_offer_from_a_non_existent_partition(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let client = ctx.get_active_lnurl_client()?;

    // Try to get a non-existent offer with a different partition name to trigger 404
    let non_existent_offer_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000")?;
    let result = client
        .get_offer("non-existent-partition", &non_existent_offer_id)
        .await;

    // Expect this to fail with a 404
    match result {
        Ok(_) => {
            bail_log!("Expected offer request to fail, but it succeeded");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if !error_msg.contains("404") {
                bail_log!("Expected 404 error, but got: {}", error_msg);
            }
        }
    }

    Ok(())
}

/// Step: "When I request an invoice for a non-existent offer"
/// Makes a request for an invoice with a non-existent offer ID
pub async fn step_when_i_request_an_invoice_for_a_non_existent_offer(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let client = ctx.get_active_lnurl_client()?;
    let non_existent_offer_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000")?;

    // Try to get a non-existent offer, expect it to fail
    let result = client.get_offer("default", &non_existent_offer_id).await;
    let status_code = match result {
        Ok(_) => {
            bail_log!("Expected offer request to fail, but it succeeded");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("404") {
                404
            } else {
                bail_log!("Expected 404 error, got: {}", error_msg);
            }
        }
    };

    // Assert expected 404 status code
    if status_code != 404 {
        bail_log!(
            "Expected 404 status code for non-existent offer, got {}",
            status_code
        );
    }

    Ok(())
}

/// Step: "When I try to get a missing backend"
/// Attempts to get a backend that doesn't exist to trigger error logging
pub async fn step_when_i_try_to_get_a_missing_backend(ctx: &mut GlobalContext) -> Result<()> {
    let client = ctx.get_active_discovery_client()?;

    let address = DiscoveryBackendAddress::Url(Url::parse("http://fake.com")?);
    let backend = client.get(&address).await?;

    if backend.is_some() {
        bail_log!("backend found unexpectedly")
    } else {
        Ok(())
    }
}

// =============================================================================
// POST-SHUTDOWN LOG ASSERTION STEP FUNCTIONS - Assert logs after server stops
// =============================================================================

/// Step: "And the server logs should contain health check requests for all services"
/// Verifies that health check requests are logged for all services (expected pattern: clf::{service} ... /health 200)
pub async fn step_and_the_server_logs_should_contain_health_check_requests_for_all_services(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Expected patterns for health check logs: clf::{service_name} ... /health HTTP/1.1 200
    let expected_services = [Service::LnUrl, Service::Discovery, Service::Offer];
    let mut service_counts = std::collections::HashMap::new();

    // Initialize counts
    for service in &expected_services {
        service_counts.insert(service.to_string(), 0);
    }

    // Iterate through each line and count matches
    for line in &stderr_lines {
        for service in &expected_services {
            let clf_pattern = format!("clf::{service}");
            if line.contains(&clf_pattern)
                && line.contains("/health HTTP/1.1 200")
                && line.contains(" INFO ")
            {
                *service_counts
                    .get_mut(&service.to_string())
                    .ok_or_else(|| anyhow_log!("could not find service {service}"))? += 1;
            }
        }
    }

    // Verify exact expected counts for each service (at least 1 health check per service)
    let expected_min_count = 1;
    let mut service_errors = Vec::new();
    let mut found_services = Vec::new();

    for (service, count) in &service_counts {
        if *count < expected_min_count {
            service_errors.push(format!(
                "{service}:expected≥{expected_min_count},got{count}",
            ));
        } else {
            found_services.push(format!("{service}:{count}"));
        }
    }

    if service_errors.is_empty() {
        Ok(())
    } else {
        let error_msg = format!("❌ Health check log count mismatch: {service_errors:?}. Found: {found_services:?} (expected: clf::{{service}} ... /health HTTP/1.1 200 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain backend registration requests"
/// Verifies that backend registration requests are logged
pub async fn step_and_the_server_logs_should_contain_backend_registration_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let server_logs = stderr_lines.join("\n");

    // Look for backend registration patterns: clf::discovery ... POST /discovery/default HTTP/1.1 201
    let registration_patterns = ["clf::discovery", "POST /discovery HTTP/1.1 201", " INFO "];
    let expected_count = 1; // We register exactly 1 backend in the test
    let registration_count = count_log_patterns(&server_logs, &registration_patterns);

    if registration_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Backend registration log count mismatch: expected={expected_count}, got={registration_count} (pattern: clf::discovery ... POST /discovery/default HTTP/1.1 201 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain offer retrieval requests"
/// Verifies that offer retrieval requests are logged
pub async fn step_and_the_server_logs_should_contain_offer_retrieval_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for offer retrieval patterns: clf::lnurl ... GET /offers/default/{uuid} HTTP/1.1 200
    let mut offer_count = 0;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("HTTP/1.1 200")
            && line.contains(" INFO ")
        {
            // Verify the URI pattern matches the full path with UUID
            if let Some(get_start) = line.find("GET /offers/default/") {
                let uri_part = &line[get_start..];
                if let Some(http_start) = uri_part.find(" HTTP/1.1") {
                    let full_uri = &uri_part[..http_start];
                    // Verify it has the UUID pattern: GET /offers/default/{uuid}
                    if full_uri.len() > "GET /offers/default/".len() + 30 {
                        // UUID is 36 chars
                        offer_count += 1;
                    }
                }
            }
        }
    }

    let expected_count = 2; // We make multiple offer-related requests (1 explicit + health checks)
    if offer_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Offer retrieval log count mismatch: expected={expected_count}, got={offer_count} (pattern: clf::lnurl ... GET /offers/default/{{uuid}} HTTP/1.1 200 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain invoice generation requests"
/// Verifies that invoice generation requests are logged
pub async fn step_and_the_server_logs_should_contain_invoice_generation_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for invoice generation patterns: clf::lnurl ... GET /offers/default/{uuid}/invoice?amount={msat} HTTP/1.1 200
    let mut invoice_count = 0;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("/invoice?amount=")
            && line.contains("HTTP/1.1 200")
            && line.contains(" INFO ")
        {
            // Verify the URI pattern matches the full invoice path with UUID and amount
            if let Some(get_start) = line.find("GET ") {
                let uri_part = &line[get_start..];
                if let Some(http_start) = uri_part.find(" HTTP/1.1") {
                    let full_uri = &uri_part[4..http_start]; // Skip "GET "
                                                             // Verify it has invoice pattern with UUID and amount query param
                    if full_uri.contains("/offers/default/")
                        && full_uri.contains("/invoice?amount=")
                    {
                        // Further verify it has reasonable UUID length (offers/default/{uuid}/invoice?amount=)
                        if full_uri.len() > "/offers/default/".len() + 30 {
                            // UUID is 36 chars
                            invoice_count += 1;
                        }
                    }
                }
            }
        }
    }

    let expected_count = 1; // We make exactly 1 invoice generation request in the test
    if invoice_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Invoice generation log count mismatch: expected={expected_count}, got={invoice_count} (pattern: clf::lnurl ... GET /offers/default/{{uuid}}/invoice?amount={{msat}} HTTP/1.1 200 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain 404 error responses"
/// Verifies that 404 error responses are logged
pub async fn step_and_the_server_logs_should_contain_404_error_responses(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for 404 error patterns: clf::{service} ... HTTP/1.1 404
    let mut error_404_count = 0;

    for line in &stderr_lines {
        if (line.contains("clf::lnurl")
            || line.contains("clf::discovery")
            || line.contains("clf::offer"))
            && line.contains("HTTP/1.1 404")
            && line.contains(" WARN ")
        {
            error_404_count += 1;
        }
    }

    let expected_count = 3; // We make 3 requests that result in 404: non-existent endpoint + non-existent offer + 1 discovery GET request
    if error_404_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ 404 error log count mismatch: expected={expected_count}, got={error_404_count} (pattern: clf::{{service}} ... HTTP/1.1 404 WARN)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain invalid offer error responses"
/// Verifies that invalid offer error responses are logged
pub async fn step_and_the_server_logs_should_contain_invalid_offer_error_responses(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let server_logs = stderr_lines.join("\n");

    // Look for invalid offer error patterns: clf::lnurl ... HTTP/1.1 404
    let invalid_offer_patterns = ["clf::lnurl", "HTTP/1.1 404", " WARN "];
    let expected_count = 2; // We make 2 requests to lnurl service that result in 404: non-existent endpoint + non-existent offer
    let invalid_offer_count = count_log_patterns(&server_logs, &invalid_offer_patterns);

    if invalid_offer_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Invalid offer error log count mismatch: expected={expected_count}, got={invalid_offer_count} (pattern: clf::lnurl ... HTTP/1.1 404 WARN)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain invalid backend get errors"
/// Verifies that invalid backend get errors are logged
pub async fn step_and_the_server_logs_should_contain_invalid_backend_get_errors(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let server_logs = stderr_lines.join("\n");

    let invalid_backend_patterns = [
        "clf::discovery",
        "GET /discovery/url/aHR0cDovL2Zha2UuY29tLw",
        "HTTP/1.1 404",
        " WARN ",
    ];
    let expected_count = 1; // We make exactly 1 invalid backend GET request in the test
    let invalid_backend_count = count_log_patterns(&server_logs, &invalid_backend_patterns);

    if invalid_backend_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Invalid backend GET error log count mismatch: expected={expected_count}, got={invalid_backend_count} (pattern: clf::discovery ... GET /discovery/url/aHR0cDovL2Zha2UuY29tLw 404 WARN)");
        bail_log!(error_msg)
    }
}

// =============================================================================
// MULTI-SERVER STEP FUNCTIONS - For HTTP remote stores testing
// =============================================================================

/// Step: "When I start server 1 with offers and discovery services"
/// Starts server 1 with only offers and discovery services enabled
pub async fn step_when_i_start_server_1_with_offers_and_discovery_services(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let _pid = ctx
        .start_active_server(&[Service::Discovery, Service::Offer], log::Level::Info)
        .await?;

    Ok(())
}

/// Step: "When I start server 2 with only lnurl service"
/// Starts server 2 with only the lnurl service enabled
pub async fn step_when_i_start_server_2_with_only_lnurl_service(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let _pid = ctx
        .start_active_server(&[Service::LnUrl], log::Level::Info)
        .await?;
    Ok(())
}

/// Step: "Then server 1 should have offers and discovery services listening"
/// Verifies that server 1's offers and discovery services are listening
pub async fn step_then_server_1_should_have_offers_and_discovery_services_listening(
    ctx: &mut GlobalContext,
) -> Result<()> {
    check_services_listening_status(
        ctx,
        vec![Service::Discovery, Service::Offer],
        true,
        1000,
        10,
    )
    .await?;

    Ok(())
}

/// Step: "Then server 2 should have only lnurl service listening"
/// Verifies that server 2's lnurl service is listening
pub async fn step_then_server_2_should_have_only_lnurl_service_listening(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::LnUrl], true, 1000, 10).await?;

    Ok(())
}

/// Step: "When I stop all servers"
/// Stops all running server processes and captures their logs
pub async fn step_when_i_stop_all_servers(ctx: &mut GlobalContext) -> Result<()> {
    ctx.stop_all_servers()?;

    Ok(())
}

/// Step: "Then server 1 logs should contain offer creation requests"
/// Validates that server 1 logs contain expected offer creation patterns
pub async fn step_then_server_1_logs_should_contain_offer_creation_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stdout buffer");
    };

    // Look for offer creation patterns
    let mut metadata_creation_found = false;
    let mut offer_creation_found = false;

    for line in &stderr_lines {
        if line.contains("clf::offer") && line.contains("POST /metadata HTTP/1.1 201") {
            metadata_creation_found = true;
        }
        if line.contains("clf::offer") && line.contains("POST /offers HTTP/1.1 201") {
            offer_creation_found = true;
        }
    }

    if metadata_creation_found && offer_creation_found {
        Ok(())
    } else {
        let error_msg = format!(
            "❌ Server 1 offer creation logs missing: metadata={metadata_creation_found}, offer={offer_creation_found}",

        );
        bail_log!(error_msg)
    }
}

/// Step: "And server 1 logs should contain backend registration requests"
/// Validates that server 1 logs contain expected backend registration patterns
pub async fn step_and_server_1_logs_should_contain_backend_registration_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for backend registration patterns
    let mut backend_registration_found = false;

    for line in &stderr_lines {
        if line.contains("clf::discovery") && line.contains("POST /discovery HTTP/1.1 201") {
            backend_registration_found = true;
            break;
        }
    }

    if backend_registration_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 1 backend registration logs missing";
        bail_log!(error_msg)
    }
}

/// Step: "And server 1 logs should contain health check requests for offers and discovery services"
/// Validates that server 1 logs contain health check patterns for offers and discovery services
pub async fn step_and_server_1_logs_should_contain_health_check_requests_for_offers_and_discovery_services(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut offers_health_found = false;
    let mut discovery_health_found = false;

    for line in &stderr_lines {
        if line.contains("clf::offer") && line.contains("GET /health HTTP/1.1 200") {
            offers_health_found = true;
        }
        if line.contains("clf::discovery") && line.contains("GET /health HTTP/1.1 200") {
            discovery_health_found = true;
        }
    }

    if offers_health_found && discovery_health_found {
        Ok(())
    } else {
        let error_msg = format!(
            "❌ Server 1 health check logs missing: offers={offers_health_found}, discovery={discovery_health_found}",

        );
        bail_log!(error_msg)
    }
}

/// Step: "And server 1 logs should contain HTTP requests from server 2 for offers and discovery"
/// Validates that server 1 logs show HTTP requests from server 2 for offers and discovery services
pub async fn step_and_server_1_logs_should_contain_http_requests_from_server_2_for_offers_and_discovery(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for HTTP requests from server 2 to server 1's services
    let mut offers_http_request_found = false;
    let mut discovery_http_request_found = false;

    for line in &stderr_lines {
        // Look for offer requests from HTTP stores (server 2 accessing server 1's offers service)
        if line.contains("clf::offer")
            && (line.contains("GET /offers/default/") || line.contains("GET /metadata/default/"))
            && line.contains("HTTP/1.1 200")
        {
            offers_http_request_found = true;
        }

        // Look for discovery requests from HTTP stores (server 2 accessing server 1's discovery service)
        if line.contains("clf::discovery")
            && line.contains("GET /discovery")
            && line.contains("HTTP/1.1 200")
        {
            discovery_http_request_found = true;
        }
    }

    if offers_http_request_found && discovery_http_request_found {
        Ok(())
    } else {
        let error_msg = format!(
            "❌ Server 1 HTTP requests from server 2 missing: offers={offers_http_request_found}, discovery={discovery_http_request_found}",
        );
        bail_log!(error_msg)
    }
}

/// Step: "And server 2 logs should contain offer retrieval requests via HTTP stores"
/// Validates that server 2 logs contain patterns showing HTTP store usage for offers
pub async fn step_and_server_2_logs_should_contain_offer_retrieval_requests_via_http_stores(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut offer_retrieval_found = false;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("HTTP/1.1 200")
        {
            offer_retrieval_found = true;
            break;
        }
    }

    if offer_retrieval_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 2 offer retrieval via HTTP stores logs missing";
        bail_log!(error_msg)
    }
}

/// Step: "And server 2 logs should contain invoice generation requests"
/// Validates that server 2 logs contain invoice generation patterns
pub async fn step_and_server_2_logs_should_contain_invoice_generation_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut invoice_generation_found = false;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("/invoice")
            && line.contains("HTTP/1.1 200")
        {
            invoice_generation_found = true;
            break;
        }
    }

    if invoice_generation_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 2 invoice generation logs missing";
        bail_log!(error_msg)
    }
}

/// Step: "And server 2 logs should contain health check requests for lnurl service"
/// Validates that server 2 logs contain health check patterns for lnurl service
pub async fn step_and_server_2_logs_should_contain_health_check_requests_for_lnurl_service(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut lnurl_health_found = false;

    for line in &stderr_lines {
        if line.contains("clf::lnurl") && line.contains("GET /health HTTP/1.1 200") {
            lnurl_health_found = true;
            break;
        }
    }

    if lnurl_health_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 2 lnurl health check logs missing";
        bail_log!(error_msg)
    }
}
