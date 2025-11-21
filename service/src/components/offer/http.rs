use crate::api::lnurl::LnUrlOfferMetadata;
use crate::api::offer::{
    HttpOfferClient, Offer, OfferMetadata, OfferMetadataStore, OfferProvider, OfferRecord,
    OfferStore,
};
use crate::api::service::ServiceErrorSource;
use crate::components::offer::error::OfferStoreError;
use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderValue};
use reqwest::{Certificate, Client, ClientBuilder, StatusCode};
use sha2::Digest;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct HttpOfferStore {
    client: Client,
    offer_url: String,
    metadata_url: String,
    health_check_url: String,
}

impl HttpOfferStore {
    pub fn create(
        base_url: Url,
        total_timeout: Duration,
        connect_timeout: Duration,
        trusted_roots: Vec<Certificate>,
        authorization: String,
    ) -> Result<Self, OfferStoreError> {
        let mut headers = HeaderMap::new();
        let mut auth_value =
            HeaderValue::from_str(&format!("Bearer {authorization}")).map_err(|e| {
                OfferStoreError::internal_error(
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
                OfferStoreError::http_error(
                    ServiceErrorSource::Internal,
                    format!("creating http client with base url: {base_url}"),
                    e,
                )
            })?;
        Self::with_client(client, base_url)
    }

    fn with_client(client: Client, base_url: Url) -> Result<Self, OfferStoreError> {
        let base_url = base_url.as_str().trim_end_matches('/').to_string();

        let offer_url = format!("{base_url}/offers");
        Url::parse(&offer_url).map_err(|e| {
            OfferStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!("parsing service url {offer_url}"),
                e.to_string(),
            )
        })?;

        let metadata_url = format!("{base_url}/metadata");
        Url::parse(&offer_url).map_err(|e| {
            OfferStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!("parsing service url {metadata_url}"),
                e.to_string(),
            )
        })?;

        let health_check_url = format!("{base_url}/health");
        Url::parse(&health_check_url).map_err(|e| {
            OfferStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!("parsing service url {health_check_url}"),
                e.to_string(),
            )
        })?;

        Ok(Self {
            client,
            offer_url,
            metadata_url,
            health_check_url,
        })
    }

    fn offers_partition_url(&self, partition: &str) -> String {
        format!("{}/{}", self.offer_url, partition)
    }

    fn offers_partition_id_url(&self, partition: &str, id: &Uuid) -> String {
        format!("{}/{}", self.offers_partition_url(partition), id)
    }

    fn metadata_partition_url(&self, partition: &str) -> String {
        format!("{}/{}", self.metadata_url, partition)
    }

    fn metadata_partition_id_url(&self, partition: &str, id: &Uuid) -> String {
        format!("{}/{}", self.metadata_partition_url(partition), id)
    }

    fn general_error(status: StatusCode, context: &str) -> OfferStoreError {
        if status.is_success() {
            return OfferStoreError::internal_error(
                ServiceErrorSource::Upstream,
                context.to_string(),
                format!("unexpected http status {status}"),
            );
        }
        if status.is_client_error() {
            return OfferStoreError::invalid_input_error(
                context.to_string(),
                format!("invalid input, http status: {status}"),
            );
        }
        OfferStoreError::http_status_error(
            ServiceErrorSource::Upstream,
            context.to_string(),
            status.as_u16(),
        )
    }
}

