use anyhow::bail;
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use p256::ecdsa::SigningKey;
use pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rand::thread_rng;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use switchgear_service::components::discovery::memory::MemoryDiscoveryBackendStore;
use switchgear_service::components::offer::memory::MemoryOfferStore;
use switchgear_service::{
    DiscoveryAudience, DiscoveryClaims, DiscoveryService, DiscoveryState, OfferAudience,
    OfferClaims, OfferService, OfferState,
};
use switchgear_testing::ports::PortAllocator;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::Notify;
use tokio::time::{sleep as tokio_sleep, timeout};

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
        let mut rng = thread_rng();
        let discovery_key_pair = SigningKey::random(&mut rng);

        let discovery_decoding_key = *discovery_key_pair.verifying_key();
        let discovery_decoding_key =
            discovery_decoding_key.to_public_key_pem(LineEnding::default())?;
        let discovery_decoding_key = DecodingKey::from_ec_pem(discovery_decoding_key.as_bytes())?;

        let discovery_encoding_key = discovery_key_pair;
        let discovery_encoding_key = discovery_encoding_key.to_pkcs8_pem(LineEnding::default())?;
        let discovery_encoding_key = EncodingKey::from_ec_pem(discovery_encoding_key.as_bytes())?;

        let discovery_token_header = Header::new(Algorithm::ES256);
        let discovery_claims = DiscoveryClaims {
            aud: DiscoveryAudience::Discovery,
            exp: (SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + 3600) as usize,
        };
        let discovery_authorization = encode(
            &discovery_token_header,
            &discovery_claims,
            &discovery_encoding_key,
        )?;

        let offer_key_pair = SigningKey::random(&mut rng);

        let offer_decoding_key = *offer_key_pair.verifying_key();
        let offer_decoding_key = offer_decoding_key.to_public_key_pem(LineEnding::default())?;
        let offer_decoding_key = DecodingKey::from_ec_pem(offer_decoding_key.as_bytes())?;

        let offer_encoding_key = offer_key_pair;
        let offer_encoding_key = offer_encoding_key.to_pkcs8_pem(LineEnding::default())?;
        let offer_encoding_key = EncodingKey::from_ec_pem(offer_encoding_key.as_bytes())?;

        let offer_token_header = Header::new(Algorithm::ES256);
        let offer_claims = OfferClaims {
            aud: OfferAudience::Offer,
            exp: (SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + 3600) as usize,
        };
        let offer_authorization = encode(&offer_token_header, &offer_claims, &offer_encoding_key)?;

        // Generate random high ports
        let discovery_port = PortAllocator::find_available_port(ports_path)?;
        let offer_port = PortAllocator::find_available_port(ports_path)?;

        // Create DiscoveryState with MemoryDiscoveryBackendStore
        let discovery_store = MemoryDiscoveryBackendStore::default();
        let discovery_state = DiscoveryState::new(discovery_store, discovery_decoding_key);

        // Create OfferState with MemoryOfferStore for both stores
        let offer_store = MemoryOfferStore::default();
        let metadata_store = MemoryOfferStore::default();
        let offer_state = OfferState::new(offer_store, metadata_store, offer_decoding_key);

        // Create listeners
        let discovery_listener =
            TokioTcpListener::bind(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), discovery_port))
                .await?;
        let offer_listener =
            TokioTcpListener::bind(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), offer_port)).await?;

        // Create shutdown signal
        let shutdown_notify = Arc::new(Notify::new());
        let discovery_shutdown = shutdown_notify.clone();
        let offer_shutdown = shutdown_notify.clone();

        // Start the discovery service with graceful shutdown
        let discovery_handle = tokio::spawn(async move {
            axum::serve(
                discovery_listener,
                DiscoveryService::router(discovery_state),
            )
            .with_graceful_shutdown(async move {
                discovery_shutdown.notified().await;
            })
            .await
        });

        // Start the offer service with graceful shutdown
        let offer_handle = tokio::spawn(async move {
            axum::serve(offer_listener, OfferService::router(offer_state))
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
            discovery_authorization,
            offer_authorization,
        };

        // Wait for services to start up
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
            // Try to connect to both health endpoints
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

    // Legacy method for backward compatibility
    pub fn base_url(&self) -> String {
        self.discovery_base_url()
    }

    pub async fn shutdown(self) {
        // Send shutdown signal to both services
        self.shutdown_notify.notify_waiters();

        // Wait for both services to complete
        let _ = self._discovery_handle.await;
        let _ = self._offer_handle.await;
    }
}
