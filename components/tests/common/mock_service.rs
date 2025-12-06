use anyhow::bail;
use axum::routing::{delete, get, patch, post, put};
use axum::{
    extract::{Path as AxumPath, State},
    http,
    http::{HeaderMap, StatusCode},
    Json, Router,
};
use hex;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use switchgear_components::discovery::memory::MemoryDiscoveryBackendStore;
use switchgear_components::offer::memory::MemoryOfferStore;
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendPatchSparse, DiscoveryBackendSparse,
    DiscoveryBackendStore,
};
use switchgear_service_api::offer::{
    OfferMetadata, OfferMetadataSparse, OfferMetadataStore, OfferRecord, OfferRecordSparse,
    OfferStore,
};
use switchgear_testing::ports::PortAllocator;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::Notify;
use tokio::time::{sleep as tokio_sleep, timeout};
use uuid::Uuid;

#[derive(Clone)]
struct DiscoveryState {
    store: MemoryDiscoveryBackendStore,
}

#[derive(Clone)]
struct OfferState {
    store: MemoryOfferStore,
    max_page_size: usize,
}

async fn discovery_health() -> StatusCode {
    StatusCode::OK
}

async fn get_backend(
    State(state): State<DiscoveryState>,
    AxumPath(public_key): AxumPath<secp256k1::PublicKey>,
) -> Result<Json<DiscoveryBackend>, StatusCode> {
    match state.store.get(&public_key).await {
        Ok(Some(backend)) => Ok(Json(backend)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_backends(
    State(state): State<DiscoveryState>,
    headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Vec<DiscoveryBackend>>), (StatusCode, HeaderMap)> {
    let if_none_match = headers
        .get(http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            // Decode hex etag to u64
            hex::decode(s).ok().and_then(|bytes| {
                let arr: [u8; 8] = bytes.try_into().ok()?;
                Some(u64::from_be_bytes(arr))
            })
        });

    match state.store.get_all(if_none_match).await {
        Ok(result) => {
            let mut response_headers = HeaderMap::new();
            // Encode etag as hex string (8 bytes)
            let etag_hex = hex::encode(result.etag.to_be_bytes());
            response_headers.insert(http::header::ETAG, etag_hex.parse().unwrap());
            response_headers.insert(
                http::header::CACHE_CONTROL,
                "no-store, no-cache, must-revalidate".parse().unwrap(),
            );
            response_headers.insert(
                http::header::EXPIRES,
                "Thu, 01 Jan 1970 00:00:00 GMT".parse().unwrap(),
            );
            response_headers.insert(http::header::PRAGMA, "no-cache".parse().unwrap());

            match result.backends {
                Some(backends) => Ok((StatusCode::OK, response_headers, Json(backends))),
                None => Err((StatusCode::NOT_MODIFIED, response_headers)),
            }
        }
        Err(_) => {
            let headers = HeaderMap::new();
            Err((StatusCode::INTERNAL_SERVER_ERROR, headers))
        }
    }
}

async fn post_backend(
    State(state): State<DiscoveryState>,
    Json(backend): Json<DiscoveryBackend>,
) -> Result<(StatusCode, HeaderMap), StatusCode> {
    match state.store.post(backend.clone()).await {
        Ok(Some(public_key)) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::LOCATION,
                public_key.to_string().parse().unwrap(),
            );
            Ok((StatusCode::CREATED, headers))
        }
        Ok(None) => Err(StatusCode::CONFLICT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn put_backend(
    State(state): State<DiscoveryState>,
    AxumPath(public_key): AxumPath<secp256k1::PublicKey>,
    Json(backend_sparse): Json<DiscoveryBackendSparse>,
) -> Result<StatusCode, StatusCode> {
    let backend = DiscoveryBackend {
        public_key,
        backend: backend_sparse,
    };

    match state.store.put(backend).await {
        Ok(true) => Ok(StatusCode::CREATED),
        Ok(false) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn patch_backend(
    State(state): State<DiscoveryState>,
    AxumPath(public_key): AxumPath<secp256k1::PublicKey>,
    Json(patch_sparse): Json<DiscoveryBackendPatchSparse>,
) -> Result<StatusCode, StatusCode> {
    let patch = DiscoveryBackendPatch {
        public_key,
        backend: patch_sparse,
    };

    match state.store.patch(patch).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn delete_backend(
    State(state): State<DiscoveryState>,
    AxumPath(public_key): AxumPath<secp256k1::PublicKey>,
) -> Result<StatusCode, StatusCode> {
    match state.store.delete(&public_key).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// Offer handlers
async fn offer_health() -> StatusCode {
    StatusCode::OK
}

async fn get_offer(
    State(state): State<OfferState>,
    AxumPath((partition, id)): AxumPath<(String, Uuid)>,
) -> Result<Json<OfferRecord>, StatusCode> {
    match state.store.get_offer(&partition, &id).await {
        Ok(Some(offer)) => Ok(Json(offer)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_offers(
    State(state): State<OfferState>,
    AxumPath(partition): AxumPath<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<OfferRecord>>, StatusCode> {
    let start: usize = params
        .get("start")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let count: usize = params
        .get("count")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    if count > state.max_page_size {
        return Err(StatusCode::BAD_REQUEST);
    }

    match state.store.get_offers(&partition, start, count).await {
        Ok(offers) => Ok(Json(offers)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn post_offer(
    State(state): State<OfferState>,
    Json(offer): Json<OfferRecord>,
) -> Result<(StatusCode, HeaderMap), StatusCode> {
    match state.store.post_offer(offer.clone()).await {
        Ok(Some(id)) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::LOCATION,
                format!("{}/{}", offer.partition, id).parse().unwrap(),
            );
            Ok((StatusCode::CREATED, headers))
        }
        Ok(None) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::LOCATION,
                format!("{}/{}", offer.partition, offer.id).parse().unwrap(),
            );
            Err(StatusCode::CONFLICT)
        }
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn put_offer(
    State(state): State<OfferState>,
    AxumPath((partition, id)): AxumPath<(String, Uuid)>,
    Json(offer_sparse): Json<OfferRecordSparse>,
) -> Result<StatusCode, StatusCode> {
    let offer = OfferRecord {
        partition,
        id,
        offer: offer_sparse,
    };

    match state.store.put_offer(offer).await {
        Ok(true) => Ok(StatusCode::CREATED),
        Ok(false) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn delete_offer(
    State(state): State<OfferState>,
    AxumPath((partition, id)): AxumPath<(String, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    match state.store.delete_offer(&partition, &id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_metadata(
    State(state): State<OfferState>,
    AxumPath((partition, id)): AxumPath<(String, Uuid)>,
) -> Result<Json<OfferMetadata>, StatusCode> {
    match state.store.get_metadata(&partition, &id).await {
        Ok(Some(metadata)) => Ok(Json(metadata)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_all_metadata(
    State(state): State<OfferState>,
    AxumPath(partition): AxumPath<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<OfferMetadata>>, StatusCode> {
    let start: usize = params
        .get("start")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let count: usize = params
        .get("count")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    if count > state.max_page_size {
        return Err(StatusCode::BAD_REQUEST);
    }

    match state.store.get_all_metadata(&partition, start, count).await {
        Ok(metadata) => Ok(Json(metadata)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn post_metadata(
    State(state): State<OfferState>,
    Json(metadata): Json<OfferMetadata>,
) -> Result<(StatusCode, HeaderMap), StatusCode> {
    match state.store.post_metadata(metadata.clone()).await {
        Ok(Some(id)) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::LOCATION,
                format!("{}/{}", metadata.partition, id).parse().unwrap(),
            );
            Ok((StatusCode::CREATED, headers))
        }
        Ok(None) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::LOCATION,
                format!("{}/{}", metadata.partition, metadata.id)
                    .parse()
                    .unwrap(),
            );
            Err(StatusCode::CONFLICT)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn put_metadata(
    State(state): State<OfferState>,
    AxumPath((partition, id)): AxumPath<(String, Uuid)>,
    Json(metadata_sparse): Json<OfferMetadataSparse>,
) -> Result<StatusCode, StatusCode> {
    let metadata = OfferMetadata {
        partition,
        id,
        metadata: metadata_sparse,
    };

    match state.store.put_metadata(metadata).await {
        Ok(true) => Ok(StatusCode::CREATED),
        Ok(false) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn delete_metadata(
    State(state): State<OfferState>,
    AxumPath((partition, id)): AxumPath<(String, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    match state.store.delete_metadata(&partition, &id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

pub struct TestService {
    pub discovery_port: u16,
    pub offer_port: u16,
    _discovery_handle: tokio::task::JoinHandle<std::io::Result<()>>,
    _offer_handle: tokio::task::JoinHandle<std::io::Result<()>>,
    shutdown_notify: Arc<Notify>,
    pub discovery_authorization: String,
    pub offer_authorization: String,
}

impl TestService {
    pub async fn start(ports_path: &Path) -> anyhow::Result<Self> {
        let discovery_port = PortAllocator::find_available_port(ports_path)?;
        let offer_port = PortAllocator::find_available_port(ports_path)?;

        let discovery_state = DiscoveryState {
            store: MemoryDiscoveryBackendStore::default(),
        };

        let offer_state = OfferState {
            store: MemoryOfferStore::default(),
            max_page_size: 100,
        };

        let discovery_router = Router::new()
            .route("/discovery/{public_key}", get(get_backend))
            .route("/discovery/{public_key}", put(put_backend))
            .route("/discovery/{public_key}", patch(patch_backend))
            .route("/discovery/{public_key}", delete(delete_backend))
            .route("/discovery", get(get_backends))
            .route("/discovery", post(post_backend))
            .route("/health", get(discovery_health))
            .with_state(discovery_state);

        let offer_router = Router::new()
            .route("/offers/{partition}/{id}", get(get_offer))
            .route("/offers/{partition}/{id}", put(put_offer))
            .route("/offers/{partition}/{id}", delete(delete_offer))
            .route("/offers/{partition}", get(get_offers))
            .route("/offers", post(post_offer))
            .route("/metadata/{partition}/{id}", get(get_metadata))
            .route("/metadata/{partition}/{id}", put(put_metadata))
            .route("/metadata/{partition}/{id}", delete(delete_metadata))
            .route("/metadata/{partition}", get(get_all_metadata))
            .route("/metadata", post(post_metadata))
            .route("/health", get(offer_health))
            .with_state(offer_state);

        let discovery_listener =
            TokioTcpListener::bind(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), discovery_port))
                .await?;
        let offer_listener =
            TokioTcpListener::bind(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), offer_port)).await?;

        let shutdown_notify = Arc::new(Notify::new());
        let discovery_shutdown = shutdown_notify.clone();
        let offer_shutdown = shutdown_notify.clone();

        let discovery_handle = tokio::spawn(async move {
            axum::serve(discovery_listener, discovery_router)
                .with_graceful_shutdown(async move {
                    discovery_shutdown.notified().await;
                })
                .await
        });

        let offer_handle = tokio::spawn(async move {
            axum::serve(offer_listener, offer_router)
                .with_graceful_shutdown(async move {
                    offer_shutdown.notified().await;
                })
                .await
        });

        let service = TestService {
            discovery_port,
            offer_port,
            _discovery_handle: discovery_handle,
            _offer_handle: offer_handle,
            shutdown_notify,
            discovery_authorization: "mock-bearer-token".to_string(),
            offer_authorization: "mock-bearer-token".to_string(),
        };

        service.wait_for_startup().await?;

        Ok(service)
    }

    async fn wait_for_startup(&self) -> anyhow::Result<()> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;
        let timeout_duration = Duration::from_secs(15);
        let start = Instant::now();

        while start.elapsed() < timeout_duration {
            let discovery_health = timeout(
                Duration::from_secs(1),
                client
                    .get(format!("http://127.0.0.1:{}/health", self.discovery_port))
                    .send(),
            )
            .await;

            let offer_health = timeout(
                Duration::from_secs(1),
                client
                    .get(format!("http://127.0.0.1:{}/health", self.offer_port))
                    .send(),
            )
            .await;

            if discovery_health.is_ok()
                && discovery_health?.is_ok()
                && offer_health.is_ok()
                && offer_health?.is_ok()
            {
                return Ok(());
            }

            tokio_sleep(Duration::from_millis(200)).await;
        }

        bail!("Services failed to start within timeout")
    }

    pub fn discovery_base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.discovery_port)
    }

    pub fn offer_base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.offer_port)
    }

    pub async fn shutdown(self) {
        self.shutdown_notify.notify_waiters();
        let _ = self._discovery_handle.await;
        let _ = self._offer_handle.await;
    }
}
