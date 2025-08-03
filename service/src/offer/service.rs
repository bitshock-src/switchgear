use crate::api::offer::{OfferMetadataStore, OfferStore};
use crate::api::service::StatusCode;
use crate::axum::middleware::BearerTokenAuthLayer;
use crate::offer::auth::OfferBearerTokenValidator;
use crate::offer::handler::OfferHandlers;
use crate::offer::state::OfferState;
use axum::routing::{delete, get, post, put};
use axum::Router;

#[derive(Debug)]
pub struct OfferService;

impl OfferService {
    pub fn router<S, M>(state: OfferState<S, M>) -> Router
    where
        S: OfferStore + Clone + Send + Sync + 'static,
        M: OfferMetadataStore + Clone + Send + Sync + 'static,
    {
        Router::new()
            .route("/offers/{partition}/{id}", get(OfferHandlers::get_offer))
            .route("/offers/{partition}/{id}", put(OfferHandlers::put_offer))
            .route(
                "/offers/{partition}/{id}",
                delete(OfferHandlers::delete_offer),
            )
            .route("/offers/{partition}", get(OfferHandlers::get_offers))
            .route("/offers", post(OfferHandlers::post_offer))
            .route(
                "/metadata/{partition}/{id}",
                get(OfferHandlers::get_metadata),
            )
            .route(
                "/metadata/{partition}/{id}",
                put(OfferHandlers::put_metadata),
            )
            .route(
                "/metadata/{partition}/{id}",
                delete(OfferHandlers::delete_metadata),
            )
            .route(
                "/metadata/{partition}",
                get(OfferHandlers::get_all_metadata),
            )
            .route("/metadata", post(OfferHandlers::post_metadata))
            .layer(BearerTokenAuthLayer::new(
                OfferBearerTokenValidator::new(state.auth_authority().clone()),
                "offer",
            ))
            .route("/health", get(Self::health_check_handler))
            .with_state(state)
    }

    async fn health_check_handler() -> StatusCode {
        StatusCode::OK
    }
}

#[cfg(test)]
mod tests {
    use crate::api::offer::{
        OfferMetadata, OfferMetadataIdentifier, OfferMetadataImage, OfferMetadataSparse,
        OfferMetadataStore, OfferRecord, OfferRecordSparse, OfferStore,
    };
    use crate::components::offer::memory::MemoryOfferStore;
    use crate::offer::service::OfferService;
    use crate::offer::state::OfferState;
    use crate::{OfferAudience, OfferClaims};
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::EncodePrivateKey;
    use p256::pkcs8::EncodePublicKey;
    use rand::thread_rng;
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;

    fn create_test_offer() -> OfferRecord {
        OfferRecord {
            partition: "default".to_string(),
            id: Uuid::new_v4(),
            offer: OfferRecordSparse {
                max_sendable: 1000000,
                min_sendable: 1000,
                metadata_id: Uuid::new_v4(),
                timestamp: Utc::now() - Duration::hours(1),
                expires: Some(Utc::now() + Duration::hours(1)),
            },
        }
    }

    fn create_test_metadata() -> OfferMetadata {
        OfferMetadata {
            id: Uuid::new_v4(),
            partition: "default".to_string(),
            metadata: OfferMetadataSparse {
                text: "Test offer".to_string(),
                long_text: Some("This is a test offer description".to_string()),
                image: Some(OfferMetadataImage::Png(vec![0x89, 0x50, 0x4E, 0x47])),
                identifier: Some(OfferMetadataIdentifier::Email(
                    "test@example.com".parse().unwrap(),
                )),
            },
        }
    }

    async fn create_test_server_with_offer(offer: OfferRecord) -> TestServerWithAuthorization {
        let mut rng = thread_rng();
        let private_key = SigningKey::random(&mut rng);
        let public_key = *private_key.verifying_key();

        let private_key = private_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::default())
            .unwrap();
        let encoding_key = EncodingKey::from_ec_pem(private_key.as_bytes()).unwrap();

        let public_key = public_key
            .to_public_key_pem(p256::pkcs8::LineEnding::default())
            .unwrap();
        let decoding_key = DecodingKey::from_ec_pem(public_key.as_bytes()).unwrap();

