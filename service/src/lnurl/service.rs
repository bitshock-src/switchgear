use crate::axum::partitions::PartitionsLayer;
use crate::lnurl::pay::handler::LnUrlPayHandlers;
use crate::lnurl::pay::state::LnUrlPayState;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use switchgear_service_api::balance::LnBalancer;
use switchgear_service_api::offer::OfferProvider;

#[derive(Debug)]
pub struct LnUrlBalancerService;

impl LnUrlBalancerService {
    pub fn router<O, B>(state: LnUrlPayState<O, B>) -> Router
    where
        O: OfferProvider + Send + Sync + Clone + 'static,
        B: LnBalancer + Send + Sync + Clone + 'static,
    {
        Router::new()
            .route(
                "/offers/{partition}/{id}/bech32/qr",
                get(LnUrlPayHandlers::bech32_qr),
            )
            .route(
                "/offers/{partition}/{id}/bech32",
                get(LnUrlPayHandlers::bech32),
            )
            .route(
                "/offers/{partition}/{id}/invoice",
                get(LnUrlPayHandlers::invoice),
            )
            .route("/offers/{partition}/{id}", get(LnUrlPayHandlers::offer))
            .layer(PartitionsLayer::new(Arc::new(state.partitions().clone())))
            .route("/health/full", get(LnUrlPayHandlers::health_full))
            .route("/health", get(Self::health_check_handler))
            .with_state(state)
    }

    async fn health_check_handler() -> StatusCode {
        StatusCode::OK
    }
}

#[cfg(test)]
mod tests {
    use crate::axum::extract::scheme::Scheme;
    use crate::lnurl::pay::state::LnUrlPayState;
    use crate::lnurl::service::LnUrlBalancerService;
    use async_trait::async_trait;
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use chrono::{Duration, Utc};
    use std::collections::HashSet;
    use switchgear_components::offer::memory::MemoryOfferStore;
    use switchgear_service_api::balance::LnBalancer;
    use switchgear_service_api::lnurl::{LnUrlInvoice, LnUrlOffer, LnUrlOfferMetadata};
    use switchgear_service_api::offer::{
        Offer, OfferMetadata, OfferMetadataSparse, OfferMetadataStore, OfferRecord,
        OfferRecordSparse, OfferStore,
    };
    use switchgear_service_api::service::HasServiceErrorSource;
    use uuid::Uuid;

    // Mock LnBalancer implementation
    #[derive(Debug, Clone)]
    pub struct MockLnBalancer {
        should_fail: bool,
        should_fail_upstream: bool,
        invoice_response: String,
        captured_expiry: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
    }

    impl MockLnBalancer {
        pub fn new() -> Self {
            Self {
                should_fail: false,
                should_fail_upstream: false,
                invoice_response: "lnbc1000n1pjdkqs0pp5...".to_string(),
                captured_expiry: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }
        }

        pub fn with_failure() -> Self {
            Self {
                should_fail: true,
                should_fail_upstream: false,
                invoice_response: String::new(),
                captured_expiry: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }
        }

        pub fn with_invoice(invoice: &str) -> Self {
            Self {
                should_fail: false,
                should_fail_upstream: false,
                invoice_response: invoice.to_string(),
                captured_expiry: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }
        }

