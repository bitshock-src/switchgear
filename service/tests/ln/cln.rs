use crate::try_create_cln_backend;
use anyhow::bail;
use lightning_invoice::Bolt11Invoice;
use rand::{distributions::Alphanumeric, Rng};
use secp256k1::hashes::Hash;
use sha2::{Digest, Sha256};
use std::str::FromStr;
use std::time::Duration;
use switchgear_service::api::discovery::DiscoveryBackendImplementation;
use switchgear_service::components::pool::cln::grpc::client::DefaultClnGrpcClient;
use switchgear_service::components::pool::{Bolt11InvoiceDescription, LnRpcClient};

async fn try_create_cln_client() -> anyhow::Result<
    Option<
        Box<
            dyn LnRpcClient<Error = switchgear_service::components::pool::error::LnPoolError>
                + Send
                + Sync
                + 'static,
        >,
    >,
> {
    let backend = match try_create_cln_backend()? {
        None => return Ok(None),
        Some(backend) => match backend.backend.implementation {
            DiscoveryBackendImplementation::ClnGrpc(b) => b,
            _ => bail!("wrong implementation"),
        },
    };

    let client = DefaultClnGrpcClient::create(Duration::from_secs(1), backend)?;

    Ok(Some(Box::new(client)))
}

#[tokio::test]
async fn test_cln_invoice_with_direct_description() {
    let client = match try_create_cln_client().await {
        Ok(Some(client)) => client,
        Ok(None) => return, // Test skipped gracefully
        Err(e) => panic!("{}", e),
    };

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let expected_amount_msat = 1_000_000; // 1000 sats in msat
    let expected_expiry_secs = 3600; // 1 hour expiry

    let description = Bolt11InvoiceDescription::Direct(&random_string);
    let invoice_str = client
        .get_invoice(
            Some(expected_amount_msat),
            description,
            Some(expected_expiry_secs),
        )
        .await
        .expect("Failed to generate invoice with direct description");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate amount
    assert_eq!(
        invoice.amount_milli_satoshis().unwrap(),
        expected_amount_msat
    );

    // Validate description is Direct type with correct content
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(desc) => {
            let desc_str = desc.to_string();
            assert_eq!(
                desc_str, random_string,
                "Invoice description '{desc_str}' doesn't match expected '{random_string}'",
            );
        }
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(_) => {
            panic!("Expected Direct description but got Hash description");
        }
    }

    // Validate expiry
    assert_eq!(invoice.expiry_time().as_secs(), expected_expiry_secs);
}

#[tokio::test]
async fn test_cln_invoice_with_direct_into_hash_description() {
    let client = match try_create_cln_client().await {
        Ok(Some(client)) => client,
        Ok(None) => return, // Test skipped gracefully
        Err(e) => panic!("{}", e),
    };

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let expected_amount_msat = 500_000; // 500 sats in msat

    let description = Bolt11InvoiceDescription::DirectIntoHash(&random_string);
    let invoice_str = client
        .get_invoice(
            Some(expected_amount_msat),
            description,
            Some(1800), // 30 minutes
        )
        .await
        .expect("Failed to generate invoice with direct-into-hash description");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate amount
    assert_eq!(
        invoice.amount_milli_satoshis().unwrap(),
        expected_amount_msat
    );

    // Validate description is Hash type
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(hash) => {
            let hash = hash.0.to_byte_array();
            // Calculate expected SHA256 hash for verification
            let mut hasher = Sha256::new();
            hasher.update(random_string.as_bytes());
            let expected_hash: [u8; 32] = hasher.finalize().to_vec().try_into().unwrap();

            assert_eq!(expected_hash, hash);
        }
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(_) => {
            panic!("Expected Hash description but got Direct description");
        }
    }
}

#[tokio::test]
async fn test_cln_invoice_with_hash_description_produces_error() {
    let client = match try_create_cln_client().await {
        Ok(Some(client)) => client,
        Ok(None) => return, // Test skipped gracefully
        Err(e) => panic!("{}", e),
    };

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    // Create a SHA256 hash from the random string
    let mut hasher = Sha256::new();
    hasher.update(random_string.as_bytes());
    let hash_bytes = hasher.finalize();
    let hash_array: [u8; 32] = hash_bytes.as_slice().try_into().unwrap();

    let description = Bolt11InvoiceDescription::Hash(&hash_array);
    let result = client
        .get_invoice(
            Some(250_000), // 250 sats in msat
            description,
            Some(900), // 15 minutes
        )
        .await;

    let error = result.expect_err("Expected error for Hash description but got successful invoice");

    // Verify it's the expected error message
    let error_msg = error.to_string();
    assert!(
        error_msg.contains("hash descriptions unsupported"),
        "Error message should mention hash descriptions unsupported, got: {error_msg}",
    );
}

#[tokio::test]
async fn test_cln_invoice_with_none_amount() {
    let client = match try_create_cln_client().await {
        Ok(Some(client)) => client,
        Ok(None) => return, // Test skipped gracefully
        Err(e) => panic!("{}", e),
    };

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let description = Bolt11InvoiceDescription::Direct(&random_string);
    let invoice_str = client
        .get_invoice(
            None, // No amount specified
            description,
            Some(7200), // 2 hours
        )
        .await
        .expect("Failed to generate invoice with no amount");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate that amount is None (zero-amount invoice)
    assert!(
        invoice.amount_milli_satoshis().is_none(),
        "Expected no amount but got: {:?}",
        invoice.amount_milli_satoshis()
    );

    // Validate description
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(desc) => {
            assert_eq!(desc.to_string(), random_string);
        }
        _ => panic!("Expected Direct description"),
    }

    // Validate expiry
    assert_eq!(invoice.expiry_time().as_secs(), 7200);
}

#[tokio::test]
async fn test_cln_invoice_with_none_expiry() {
    let client = match try_create_cln_client().await {
        Ok(Some(client)) => client,
        Ok(None) => return, // Test skipped gracefully
        Err(e) => panic!("{}", e),
    };

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let description = Bolt11InvoiceDescription::Direct(&random_string);
    let invoice_str = client
        .get_invoice(
            Some(750_000), // 750 sats in msat
            description,
            None, // No expiry specified - should use CLN default
        )
        .await
        .expect("Failed to generate invoice with no expiry");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate amount
    assert_eq!(invoice.amount_milli_satoshis().unwrap(), 750_000);

    // Validate description
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(desc) => {
            assert_eq!(desc.to_string(), random_string);
        }
        _ => panic!("Expected Direct description"),
    }

    // Validate expiry is CLN's default (typically 3600 seconds = 1 hour)
    let expiry_secs = invoice.expiry_time().as_secs();

    // CLN's default should be reasonable (between 1 minute and 7 days)
    assert!(
        (60..=604800).contains(&expiry_secs),
        "Expected reasonable default expiry, got {expiry_secs} seconds",
    );
}

#[tokio::test]
async fn test_cln_metrics() {
    let client = match try_create_cln_client().await {
        Ok(Some(client)) => client,
        Ok(None) => return, // Test skipped gracefully
        Err(e) => panic!("{}", e),
    };

    let metrics_result = client
        .get_metrics()
        .await
        .expect("Failed to connect to LND node and retrieve metrics");

    assert!(
        metrics_result.healthy,
        "Expected metrics response (proving LND connectivity) but got None"
    );
}
