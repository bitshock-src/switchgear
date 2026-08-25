use anyhow::{Context, bail};
use reqwest::{Certificate, Client, ClientBuilder, Url};
use std::net::SocketAddr;
use std::time::Duration;
use switchgear_service_api::lnurl::{LnUrlInvoice, LnUrlOffer};
use tokio::net::TcpStream;
use tokio::time::timeout;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct LnUrlTestClient {
    client: Client,
    base_url: String,
}

impl LnUrlTestClient {
    pub fn create(
        base_url: String,
        total_timeout: Duration,
        connect_timeout: Duration,
        trusted_roots: Vec<Certificate>,
    ) -> anyhow::Result<Self> {
        let mut builder = ClientBuilder::new();
        for root in trusted_roots {
            builder = builder.add_root_certificate(root);
        }

        let client = builder
            .use_rustls_tls()
            .timeout(total_timeout)
            .connect_timeout(connect_timeout)
            .build()
            .with_context(|| format!("creating http client with base url: {base_url}"))?;

        Ok(Self::with_client(client, base_url))
    }

    fn with_client(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    pub async fn get_offer(&self, partition: &str, offer_id: &Uuid) -> anyhow::Result<LnUrlOffer> {
        let url = format!("{}/offers/{partition}/{offer_id}", self.base_url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("getting lnurl offer: {partition}/{offer_id}"))?;

        if !response.status().is_success() {
            anyhow::bail!(
                "failed getting lnurl offer: {partition}/{offer_id} with status code {0}",
                response.status()
            );
        }

        response
            .json()
            .await
            .with_context(|| format!("getting lnurl offer: {partition}/{offer_id}"))
    }

    pub async fn get_invoice(
        &self,
        offer: &LnUrlOffer,
        amount: usize,
    ) -> anyhow::Result<LnUrlInvoice> {
        let callback = offer.callback.to_string();

        if !callback.starts_with(self.base_url.as_str()) {
            bail!(
                "offer {offer:?} callback does not start with {}",
                self.base_url
            );
        }
        let mut callback = Url::parse(callback.as_str())
            .with_context(|| format!("parsing callback {callback} for offer {offer:?}"))?;
        callback
            .query_pairs_mut()
            .append_pair("amount", amount.to_string().as_str());

        let response = self
            .client
            .get(callback)
            .send()
            .await
            .with_context(|| format!("getting invoice from callback for offer: {offer:?}"))?;

        if !response.status().is_success() {
            anyhow::bail!(
                "failed getting lnurl offer: {offer:?} with status code {0}",
                response.status()
            );
        }

        response
            .json()
            .await
            .with_context(|| format!("getting lnurl offer: {offer:?}"))
    }

    pub async fn health(&self) -> anyhow::Result<()> {
        let url = format!("{}/health", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| "health check")?;
        if !response.status().is_success() {
            anyhow::bail!(
                "failed health check with status code {0}",
                response.status()
            );
        }
        Ok(())
    }

    pub async fn get_raw(&self, path: &str) -> anyhow::Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);

        self.client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET request to path: {path}"))
    }
}

/// Minimal HTTP client for negative-path tests where we need to bypass the
/// typed HttpDiscoveryBackendStore / HttpOfferStore clients — e.g. sending
/// requests with no bearer token, or a body the typed clients wouldn't emit.
#[derive(Clone, Debug)]
pub struct RawHttpClient {
    client: Client,
    base_url: String,
}

impl RawHttpClient {
    pub fn create(base_url: String, trusted_roots: Vec<Certificate>) -> anyhow::Result<Self> {
        let mut builder = ClientBuilder::new();
        for root in trusted_roots {
            builder = builder.add_root_certificate(root);
        }
        let client = builder
            .use_rustls_tls()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .with_context(|| format!("creating raw http client with base url: {base_url}"))?;
        Ok(Self { client, base_url })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn get(&self, path: &str) -> anyhow::Result<reqwest::Response> {
        let url = format!("{}{path}", self.base_url);
        self.client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))
    }

    pub async fn get_with_bearer(
        &self,
        path: &str,
        bearer: &str,
    ) -> anyhow::Result<reqwest::Response> {
        let url = format!("{}{path}", self.base_url);
        self.client
            .get(&url)
            .bearer_auth(bearer)
            .send()
            .await
            .with_context(|| format!("GET {url} (with bearer)"))
    }

    pub async fn post_json(
        &self,
        path: &str,
        bearer: Option<&str>,
        body: &str,
    ) -> anyhow::Result<reqwest::Response> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string());
        if let Some(b) = bearer {
            req = req.bearer_auth(b);
        }
        req.send()
            .await
            .with_context(|| format!("POST {url} (bearer={})", bearer.is_some()))
    }
}

pub struct TcpProbe {
    address: SocketAddr,
    timeout: Duration,
}

impl TcpProbe {
    pub fn new(address: SocketAddr, timeout: Duration) -> Self {
        Self { address, timeout }
    }

    pub async fn probe(&self) -> bool {
        match timeout(self.timeout, TcpStream::connect(&self.address)).await {
            Ok(c) => c.is_ok(),
            Err(_) => false,
        }
    }
}