        pub fn captured_expiry(&self) -> Option<u64> {
            *self.captured_expiry.lock().unwrap()
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum MockLnBalancerCombinedError {
        #[error("Mock LnBalancer internal error")]
        Internal,
        #[error("Mock LnBalancer upstream error")]
        Upstream,
    }

    impl HasServiceErrorSource for MockLnBalancerCombinedError {
        fn get_service_error_source(&self) -> switchgear_service_api::service::ServiceErrorSource {
            match self {
                MockLnBalancerCombinedError::Internal => {
                    switchgear_service_api::service::ServiceErrorSource::Internal
                }
                MockLnBalancerCombinedError::Upstream => {
                    switchgear_service_api::service::ServiceErrorSource::Upstream
                }
            }
        }
    }

    #[async_trait]
    impl LnBalancer for MockLnBalancer {
        type Error = MockLnBalancerCombinedError;

        async fn get_invoice(
            &self,
            _offer: &Offer,
            _amount_msat: u64,
            expiry_secs: u64,
            _key: &[u8],
        ) -> Result<String, Self::Error> {
            // Capture the expiry parameter for testing
            *self.captured_expiry.lock().unwrap() = Some(expiry_secs);

            if self.should_fail_upstream {
                Err(MockLnBalancerCombinedError::Upstream)
            } else if self.should_fail {
                Err(MockLnBalancerCombinedError::Internal)
            } else {
                Ok(self.invoice_response.clone())
            }
        }

        async fn health(&self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    // Test helper functions
    fn create_test_offer_and_metadata() -> (OfferRecord, OfferMetadata) {
        // Create metadata first
        let metadata_id = Uuid::new_v4();
        let metadata = OfferMetadata {
            id: metadata_id,
            partition: "default".to_string(),
            metadata: OfferMetadataSparse {
                text: "Test offer".to_string(),
                long_text: Some("This is a test offer for LNURL Pay".to_string()),
                image: None,
                identifier: None,
            },
        };

        let offer = OfferRecord {
            partition: "default".to_string(),
            id: Uuid::new_v4(),
            offer: OfferRecordSparse {
                max_sendable: 1000000,
                min_sendable: 1000,
                metadata_id,
                timestamp: Utc::now() - Duration::hours(1),
                expires: Some(Utc::now() + Duration::hours(1)),
            },
        };

        (offer, metadata)
    }

    fn create_test_offer() -> OfferRecord {
        let (offer, _) = create_test_offer_and_metadata();
        offer
    }

    async fn create_test_server_with_offer(offer: OfferRecord) -> TestServer {
        create_test_server_with_offer_and_expiry(offer, 3600).await
    }

    async fn create_test_server_with_offer_and_expiry(
        offer: OfferRecord,
        expiry: u64,
    ) -> TestServer {
        let (server, _) =
            create_test_server_with_offer_and_expiry_and_balancer(offer, expiry).await;
        server
    }

    async fn create_test_server_with_offer_and_expiry_and_balancer(
        offer: OfferRecord,
        expiry: u64,
    ) -> (TestServer, MockLnBalancer) {
        create_test_server_with_offer_and_expiry_and_balancer_and_partitions(offer, expiry, None)
            .await
    }

    async fn create_test_server_with_offer_and_expiry_and_balancer_and_partitions(
        offer: OfferRecord,
        expiry: u64,
        partitions: Option<HashSet<String>>,
    ) -> (TestServer, MockLnBalancer) {
        let partition = offer.partition.clone();
        let offer_provider = MemoryOfferStore::default();

        // Create metadata for the offer
        let metadata = OfferMetadata {
            id: offer.offer.metadata_id,
            partition: offer.partition.clone(),
            metadata: OfferMetadataSparse {
                text: "Test offer".to_string(),
                long_text: Some("This is a test offer for LNURL Pay".to_string()),
                image: None,
                identifier: None,
            },
        };
        offer_provider.put_metadata(metadata).await.unwrap();
        offer_provider.put_offer(offer).await.unwrap();

        let balancer = MockLnBalancer::new();
        let partitions = partitions.unwrap_or_else(|| HashSet::from([partition.clone()]));
        let state = LnUrlPayState::new(
            partitions,
            offer_provider,
            balancer.clone(),
            expiry,
            Scheme("http".to_string()),
            Default::default(),
            Default::default(),
            8,
            255u8,
            0u8,
        );

        let app = LnUrlBalancerService::router(state);
        let server = TestServer::new(app).unwrap();
        (server, balancer)
    }

    fn create_empty_test_server() -> TestServer {
        let offer_provider = MemoryOfferStore::default();
        let balancer = MockLnBalancer::new();
        let state = LnUrlPayState::new(
            HashSet::from(["default".to_string()]),
            offer_provider,
            balancer,
            3600,
            Scheme("http".to_string()),
            Default::default(),
            Default::default(),
            8,
            255u8,
            0u8,
        );

        let app = LnUrlBalancerService::router(state);
        TestServer::new(app).unwrap()
    }

    async fn create_test_server_with_failing_balancer(offer: OfferRecord) -> TestServer {
        let partition = offer.partition.clone();
        let offer_provider = MemoryOfferStore::default();

        // Create metadata for the offer
        let metadata = OfferMetadata {
            id: offer.offer.metadata_id,
            partition: offer.partition.clone(),
            metadata: OfferMetadataSparse {
                text: "Test offer".to_string(),
                long_text: Some("This is a test offer for LNURL Pay".to_string()),
                image: None,
                identifier: None,
            },
        };
        offer_provider.put_metadata(metadata).await.unwrap();
        offer_provider.put_offer(offer).await.unwrap();

        let balancer = MockLnBalancer::with_failure();
        let state = LnUrlPayState::new(
            HashSet::from([partition.clone()]),
            offer_provider,
            balancer,
            3600,
            Scheme("http".to_string()),
            Default::default(),
            Default::default(),
            8,
            255u8,
            0u8,
        );

        let app = LnUrlBalancerService::router(state);
        TestServer::new(app).unwrap()
    }

    // Health Check Tests

    #[tokio::test]
    async fn health_check_when_called_then_returns_ok() {
        let server = create_empty_test_server();
        let response = server.get("/health").await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.text(), "");
    }

    // Offer Endpoint Tests

    #[tokio::test]
    async fn get_offer_when_exists_then_returns_lnurl_pay_request() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer.clone()).await;

        let response = server.get(&format!("/offers/default/{offer_id}")).await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify response structure (LNURL Pay spec)
        let offer: LnUrlOffer = response.json();
        assert!(
            offer.callback.host_str().unwrap() == "127.0.0.1"
                || offer.callback.host_str().unwrap() == "localhost"
        );
        assert!(offer
            .callback
            .path()
            .contains(&format!("/offers/default/{offer_id}/invoice")));
        assert_eq!(offer.max_sendable, test_offer.offer.max_sendable);
        assert_eq!(offer.min_sendable, test_offer.offer.min_sendable);

        // Deserialize the metadata string to verify it matches our test data
        let metadata: LnUrlOfferMetadata = serde_json::from_str(&offer.metadata).unwrap();
        assert_eq!(metadata.0.text, "Test offer");
        assert_eq!(
            metadata.0.long_text,
            Some("This is a test offer for LNURL Pay".to_string())
        );
    }