#[async_trait]
impl OfferStore for HttpOfferStore {
    type Error = OfferStoreError;

    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferRecord>, Self::Error> {
        let url = self.offers_partition_id_url(partition, id);
        let response = self.client.get(&url).send().await.map_err(|e| {
            OfferStoreError::http_error(ServiceErrorSource::Upstream, format!("get offer {url}"), e)
        })?;

        match response.status() {
            StatusCode::OK => {
                let offer = response.json::<OfferRecord>().await.map_err(|e| {
                    OfferStoreError::deserialization_error(
                        ServiceErrorSource::Upstream,
                        format!("parsing offer {id}"),
                        e,
                    )
                })?;
                Ok(Some(offer))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(Self::general_error(status, &format!("get offer {url}"))),
        }
    }

    async fn get_offers(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferRecord>, Self::Error> {
        let url = self.offers_partition_url(partition);
        let url = format!("{url}?start={start}&count={count}");
        let response = self.client.get(&url).send().await.map_err(|e| {
            OfferStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("get all offers {url}"),
                e,
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let offer_records = response.json::<Vec<OfferRecord>>().await.map_err(|e| {
                    OfferStoreError::deserialization_error(
                        ServiceErrorSource::Upstream,
                        format!("parsing all offers for {url}"),
                        e,
                    )
                })?;
                Ok(offer_records)
            }
            status => Err(Self::general_error(
                status,
                &format!("get all offers {url}"),
            )),
        }
    }

    async fn post_offer(&self, offer: OfferRecord) -> Result<Option<Uuid>, Self::Error> {
        let response = self
            .client
            .post(&self.offer_url)
            .json(&offer)
            .send()
            .await
            .map_err(|e| {
                OfferStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!("post offer: {}, url: {}", offer.id, &self.offer_url),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::CREATED => Ok(Some(offer.id)),
            StatusCode::CONFLICT => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!("post offer: {}, url: {}", offer.id, &self.offer_url),
            )),
        }
    }

    async fn put_offer(&self, offer: OfferRecord) -> Result<bool, Self::Error> {
        let url = self.offers_partition_id_url(&offer.partition, &offer.id);
        let response = self
            .client
            .put(&url)
            .json(&offer)
            .send()
            .await
            .map_err(|e| {
                OfferStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!("put offer {url}"),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::CREATED => Ok(true),
            StatusCode::NO_CONTENT => Ok(false),
            status => Err(Self::general_error(status, &format!("put offer {url}"))),
        }
    }

    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let url = self.offers_partition_id_url(partition, id);
        let response = self.client.delete(&url).send().await.map_err(|e| {
            OfferStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("delete offer {url}"),
                e,
            )
        })?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(Self::general_error(status, &format!("delete offer {url}"))),
        }
    }
}

#[async_trait]
impl OfferMetadataStore for HttpOfferStore {
    type Error = OfferStoreError;

    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferMetadata>, Self::Error> {
        let url = self.metadata_partition_id_url(partition, id);
        let response = self.client.get(&url).send().await.map_err(|e| {
            OfferStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("get offer metadata {url}"),
                e,
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let metadata = response.json::<OfferMetadata>().await.map_err(|e| {
                    OfferStoreError::deserialization_error(
                        ServiceErrorSource::Upstream,
                        format!("parse offer metadata {url}"),
                        e,
                    )
                })?;
                Ok(Some(metadata))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!("get offer metadata {url}"),
            )),
        }
    }

    async fn get_all_metadata(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferMetadata>, Self::Error> {
        let url = self.metadata_partition_url(partition);
        let url = format!("{url}?start={start}&count={count}");
        let response = self.client.get(&url).send().await.map_err(|e| {
            OfferStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("get all metadata {url}"),
                e,
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let metadata_all = response.json::<Vec<OfferMetadata>>().await.map_err(|e| {
                    OfferStoreError::deserialization_error(
                        ServiceErrorSource::Upstream,
                        format!("parse all metadata {url}"),
                        e,
                    )
                })?;
                Ok(metadata_all)
            }
            status => Err(Self::general_error(
                status,
                &format!("get all metadata {url}"),
            )),
        }
    }

    async fn post_metadata(&self, metadata: OfferMetadata) -> Result<Option<Uuid>, Self::Error> {
        let response = self
            .client
            .post(&self.metadata_url)
            .json(&metadata)
            .send()
            .await
            .map_err(|e| {
                OfferStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!(
                        "post offer metadata {}, url: {}",
                        metadata.id, &self.metadata_url
                    ),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::CREATED => Ok(Some(metadata.id)),
            StatusCode::CONFLICT => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!(
                    "post offer metadata {}, url: {}",
                    metadata.id, &self.metadata_url
                ),
            )),
        }
    }

    async fn put_metadata(&self, metadata: OfferMetadata) -> Result<bool, Self::Error> {
        let url = self.metadata_partition_id_url(&metadata.partition, &metadata.id);
        let response = self
            .client
            .put(&url)
            .json(&metadata)
            .send()
            .await
            .map_err(|e| {
                OfferStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!("put offer metadata {url}"),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::CREATED => Ok(true),
            StatusCode::NO_CONTENT => Ok(false),
            status => Err(Self::general_error(
                status,
                &format!("put offer metadata {url}"),
            )),
        }
    }

    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let url = self.metadata_partition_id_url(partition, id);
        let response = self.client.delete(&url).send().await.map_err(|e| {
            OfferStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("delete offer metadata {url}"),
                e,
            )
        })?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(Self::general_error(
                status,
                &format!("delete offer metadata {url}"),
            )),
        }
    }
}