        let header = Header::new(Algorithm::ES256);
        let claims = OfferClaims {
            aud: OfferAudience::Offer,
            exp: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600) as usize,
        };
        let authorization = encode(&header, &claims, &encoding_key).unwrap();

        let offer_store = MemoryOfferStore::default();
        offer_store.put_offer(offer).await.unwrap();
        let metadata_store = MemoryOfferStore::default();
        let state = OfferState::new(offer_store, metadata_store, decoding_key);

        let app = OfferService::router(state);
        TestServerWithAuthorization {
            server: TestServer::new(app).unwrap(),
            authorization,
        }
    }

    async fn create_test_server_with_metadata(
        metadata: OfferMetadata,
    ) -> TestServerWithAuthorization {
        let mut rng = thread_rng();
        let private_key = SigningKey::random(&mut rng);
        let public_key = *private_key.verifying_key();

        let private_key = private_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::default())
            .unwrap();
        let encoding_key = EncodingKey::from_ec_pem(private_key.as_bytes()).unwrap();

        let public_key = public_key
            .to_public_key_pem(p256::pkcs8::LineEnding::default())
            .unwrap();
        let decoding_key = DecodingKey::from_ec_pem(public_key.as_bytes()).unwrap();

        let header = Header::new(Algorithm::ES256);
        let claims = OfferClaims {
            aud: OfferAudience::Offer,
            exp: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600) as usize,
        };
        let authorization = encode(&header, &claims, &encoding_key).unwrap();

        let offer_store = MemoryOfferStore::default();
        let metadata_store = MemoryOfferStore::default();
        metadata_store.put_metadata(metadata).await.unwrap();
        let state = OfferState::new(offer_store, metadata_store, decoding_key);

        let app = OfferService::router(state);
        TestServerWithAuthorization {
            server: TestServer::new(app).unwrap(),
            authorization,
        }
    }

    fn create_empty_test_server() -> TestServerWithAuthorization {
        let mut rng = thread_rng();
        let private_key = SigningKey::random(&mut rng);
        let public_key = *private_key.verifying_key();

        let private_key = private_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::default())
            .unwrap();
        let encoding_key = EncodingKey::from_ec_pem(private_key.as_bytes()).unwrap();

        let public_key = public_key
            .to_public_key_pem(p256::pkcs8::LineEnding::default())
            .unwrap();
        let decoding_key = DecodingKey::from_ec_pem(public_key.as_bytes()).unwrap();

        let header = Header::new(Algorithm::ES256);
        let claims = OfferClaims {
            aud: OfferAudience::Offer,
            exp: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600) as usize,
        };
        let authorization = encode(&header, &claims, &encoding_key).unwrap();

        let offer_store = MemoryOfferStore::default();
        let metadata_store = MemoryOfferStore::default();
        let state = OfferState::new(offer_store, metadata_store, decoding_key);

        let app = OfferService::router(state);
        TestServerWithAuthorization {
            server: TestServer::new(app).unwrap(),
            authorization,
        }
    }

    struct TestServerWithAuthorization {
        server: TestServer,
        authorization: String,
    }

    // Health Check Tests

    #[tokio::test]
    async fn health_check_when_called_then_returns_ok() {
        let server = create_empty_test_server();
        let response = server.server.get("/health").await;

        assert_eq!(response.status_code(), StatusCode::OK);
    }

    // Offer Tests

    #[tokio::test]
    async fn delete_offer_when_exists_then_removes_and_second_delete_not_found() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer).await;
        let response = server
            .server
            .delete(&format!("/offers/default/{offer_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

        // Delete again and assert NOT_FOUND
        let response = server
            .server
            .delete(&format!("/offers/default/{offer_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_offer_when_not_exists_then_returns_not_found() {
        let server = create_empty_test_server();
        let non_existent_id = Uuid::new_v4();
        let response = server
            .server
            .delete(&format!("/offers/default/{non_existent_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_offer_when_exists_then_returns_resource() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer.clone()).await;
        let response = server
            .server
            .get(&format!("/offers/default/{offer_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let returned_offer: OfferRecord = response.json();
        assert_eq!(returned_offer.id, offer_id);
        assert_eq!(
            returned_offer.offer.max_sendable,
            test_offer.offer.max_sendable
        );
    }

    #[tokio::test]
    async fn get_offer_when_invalid_uuid_then_returns_not_found() {
        let server = create_empty_test_server();
        let response = server
            .server
            .get("/offers/default/invalid-uuid")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_offer_when_not_exists_then_returns_not_found() {
        let server = create_empty_test_server();
        let non_existent_id = Uuid::new_v4();
        let response = server
            .server
            .get(&format!("/offers/default/{non_existent_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_offers_when_empty_then_returns_empty_list() {
        let server = create_empty_test_server();
        let response = server
            .server
            .get("/offers/default")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let offers: Vec<OfferRecord> = response.json();
        assert!(offers.is_empty());
    }

    #[tokio::test]
    async fn get_offers_when_exists_then_returns_list() {
        let test_offer = create_test_offer();
        let server = create_test_server_with_offer(test_offer).await;
        let response = server
            .server
            .get("/offers/default")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let offers: Vec<OfferRecord> = response.json();
        assert_eq!(offers.len(), 1);
    }

    #[tokio::test]
    async fn post_offer_when_new_then_creates_and_returns_location() {
        let server = create_empty_test_server();
        let test_offer = create_test_offer();
        let response = server
            .server
            .post("/offers")
            .authorization_bearer(server.authorization.clone())
            .json(&test_offer)
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        assert!(response.headers().contains_key("location"));

        // Get the location header and make a GET request
        let location = response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        let get_response = server
            .server
            .get(&format!("/offers/{location}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(get_response.status_code(), StatusCode::OK);
        let returned_offer: OfferRecord = get_response.json();
        assert_eq!(returned_offer.id, test_offer.id);
        assert_eq!(
            returned_offer.offer.max_sendable,
            test_offer.offer.max_sendable
        );
        assert_eq!(
            returned_offer.offer.min_sendable,
            test_offer.offer.min_sendable
        );
        assert_eq!(
            returned_offer.offer.metadata_id,
            test_offer.offer.metadata_id
        );

        // Post again and assert CONFLICT
        let response = server
            .server
            .post("/offers")
            .json(&test_offer)
            .authorization_bearer(server.authorization.clone())
            .await;
        assert_eq!(response.status_code(), StatusCode::CONFLICT);
        assert!(response.headers().contains_key("location"));
    }

    #[tokio::test]
    async fn put_offer_when_exists_then_updates_no_content() {
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let server = create_test_server_with_offer(test_offer.clone()).await;

        let mut updated_offer = test_offer.offer.clone();
        updated_offer.max_sendable = 2000000;

        let response = server
            .server
            .put(&format!("/offers/default/{offer_id}"))
            .authorization_bearer(server.authorization.clone())
            .json(&updated_offer)
            .await;

        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn put_offer_when_new_then_created() {
        let server = create_empty_test_server();
        let test_offer = create_test_offer();
        let offer_id = test_offer.id;
        let response = server
            .server
            .put(&format!("/offers/default/{offer_id}"))
            .authorization_bearer(server.authorization.clone())
            .json(&test_offer.offer)
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
    }

    // Metadata Tests

    #[tokio::test]
    async fn delete_metadata_when_exists_then_removes_and_second_delete_not_found() {
        let test_metadata = create_test_metadata();
        let metadata_id = test_metadata.id;
        let server = create_test_server_with_metadata(test_metadata).await;
        let response = server
            .server
            .delete(&format!("/metadata/default/{metadata_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

        // Delete again and assert NOT_FOUND
        let response = server
            .server
            .delete(&format!("/metadata/default/{metadata_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_metadata_when_not_exists_then_returns_not_found() {
        let server = create_empty_test_server();
        let non_existent_id = Uuid::new_v4();
        let response = server
            .server
            .delete(&format!("/metadata/default/{non_existent_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_metadata_when_exists_then_returns_resource() {
        let test_metadata = create_test_metadata();
        let metadata_id = test_metadata.id;
        let server = create_test_server_with_metadata(test_metadata.clone()).await;
        let response = server
            .server
            .get(&format!("/metadata/default/{metadata_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let returned_metadata: OfferMetadata = response.json();
        assert_eq!(returned_metadata.id, metadata_id);
        assert_eq!(returned_metadata.metadata.text, test_metadata.metadata.text);
    }

    #[tokio::test]
    async fn get_metadata_when_not_exists_then_returns_not_found() {
        let server = create_empty_test_server();
        let non_existent_id = Uuid::new_v4();
        let response = server
            .server
            .get(&format!("/metadata/default/{non_existent_id}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_all_metadata_when_empty_then_returns_empty_list() {
        let server = create_empty_test_server();
        let response = server
            .server
            .get("/metadata/default")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let metadata: Vec<OfferMetadata> = response.json();
        assert!(metadata.is_empty());
    }

    #[tokio::test]
    async fn get_all_metadata_when_exists_then_returns_list() {
        let test_metadata = create_test_metadata();
        let server = create_test_server_with_metadata(test_metadata).await;
        let response = server
            .server
            .get("/metadata/default")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let metadata: Vec<OfferMetadata> = response.json();
        assert_eq!(metadata.len(), 1);
    }

    #[tokio::test]
    async fn post_metadata_when_new_then_creates_and_returns_location() {
        let server = create_empty_test_server();
        let test_metadata = create_test_metadata();
        let response = server
            .server
            .post("/metadata")
            .authorization_bearer(server.authorization.clone())
            .json(&test_metadata)
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        assert!(response.headers().contains_key("location"));

        // Get the location header and make a GET request
        let location = response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        let get_response = server
            .server
            .get(&format!("/metadata/{location}"))
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(get_response.status_code(), StatusCode::OK);
        let returned_metadata: OfferMetadata = get_response.json();
        assert_eq!(returned_metadata.id, test_metadata.id);
        assert_eq!(returned_metadata.metadata.text, test_metadata.metadata.text);
        assert_eq!(
            returned_metadata.metadata.long_text,
            test_metadata.metadata.long_text
        );
        assert_eq!(
            returned_metadata.metadata.image,
            test_metadata.metadata.image
        );
        assert_eq!(
            returned_metadata.metadata.identifier,
            test_metadata.metadata.identifier
        );

        // Post again and assert CONFLICT
        let response = server
            .server
            .post("/metadata")
            .json(&test_metadata)
            .authorization_bearer(server.authorization.clone())
            .await;
        assert_eq!(response.status_code(), StatusCode::CONFLICT);
        assert!(response.headers().contains_key("location"));
    }

    #[tokio::test]
    async fn put_metadata_when_exists_then_updates_no_content() {
        let test_metadata = create_test_metadata();
        let metadata_id = test_metadata.id;
        let server = create_test_server_with_metadata(test_metadata.clone()).await;

        let mut updated_metadata = test_metadata.metadata.clone();
        updated_metadata.text = "Updated text".to_string();

        let response = server
            .server
            .put(&format!("/metadata/default/{metadata_id}"))
            .authorization_bearer(server.authorization.clone())
            .json(&updated_metadata)
            .await;

        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn put_metadata_when_new_then_created() {
        let server = create_empty_test_server();
        let test_metadata = create_test_metadata();
        let metadata_id = test_metadata.id;
        let response = server
            .server
            .put(&format!("/metadata/default/{metadata_id}"))
            .authorization_bearer(server.authorization.clone())
            .json(&test_metadata.metadata)
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn unauthorized() {
        let server = create_empty_test_server();
        let test_offer = create_test_offer();

        let response = server.server.post("/offers").json(&test_offer).await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server.server.get("/offers/default").await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server.server.put("/offers/default").json(&test_offer).await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server.server.delete("/offers/default").await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server.server.post("/metadata").json(&test_offer).await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server.server.get("/metadata/default").await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server
            .server
            .put("/metadata/default")
            .json(&test_offer)
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server.server.delete("/metadata/default").await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }
}