    async fn create_test_server_with_scheme(
        offer: OfferRecord,
        scheme: &str,
    ) -> (TestServer, Uuid) {
        let partition = offer.partition.clone();
        let offer_provider = MemoryOfferStore::default();
        let metadata = OfferMetadata {
            id: offer.offer.metadata_id,
            partition: offer.partition.clone(),
            metadata: OfferMetadataSparse {
                text: "Test offer".to_string(),
                long_text: Some("This is a test offer for LNURL Pay".to_string()),
                image: None,
                identifier: None,
            },
        };
        offer_provider.put_metadata(metadata).await.unwrap();
        offer_provider.put_offer(offer.clone()).await.unwrap();

        let balancer = MockLnBalancer::new();
        let state = LnUrlPayState::new(
            HashSet::from([partition.clone()]),
            offer_provider,
            balancer,
            3600,
            Scheme(scheme.to_string()),
            Default::default(),
            Default::default(),
            8,
            255u8,
            0u8,
        );

        let app = LnUrlBalancerService::router(state);
        let server = TestServer::new(app).unwrap();
        (server, offer.id)
    }

    #[tokio::test]
    async fn get_offer_callback_uses_default_scheme() {
        let test_offer = create_test_offer();
        let (server, offer_id) = create_test_server_with_scheme(test_offer, "https").await;

        let response = server.get(&format!("/offers/default/{offer_id}")).await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let offer: LnUrlOffer = response.json();
        assert_eq!(offer.callback.scheme(), "https");
    }

    #[tokio::test]
    async fn get_offer_callback_respects_x_forwarded_proto_header() {
        let test_offer = create_test_offer();
        let (server, offer_id) = create_test_server_with_scheme(test_offer, "http").await;

        let response = server
            .get(&format!("/offers/default/{offer_id}"))
            .add_header("X-Forwarded-Proto", "https")
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let offer: LnUrlOffer = response.json();
        assert_eq!(offer.callback.scheme(), "https");
    }

    #[tokio::test]
    async fn get_offer_callback_respects_forwarded_header() {
        let test_offer = create_test_offer();
        let (server, offer_id) = create_test_server_with_scheme(test_offer, "http").await;

        let response = server
            .get(&format!("/offers/default/{offer_id}"))
            .add_header("Forwarded", "proto=wss;host=example.com")
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let offer: LnUrlOffer = response.json();
        assert_eq!(offer.callback.scheme(), "wss");
    }

    #[tokio::test]
    async fn get_offer_callback_forwarded_header_takes_precedence() {
        let test_offer = create_test_offer();
        let (server, offer_id) = create_test_server_with_scheme(test_offer, "http").await;

        let response = server
            .get(&format!("/offers/default/{offer_id}"))
            .add_header("Forwarded", "proto=wss")
            .add_header("X-Forwarded-Proto", "https")
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let offer: LnUrlOffer = response.json();
        assert_eq!(offer.callback.scheme(), "wss");
    }