#[async_trait]
impl OfferProvider for HttpOfferStore {
    type Error = OfferStoreError;

    async fn offer(
        &self,
        _hostname: &str,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<Offer>, Self::Error> {
        if let Some(offer) = self.get_offer(partition, id).await? {
            let offer_metadata = match self
                .get_metadata(partition, &offer.offer.metadata_id)
                .await?
            {
                Some(metadata) => metadata,
                None => {
                    return Ok(None);
                }
            };

            let lnurl_metadata = LnUrlOfferMetadata(offer_metadata.metadata);
            let metadata_json_string = serde_json::to_string(&lnurl_metadata).map_err(|e| {
                OfferStoreError::serialization_error(
                    ServiceErrorSource::Internal,
                    format!("building LNURL offer response for offer {}", offer.id),
                    e,
                )
            })?;

            let metadata_json_hash = sha2::Sha256::digest(metadata_json_string.as_bytes())
                .to_vec()
                .try_into()
                .map_err(|_| {
                    OfferStoreError::hash_conversion_error(
                        ServiceErrorSource::Internal,
                        format!("generating metadata hash for offer {}", offer.id),
                    )
                })?;

            Ok(Some(Offer {
                partition: offer.partition,
                id: offer.id,
                max_sendable: offer.offer.max_sendable,
                min_sendable: offer.offer.min_sendable,
                metadata_json_string,
                metadata_json_hash,
                timestamp: offer.offer.timestamp,
                expires: offer.offer.expires,
            }))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl HttpOfferClient for HttpOfferStore {
    async fn health(&self) -> Result<(), <Self as OfferStore>::Error> {
        let response = self
            .client
            .get(&self.health_check_url)
            .send()
            .await
            .map_err(|e| {
                OfferStoreError::http_error(ServiceErrorSource::Upstream, "health check", e)
            })?;
        if !response.status().is_success() {
            return Err(OfferStoreError::http_status_error(
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
    use crate::components::offer::http::HttpOfferStore;
    use url::Url;
    use uuid::Uuid;

    #[test]
    fn base_urls() {
        let client = HttpOfferStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://offers-base.com").unwrap(),
        )
        .unwrap();

        assert_eq!(&client.offer_url, "https://offers-base.com/offers");
        assert_eq!(&client.metadata_url, "https://offers-base.com/metadata");

        let client = HttpOfferStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://offers-base.com/").unwrap(),
        )
        .unwrap();

        assert_eq!(&client.offer_url, "https://offers-base.com/offers");
        assert_eq!(&client.metadata_url, "https://offers-base.com/metadata");

        assert_eq!(&client.health_check_url, "https://offers-base.com/health");

        let offers_partition_url = client.offers_partition_url("partition");
        assert_eq!(
            "https://offers-base.com/offers/partition",
            offers_partition_url,
        );

        let id = Uuid::new_v4();
        let offers_partition_id_url = client.offers_partition_id_url("partition", &id);
        assert_eq!(
            format!("https://offers-base.com/offers/partition/{id}"),
            offers_partition_id_url,
        );

        let metadata_partition_url = client.metadata_partition_url("partition");
        assert_eq!(
            "https://offers-base.com/metadata/partition",
            metadata_partition_url,
        );

        let id = Uuid::new_v4();
        let metadata_partition_id_url = client.metadata_partition_id_url("partition", &id);
        assert_eq!(
            format!("https://offers-base.com/metadata/partition/{id}"),
            metadata_partition_id_url,
        );
    }
}
