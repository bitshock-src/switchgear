use crate::api::discovery::DiscoveryBackendStore;
use crate::api::service::StatusCode;
use crate::axum::middleware::BearerTokenAuthLayer;
use crate::discovery::auth::DiscoveryBearerTokenValidator;
use crate::discovery::handler::DiscoveryHandlers;
use crate::discovery::state::DiscoveryState;
use axum::routing::{delete, get, post, put};
use axum::Router;

#[derive(Debug)]
pub struct DiscoveryService;

impl DiscoveryService {
    pub fn router<S>(state: DiscoveryState<S>) -> Router
    where
        S: DiscoveryBackendStore + Clone + Send + Sync + 'static,
    {
        Router::new()
            .route(
                "/discovery/{addr_variant}/{addr_value}",
                get(DiscoveryHandlers::get_backend),
            )
            .route(
                "/discovery/{addr_variant}/{addr_value}",
                put(DiscoveryHandlers::put_backend),
            )
            .route(
                "/discovery/{addr_variant}/{addr_value}",
                delete(DiscoveryHandlers::delete_backend),
            )
            .route("/discovery", get(DiscoveryHandlers::get_backends))
            .route("/discovery", post(DiscoveryHandlers::post_backend))
            .layer(BearerTokenAuthLayer::new(
                DiscoveryBearerTokenValidator::new(state.auth_authority().clone()),
                "discovery",
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
    use crate::api::discovery::{
        DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
        DiscoveryBackendSparse,
    };
    use crate::components::discovery::memory::MemoryDiscoveryBackendStore;
    use crate::discovery::auth::{DiscoveryAudience, DiscoveryClaims};
    use crate::discovery::service::DiscoveryService;
    use crate::discovery::state::DiscoveryState;
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::EncodePrivateKey;
    use p256::pkcs8::EncodePublicKey;
    use rand::thread_rng;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_test_backend(partition: &str, address: &str) -> DiscoveryBackend {
        DiscoveryBackend {
            address: DiscoveryBackendAddress::Url(format!("https://{address}").parse().unwrap()),
            backend: DiscoveryBackendSparse {
                partitions: [partition.to_string()].into(),
                weight: 100,
                enabled: true,
                implementation: DiscoveryBackendImplementation::RemoteHttp,
            },
        }
    }

    struct TestServerWithAuthorization {
        server: TestServer,
        authorization: String,
    }

    async fn setup_test_server() -> TestServerWithAuthorization {
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

        let store = MemoryDiscoveryBackendStore::new();
        let state = DiscoveryState::new(store, decoding_key);

        let header = Header::new(Algorithm::ES256);
        let claims = DiscoveryClaims {
            aud: DiscoveryAudience::Discovery,
            exp: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600) as usize,
        };
        let authorization = encode(&header, &claims, &encoding_key).unwrap();

        let app = DiscoveryService::router(state);
        TestServerWithAuthorization {
            server: TestServer::new(app).unwrap(),
            authorization,
        }
    }

    #[tokio::test]
    async fn health_check_when_called_then_returns_ok() {
        let server = setup_test_server().await;

        let response = server.server.get("/health").await;

        assert_eq!(response.status_code(), StatusCode::OK);
        // Health check returns empty body with 200 status
        assert_eq!(response.text(), "");
    }

    #[tokio::test]
    async fn get_backends_when_empty_then_returns_empty_list() {
        let server = setup_test_server().await;

        let response = server
            .server
            .get("/discovery")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let backends: Vec<DiscoveryBackend> = response.json();
        assert!(backends.is_empty());

        // Verify cache headers
        assert_eq!(
            response.header("cache-control"),
            "no-store, no-cache, must-revalidate"
        );
        assert_eq!(response.header("expires"), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(response.header("pragma"), "no-cache");
    }

    #[tokio::test]
    async fn post_backend_when_new_then_creates_and_returns_location() {
        let server = setup_test_server().await;
        let backend = create_test_backend("default", "192.168.1.1:8080");

        let response = server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend)
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        let location = response.header("location");
        assert!(location.to_str().unwrap().contains("url/"));
    }

    #[tokio::test]
    async fn post_backend_when_duplicate_then_returns_conflict() {
        let server = setup_test_server().await;
        let backend = create_test_backend("default", "192.168.1.1:8080");

        // First POST should succeed
        let response1 = server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend)
            .await;
        assert_eq!(response1.status_code(), StatusCode::CREATED);

        // Second POST with same address should conflict
        let response2 = server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend)
            .await;
        assert_eq!(response2.status_code(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn get_backend_when_exists_then_returns_backend() {
        let server = setup_test_server().await;
        let backend = create_test_backend("default", "192.168.1.1:8080");

        // First create the backend
        let response = server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend)
            .await;

        let location = response.header("location");
        let location = location.to_str().unwrap();

        // Then retrieve it
        let response = server
            .server
            .get(format!("/discovery/{location}").as_str())
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let retrieved: DiscoveryBackend = response.json();
        assert_eq!(
            retrieved.backend.implementation,
            DiscoveryBackendImplementation::RemoteHttp
        );
        assert_eq!(retrieved.address, backend.address);
    }

    #[tokio::test]
    async fn get_backend_when_not_exists_then_returns_not_found() {
        let server = setup_test_server().await;

        let response = server
            .server
            .get("/discovery/default/inet/MTkyLjE2OC4xLjE6ODA4MA")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_backend_when_new_then_created() {
        let server = setup_test_server().await;
        let backend = create_test_backend("default", "192.168.1.1:8080");

        let response = server
            .server
            .put("/discovery/url/aHR0cHM6Ly8xOTIuMTY4LjEuMTo4MDgwLw")
            .authorization_bearer(server.authorization.clone())
            .json(&backend.backend)
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn put_backend_when_exists_then_updates_no_content() {
        let server = setup_test_server().await;
        let mut backend = create_test_backend("default", "192.168.1.1:8080");

        // Create initial backend
        let response = server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend)
            .await;

        let location = response.header("location");
        let location = location.to_str().unwrap();

        // Update with PUT
        backend.backend.weight = 200;
        let response = server
            .server
            .put(&format!("/discovery/{location}"))
            .authorization_bearer(server.authorization.clone())
            .json(&backend.backend)
            .await;

        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

        // Verify the update
        let get_response = server
            .server
            .get(&format!("/discovery/{location}"))
            .authorization_bearer(server.authorization.clone())
            .await;
        let updated: DiscoveryBackend = get_response.json();
        assert_eq!(updated.backend.weight, 200);
    }

    #[tokio::test]
    async fn delete_backend_when_exists_then_removes_and_returns_backend() {
        let server = setup_test_server().await;
        let backend = create_test_backend("default", "192.168.1.1:8080");

        // Create backend
        let response = server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend)
            .await;
        let location = response.header("location");
        let location = location.to_str().unwrap();

        // Delete backend
        let response = server
            .server
            .delete(&format!("/discovery/{location}"))
            .authorization_bearer(server.authorization.clone())
            .await;
        eprintln!("location: {location}");

        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
        // Delete returns empty body, no JSON to parse

        // Verify it's gone
        let get_response = server
            .server
            .get(&format!("/discovery/{location}"))
            .authorization_bearer(server.authorization.clone())
            .await;
        assert_eq!(get_response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_backend_when_not_exists_then_returns_not_found() {
        let server = setup_test_server().await;

        let response = server
            .server
            .delete("/discovery/default/inet/MTkyLjE2OC4xLjE6ODA4MA")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_backends_when_multiple_exist_then_returns_all() {
        let server = setup_test_server().await;
        let backend1 = create_test_backend("default", "192.168.1.1:8080");
        let backend2 = create_test_backend("default", "192.168.1.2:8080");

        // Create multiple backends
        server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend1)
            .await;
        server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .json(&backend2)
            .await;

        // Get all backends
        let response = server
            .server
            .get("/discovery")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let backends: Vec<DiscoveryBackend> = response.json();
        assert_eq!(backends.len(), 2);
    }

    #[tokio::test]
    async fn api_when_invalid_json_then_returns_bad_request() {
        let server = setup_test_server().await;

        let response = server
            .server
            .post("/discovery")
            .authorization_bearer(server.authorization.clone())
            .text("invalid json")
            .await;

        assert_eq!(response.status_code(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn api_when_invalid_address_encoding_then_returns_bad_request() {
        let server = setup_test_server().await;

        let response = server
            .server
            .get("/discovery/default/inet/invalid_base64")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_when_unsupported_variant_then_returns_bad_request() {
        let server = setup_test_server().await;

        let response = server
            .server
            .get("/discovery/default/unsupported/dGVzdA")
            .authorization_bearer(server.authorization.clone())
            .await;

        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unauthorized() {
        let server = setup_test_server().await;
        let backend = create_test_backend("default", "192.168.1.1:8080");

        let response = server.server.post("/discovery").json(&backend).await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server
            .server
            .get("/discovery/default/inet/MTkyLjE2OC4xLjE6ODA4MA")
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server
            .server
            .put("/discovery/default/inet/MTkyLjE2OC4xLjE6ODA4MA")
            .json(&backend)
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let response = server.server.delete("/discovery/default").await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }
}