    #[tokio::test]
    async fn get_offer_cache_headers_when_expires_in_30_minutes() {
        let mut test_offer = create_test_offer();
        // Set offer to expire in 30 minutes
        test_offer.offer.expires = Some(Utc::now() + Duration::minutes(30));
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server.get(&format!("/offers/default/{offer_id}")).await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Check Cache-Control header
        let cache_control = response.header("cache-control");
        let cache_control_str = cache_control.to_str().unwrap();
        assert!(cache_control_str.starts_with("public, max-age="));
        let max_age: u64 = cache_control_str
            .strip_prefix("public, max-age=")
            .unwrap()
            .parse()
            .unwrap();
        // Should be between 1799 and 1800 seconds (30 minutes, allowing for timing)
        assert!((1799..=1800).contains(&max_age));

        // Check Expires header
        let expires_header = response.header("expires");
        let expires_header_str = expires_header.to_str().unwrap();
        assert!(!expires_header_str.is_empty());
        assert!(expires_header_str.ends_with(" GMT"));
    }

    #[tokio::test]
    async fn get_offer_cache_headers_when_expires_in_5_minutes() {
        let mut test_offer = create_test_offer();
        // Set offer to expire in 5 minutes
        test_offer.offer.expires = Some(Utc::now() + Duration::minutes(5));
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server.get(&format!("/offers/default/{offer_id}")).await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Check Cache-Control header
        let cache_control = response.header("cache-control");
        let cache_control_str = cache_control.to_str().unwrap();
        assert!(cache_control_str.starts_with("public, max-age="));
        let max_age: u64 = cache_control_str
            .strip_prefix("public, max-age=")
            .unwrap()
            .parse()
            .unwrap();
        // Should be between 299 and 300 seconds (5 minutes, allowing for timing)
        assert!((299..=300).contains(&max_age));

        // Check Expires header
        let expires_header = response.header("expires");
        let expires_header_str = expires_header.to_str().unwrap();
        assert!(!expires_header_str.is_empty());
        assert!(expires_header_str.ends_with(" GMT"));
    }

    #[tokio::test]
    async fn get_offer_no_cache_headers_when_expires_is_none() {
        let mut test_offer = create_test_offer();
        // Set offer expires to None (no expiration)
        test_offer.offer.expires = None;
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server.get(&format!("/offers/default/{offer_id}")).await;
        assert_eq!(response.status_code(), StatusCode::OK);

        // Check no-cache headers are present
        // Cache-Control header should be "no-store, no-cache, must-revalidate"
        let cache_control = response.header("cache-control");
        let cache_control_str = cache_control.to_str().unwrap();
        assert_eq!(cache_control_str, "no-store, no-cache, must-revalidate");

        // Expires header should be "Thu, 01 Jan 1970 00:00:00 GMT"
        let expires_header = response.header("expires");
        let expires_header_str = expires_header.to_str().unwrap();
        assert_eq!(expires_header_str, "Thu, 01 Jan 1970 00:00:00 GMT");

        // Pragma header should be "no-cache"
        let pragma_header = response.header("pragma");
        let pragma_header_str = pragma_header.to_str().unwrap();
        assert_eq!(pragma_header_str, "no-cache");
    }

