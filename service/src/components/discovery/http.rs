use crate::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendStore, HttpDiscoveryBackendClient,
};
use crate::api::service::ServiceErrorSource;
use crate::components::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Certificate, Client, ClientBuilder, StatusCode};
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug)]
pub struct HttpDiscoveryBackendStore {
    client: Client,
    discovery_url: String,
    health_check_url: String,
}

impl HttpDiscoveryBackendStore {
    pub fn create(
        base_url: Url,
        total_timeout: Duration,
        connect_timeout: Duration,
        trusted_roots: Vec<Certificate>,
        authorization: String,
    ) -> Result<Self, DiscoveryBackendStoreError> {
        let mut headers = HeaderMap::new();
        let mut auth_value =
            HeaderValue::from_str(&format!("Bearer {authorization}")).map_err(|e| {
                DiscoveryBackendStoreError::internal_error(
                    ServiceErrorSource::Internal,
                    format!("creating http client with base url: {base_url}"),
                    e.to_string(),
                )
            })?;
        auth_value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth_value);

        let mut builder = ClientBuilder::new();

        for root in trusted_roots {
            builder = builder.add_root_certificate(root);
        }

        let client = builder
            .default_headers(headers)
            .use_rustls_tls()
            .timeout(total_timeout)
            .connect_timeout(connect_timeout)
            .build()
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Internal,
                    format!("creating http client with base url: {base_url}"),
                    e,
                )
            })?;
        Self::with_client(client, base_url)
    }

    pub fn with_client(client: Client, base_url: Url) -> Result<Self, DiscoveryBackendStoreError> {
        let base_url = base_url.as_str().trim_end_matches('/').to_string();
        let discovery_url = format!("{base_url}/discovery");
        Url::parse(&discovery_url).map_err(|e| {
            DiscoveryBackendStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!("parsing service url {discovery_url}"),
                e.to_string(),
            )
        })?;

        let health_check_url = format!("{base_url}/health");
        Url::parse(&health_check_url).map_err(|e| {
            DiscoveryBackendStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!("parsing service url {health_check_url}"),
                e.to_string(),
            )
        })?;

        Ok(Self {
            client,
            discovery_url,
            health_check_url,
        })
    }

    fn discovery_address_url(&self, addr: &DiscoveryBackendAddress) -> String {
        format!("{}/{}", self.discovery_url, addr.encoded())
    }
}

#[async_trait]
impl DiscoveryBackendStore for HttpDiscoveryBackendStore {
    type Error = DiscoveryBackendStoreError;

    async fn get(
        &self,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let url = self.discovery_address_url(addr);

        let response = self.client.get(url).send().await.map_err(|e| {
            DiscoveryBackendStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("getting discovery backend for address {}", addr.encoded()),
                e,
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let backend: DiscoveryBackend = response.json().await.map_err(|e| {
                    DiscoveryBackendStoreError::deserialization_error(
                        ServiceErrorSource::Upstream,
                        format!("reading discovery backend for address {}", addr.encoded()),
                        e,
                    )
                })?;
                Ok(Some(backend))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(DiscoveryBackendStoreError::http_status_error(
                ServiceErrorSource::Upstream,
                format!("getting discovery backend for address {}", addr.encoded()),
                status.as_u16(),
            )),
        }
    }

    async fn get_all(&self) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let url = &self.discovery_url;
        let response = self.client.get(url).send().await.map_err(|e| {
            DiscoveryBackendStoreError::http_error(
                ServiceErrorSource::Upstream,
                "retrieving all discovery backends",
                e,
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let backends_response: Vec<DiscoveryBackend> =
                    response.json().await.map_err(|e| {
                        DiscoveryBackendStoreError::deserialization_error(
                            ServiceErrorSource::Upstream,
                            "parsing discovery backends list",
                            e,
                        )
                    })?;
                Ok(backends_response)
            }
            status => Err(DiscoveryBackendStoreError::http_status_error(
                ServiceErrorSource::Upstream,
                "retrieving all discovery backends",
                status.as_u16(),
            )),
        }
    }

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error> {
        let response = self
            .client
            .post(&self.discovery_url)
            .json(&backend)
            .send()
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!("registering discovery backend {backend:?}",),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::CREATED => {
                // Successfully created
                Ok(Some(backend.address))
            }
            StatusCode::CONFLICT => {
                // Backend already exists
                Ok(None)
            }
            status => Err(DiscoveryBackendStoreError::http_status_error(
                ServiceErrorSource::Upstream,
                format!("registering discovery backend {backend:?}",),
                status.as_u16(),
            )),
        }
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let url = self.discovery_address_url(&backend.address);

        let response = self
            .client
            .put(url)
            .json(&backend.backend) // PUT expects DiscoveryBackendSparse
            .send()
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!(
                        "updating discovery backend at address {}",
                        backend.address.encoded()
                    ),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::NO_CONTENT => {
                // Updated existing backend
                Ok(false)
            }
            StatusCode::CREATED => {
                // Created new backend
                Ok(true)
            }
            status => Err(DiscoveryBackendStoreError::http_status_error(
                ServiceErrorSource::Upstream,
                format!(
                    "updating discovery backend at address {}",
                    backend.address.encoded()
                ),
                status.as_u16(),
            )),
        }
    }

    async fn delete(&self, addr: &DiscoveryBackendAddress) -> Result<bool, Self::Error> {
        let url = self.discovery_address_url(addr);

        let response = self.client.delete(url).send().await.map_err(|e| {
            DiscoveryBackendStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("removing discovery backend at address {}", addr.encoded()),
                e,
            )
        })?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(DiscoveryBackendStoreError::http_status_error(
                ServiceErrorSource::Upstream,
                format!("removing discovery backend at address {}", addr.encoded()),
                status.as_u16(),
            )),
        }
    }
}

#[async_trait]
impl HttpDiscoveryBackendClient for HttpDiscoveryBackendStore {
    async fn health(&self) -> Result<(), Self::Error> {
        let response = self
            .client
            .get(&self.health_check_url)
            .send()
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    "health check",
                    e,
                )
            })?;
        if !response.status().is_success() {
            return Err(DiscoveryBackendStoreError::http_status_error(
                ServiceErrorSource::Upstream,
                "health check",
                response.status().as_u16(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::api::discovery::DiscoveryBackendAddress;
    use crate::components::discovery::http::HttpDiscoveryBackendStore;
    use url::Url;

    #[test]
    fn base_urls() {
        let client = HttpDiscoveryBackendStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://base.com").unwrap(),
        )
        .unwrap();

        assert_eq!(&client.discovery_url, "https://base.com/discovery");

        let client = HttpDiscoveryBackendStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://base.com/").unwrap(),
        )
        .unwrap();

        assert_eq!(&client.discovery_url, "https://base.com/discovery");

        assert_eq!(&client.health_check_url, "https://base.com/health");

        let addr = DiscoveryBackendAddress::Url("https://remote.com/backend".parse().unwrap());
        let discovery_partition_address_url = client.discovery_address_url(&addr);
        assert_eq!(
            format!("https://base.com/discovery/{}", addr.encoded()),
            discovery_partition_address_url,
        );
    }
}
