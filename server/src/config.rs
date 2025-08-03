use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServerConfig {
    pub lnurl_service: Option<LnUrlBalancerServiceConfig>,
    pub discovery_service: Option<DiscoveryServiceConfig>,
    pub offer_service: Option<OfferServiceConfig>,
    pub store: Option<ServerStoreConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LnUrlBalancerServiceConfig {
    pub partitions: HashSet<String>,
    pub address: SocketAddr,
    pub health_check_frequency_secs: f64,
    pub parallel_health_check: bool,
    pub health_check_consecutive_success_to_healthy: usize,
    pub health_check_consecutive_failure_to_unhealthy: usize,
    pub backend_update_frequency_secs: f64,
    pub invoice_expiry_secs: u64,
    pub allowed_hosts: HashSet<String>,
    pub backoff: BackoffConfig,
    pub backend_selection: BackendSelectionConfig,
    pub tls: Option<TlsConfig>,
    pub ln_client_timeout_secs: f64,
    pub selection_capacity_bias: Option<f64>,
    pub comment_allowed: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DiscoveryServiceConfig {
    pub address: SocketAddr,
    pub auth_authority: PathBuf,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct OfferServiceConfig {
    pub address: SocketAddr,
    pub auth_authority: PathBuf,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum BackoffConfig {
    Stop,
    #[serde(rename_all = "kebab-case")]
    Exponential {
        initial_interval_secs: Option<f64>,
        randomization_factor: Option<f64>,
        multiplier: Option<f64>,
        max_interval_secs: Option<f64>,
        max_elapsed_time_secs: Option<f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendSelectionConfig {
    RoundRobin,
    Random,
    Consistent { max_iterations: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServerStoreConfig {
    pub offer: Option<OfferStoreConfig>,
    pub discover: Option<DiscoveryStoreConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum OfferStoreConfig {
    #[serde(rename_all = "kebab-case")]
    Database {
        database_url: String,
        max_connections: u32,
    },
    Memory,
    #[serde(rename_all = "kebab-case")]
    Http {
        base_url: String,
        connect_timeout_secs: f64,
        total_timeout_secs: f64,
        trusted_roots: Vec<PathBuf>,
        authorization: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum DiscoveryStoreConfig {
    #[serde(rename_all = "kebab-case")]
    Database {
        database_url: String,
        max_connections: u32,
    },
    Memory,
    #[serde(rename_all = "kebab-case")]
    Http {
        base_url: String,
        connect_timeout_secs: f64,
        total_timeout_secs: f64,
        trusted_roots: Vec<PathBuf>,
        authorization: PathBuf,
    },
    #[serde(rename_all = "kebab-case")]
    File {
        storage_dir: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}