    #[tokio::test]
    async fn get_offer_when_not_exists_then_returns_not_found() {
        let server = create_empty_test_server();
        let non_existent_id = Uuid::new_v4();

        let response = server
            .get(&format!("/offers/default/{non_existent_id}"))
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_offer_when_expired_then_returns_gone() {
        let mut test_offer = create_test_offer();
        // Make the offer expired
        test_offer.offer.expires = Some(Utc::now() - Duration::hours(1));
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server.get(&format!("/offers/default/{offer_id}")).await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_offer_when_invalid_uuid_then_returns_not_found() {
        let server = create_empty_test_server();

        let response = server.get("/offers/default/invalid-uuid").await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    // Invoice Endpoint Tests

    #[tokio::test]
    async fn get_invoice_when_valid_request_then_returns_invoice() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server
            .get(&format!("/offers/default/{offer_id}/invoice?amount=500000",))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify response structure (LNURL Pay spec)
        let invoice: LnUrlInvoice = response.json();
        assert!(invoice.pr.starts_with("lnbc"));
        assert_eq!(invoice.routes.len(), 0);
    }

    #[tokio::test]
    async fn get_invoice_when_offer_not_exists_then_returns_not_found() {
        let server = create_empty_test_server();
        let non_existent_id = Uuid::new_v4();

        let response = server
            .get(&format!(
                "/offers/default/{non_existent_id}/invoice?amount=500000",
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_invoice_when_amount_missing_then_returns_bad_request() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server
            .get(&format!("/offers/default/{offer_id}/invoice"))
            .await;

        // Axum query parameter validation would result in 400 for missing required parameter
        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_invoice_when_amount_valid_then_passes_to_balancer() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer.clone()).await;

        // Test with amount within range
        let response = server
            .get(&format!(
                "/offers/default/{}/invoice?amount={}",
                offer_id, test_offer.offer.min_sendable
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let invoice: LnUrlInvoice = response.json();
        assert!(invoice.pr.starts_with("lnbc"));
        assert_eq!(invoice.routes.len(), 0);
    }

    #[tokio::test]
    async fn get_invoice_when_amount_outside_range_then_returns_bad_request() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer.clone()).await;

        // Test amount above max_sendable
        let response = server
            .get(&format!(
                "/offers/default/{}/invoice?amount={}",
                offer_id,
                test_offer.offer.max_sendable + 1
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);

        // Test amount below min_sendable
        let response = server
            .get(&format!(
                "/offers/default/{}/invoice?amount={}",
                offer_id,
                test_offer.offer.min_sendable - 1
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_invoice_when_invalid_amount_then_returns_bad_request() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server
            .get(&format!(
                "/offers/default/{offer_id}/invoice?amount=invalid",
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_invoice_when_expired_offer_then_returns_not_found() {
        let mut test_offer = create_test_offer();
        // Make the offer expired
        test_offer.offer.expires = Some(Utc::now() - Duration::hours(1));
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let response = server
            .get(&format!("/offers/default/{offer_id}/invoice?amount=500000",))
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_invoice_when_balancer_fails_then_returns_internal_server_error() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_failing_balancer(test_offer).await;

        let response = server
            .get(&format!("/offers/default/{offer_id}/invoice?amount=500000",))
            .await;

        assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn get_invoice_when_invalid_uuid_then_returns_not_found() {
        let server = create_empty_test_server();

        let response = server
            .get("/offers/default/invalid-uuid/invoice?amount=500000")
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    // Additional edge case tests

    #[tokio::test]
    async fn get_invoice_with_custom_invoice_response() {
        let test_offer = create_test_offer();
        let offer_provider = MemoryOfferStore::default();

        let partition = test_offer.partition.clone();

        // Create metadata for the offer
        let metadata = OfferMetadata {
            id: test_offer.offer.metadata_id,
            partition: test_offer.partition.clone(),
            metadata: OfferMetadataSparse {
                text: "Test offer".to_string(),
                long_text: Some("This is a test offer for LNURL Pay".to_string()),
                image: None,
                identifier: None,
            },
        };
        offer_provider.put_metadata(metadata).await.unwrap();
        offer_provider.put_offer(test_offer.clone()).await.unwrap();

        let custom_invoice = "lnbc5000n1pjdkqs0pp5custom...";
        let balancer = MockLnBalancer::with_invoice(custom_invoice);
        let state = LnUrlPayState::new(
            HashSet::from([partition]),
            offer_provider,
            balancer,
            3600,
            Scheme("http".to_string()),
            Default::default(),
            Default::default(),
            8,
            255u8,
            0u8,
        );
        let app = LnUrlBalancerService::router(state);
        let server = TestServer::new(app).unwrap();

        let offer_id = test_offer.id;

        let response = server
            .get(&format!("/offers/default/{offer_id}/invoice?amount=500000",))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let invoice: LnUrlInvoice = response.json();
        assert_eq!(invoice.pr, custom_invoice);
    }

    #[tokio::test]
    async fn get_invoice_when_valid_request_then_uses_configured_expiry() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let expected_expiry = 7200u64; // 2 hours
        let (server, balancer) =
            create_test_server_with_offer_and_expiry_and_balancer(test_offer, expected_expiry)
                .await;

        let response = server
            .get(&format!("/offers/default/{offer_id}/invoice?amount=500000",))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify that the balancer received the correct expiry value
        assert_eq!(balancer.captured_expiry(), Some(expected_expiry));
    }

    // Bech32 Endpoint Tests

    #[tokio::test]
    async fn get_bech32_when_valid_offer_then_returns_bech32_string() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let request_url = format!("/offers/default/{offer_id}");
        let request_url_bech32 = format!("{request_url}/bech32");

        let response = server.get(&request_url_bech32).await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(
            response.header("content-type").to_str().unwrap(),
            "text/plain; charset=utf-8"
        );

        let bech32_str = response.text();
        let (hrp, data) = bech32::decode(&bech32_str).unwrap();
        assert_eq!(hrp.to_string().to_uppercase(), "LNURL");

        let decoded_bytes: Vec<u8> = data.into_iter().collect();
        let decoded_url = String::from_utf8(decoded_bytes).unwrap();
        assert_eq!(format!("http://localhost{request_url}"), decoded_url);
    }

    #[tokio::test]
    async fn get_bech32_qr_when_valid_offer_then_returns_png_image() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;

        let request_url = format!("/offers/default/{offer_id}");
        let request_url_bech32_qr = format!("{request_url}/bech32/qr");

        let response = server.get(&request_url_bech32_qr).await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(
            response.header("content-type").to_str().unwrap(),
            "image/png"
        );

        let png_bytes = response.as_bytes();

        // Decode the QR code from the PNG to verify content
        use png::Decoder;
        use std::io::Cursor;

        let decoder = Decoder::new(Cursor::new(&png_bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![
            0;
            reader
                .output_buffer_size()
                .expect("PNG has no output buffer size")
        ];
        let info = reader.next_frame(&mut buf).unwrap();
        let bytes = &buf[..info.buffer_size()];

        // Convert to rqrr-compatible image
        use rqrr::PreparedImage;
        let img = image::GrayImage::from_raw(info.width, info.height, bytes.to_vec())
            .expect("Failed to create image from PNG data");

        let mut prepared = PreparedImage::prepare(img);
        let grids = prepared.detect_grids();
        assert!(!grids.is_empty(), "Should detect at least one QR code");

        let (_, content) = grids[0].decode().unwrap();

        let (hrp, data) = bech32::decode(&content).unwrap();
        assert_eq!(hrp.to_string().to_uppercase(), "LNURL");

        let decoded_bytes: Vec<u8> = data.into_iter().collect();
        let decoded_url = String::from_utf8(decoded_bytes).unwrap();
        assert_eq!(format!("http://localhost{request_url}"), decoded_url);
    }

    #[tokio::test]
    async fn get_offer_when_invalid_partition_then_returns_not_found() {
        let test_offer = create_test_offer();
        let partition = test_offer.partition.clone();
        let offer_id = test_offer.id;

        let (server, _) = create_test_server_with_offer_and_expiry_and_balancer_and_partitions(
            test_offer,
            3600,
            Some(["alternate-partition".to_string()].into()),
        )
        .await;

        let response = server.get(&format!("/offers/{partition}/{offer_id}")).await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_invoice_when_invalid_partition_then_returns_not_found() {
        let test_offer = create_test_offer();
        let partition = test_offer.partition.clone();
        let offer_id = test_offer.id;

        let (server, _) = create_test_server_with_offer_and_expiry_and_balancer_and_partitions(
            test_offer,
            3600,
            Some(["alternate-partition".to_string()].into()),
        )
        .await;

        let response = server
            .get(&format!(
                "/offers/{partition}/{offer_id}/invoice?amount=500000"
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_bech32_when_invalid_partition_then_returns_not_found() {
        let test_offer = create_test_offer();
        let partition = test_offer.partition.clone();
        let offer_id = test_offer.id;

        let (server, _) = create_test_server_with_offer_and_expiry_and_balancer_and_partitions(
            test_offer,
            3600,
            Some(["alternate-partition".to_string()].into()),
        )
        .await;

        let response = server
            .get(&format!("/offers/{partition}/{offer_id}/bech32"))
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_bech32_qr_when_invalid_partition_then_returns_not_found() {
        let test_offer = create_test_offer();
        let partition = test_offer.partition.clone();
        let offer_id = test_offer.id;

        let (server, _) = create_test_server_with_offer_and_expiry_and_balancer_and_partitions(
            test_offer,
            3600,
            Some(["alternate-partition".to_string()].into()),
        )
        .await;

        let response = server
            .get(&format!("/offers/{partition}/{offer_id}/bech32/qr"))
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }
}
