pub mod certs;
pub mod cli;
pub mod global;
pub mod pay;
pub mod server;
pub mod token;

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use switchgear_server::config::TlsConfig;

#[derive(Clone, Debug)]
pub struct DiscoveryServiceConfigOverride {
    pub address: SocketAddr,
    pub tls: Option<TlsConfig>,
    pub domain: String,
}

#[derive(Clone, Debug)]
pub struct LnUrlBalancerServiceConfigOverride {
    pub address: SocketAddr,
    pub tls: Option<TlsConfig>,
    pub domain: String,
}

#[derive(Clone, Debug)]
pub struct OfferServiceConfigOverride {
    pub address: SocketAddr,
    pub tls: Option<TlsConfig>,
    pub domain: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Protocol {
    Http,
    Https,
}

impl Display for Protocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Http => write!(f, "http"),
            Protocol::Https => write!(f, "https"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ServerConfigOverrides {
    pub lnurl_service: LnUrlBalancerServiceConfigOverride,
    pub discovery_service: DiscoveryServiceConfigOverride,
    pub offers_service: OfferServiceConfigOverride,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Service {
    #[serde(rename = "lnurl")]
    LnUrl,
    Discovery,
    Offer,
}

impl Display for Service {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Service::Offer => write!(f, "offer"),
            Service::Discovery => write!(f, "discovery"),
            Service::LnUrl => write!(f, "lnurl"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceProfile {
    pub domain: String,
    pub protocol: Protocol,
    pub address: SocketAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestConfiguration {
    pub service_domains: TestConfigurationServiceDomains,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestConfigurationServiceDomains {
    lnurl: String,
    discovery: String,
    offer: String,
}
