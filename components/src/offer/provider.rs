use crate::offer::error::OfferStoreError;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use switchgear_service_api::lnurl::LnUrlOfferMetadata;
use switchgear_service_api::offer::{Offer, OfferProvider, OfferStore};
use switchgear_service_api::service::ServiceErrorSource;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct StoreOfferProvider<S> {
    store: S,
}

impl<S> StoreOfferProvider<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

#[async_trait]
impl<S> OfferProvider for StoreOfferProvider<S>
where
    S: OfferStore + Send + Sync + 'static,
    S::Error: From<OfferStoreError>,
{
    type Error = S::Error;

    async fn offer(
        &self,
        _hostname: &str,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<Offer>, Self::Error> {
        if let Some(offer) = self.store.get_offer(partition, id, Some(false)).await? {
            let offer_metadata = match offer.offer.metadata {
                Some(metadata) => metadata,
                None => {
                    return Ok(None);
                }
            };

            let lnurl_metadata = LnUrlOfferMetadata(offer_metadata);
            let metadata_json_string = serde_json::to_string(&lnurl_metadata).map_err(|e| {
                OfferStoreError::serialization_error(
                    ServiceErrorSource::Internal,
                    format!("building LNURL offer response for offer {}", offer.id),
                    e,
                )
            })?;

            let mut hasher = Sha256::new();
            hasher.update(metadata_json_string.as_bytes());
            let metadata_json_hash = hasher.finalize().into();

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use switchgear_service_api::offer::{
        OfferMetadataIdentifier, OfferMetadataImage, OfferMetadataSparse, OfferRecord,
        OfferRecordSparse,
    };

    // Mock OfferStore for testing
    #[derive(Clone)]
    struct MockOfferStore {
        response: Option<OfferRecord>,
    }

    impl MockOfferStore {
        fn new(response: Option<OfferRecord>) -> Self {
            Self { response }
        }
    }

    #[async_trait]
    impl OfferStore for MockOfferStore {
        type Error = OfferStoreError;

        async fn get_offer(
            &self,
            _partition: &str,
            _id: &Uuid,
            _sparse: Option<bool>,
        ) -> Result<Option<OfferRecord>, Self::Error> {
            Ok(self.response.clone())
        }

        async fn get_offers(
            &self,
            _partition: &str,
            _start: usize,
            _count: usize,
        ) -> Result<Vec<OfferRecord>, Self::Error> {
            Ok(vec![])
        }

        async fn post_offer(&self, _offer: OfferRecord) -> Result<Option<Uuid>, Self::Error> {
            Ok(None)
        }

        async fn put_offer(&self, _offer: OfferRecord) -> Result<bool, Self::Error> {
            Ok(false)
        }

        async fn delete_offer(&self, _partition: &str, _id: &Uuid) -> Result<bool, Self::Error> {
            Ok(false)
        }
    }

    // Test data generator
    fn create_offer_with_metadata(offer_id: Uuid, metadata_id: Uuid) -> OfferRecord {
        OfferRecord {
            partition: "default".to_string(),
            id: offer_id,
            offer: OfferRecordSparse {
                max_sendable: 5000000,
                min_sendable: 1000,
                metadata_id,
                metadata: Some(OfferMetadataSparse {
                    text: "Test LNURL offer".to_string(),
                    long_text: Some(
                        "This is a comprehensive test of the LNURL offer system".to_string(),
                    ),
                    image: Some(OfferMetadataImage::Png(vec![0x89, 0x50, 0x4E, 0x47])),
                    identifier: Some(OfferMetadataIdentifier::Email(
                        "test@lnurl.com".parse().unwrap(),
                    )),
                }),
                timestamp: Utc::now(),
                expires: Some(Utc::now() + chrono::Duration::hours(24)),
            },
        }
    }

    #[tokio::test]
    async fn test_offer_provider_successful_retrieval() {
        let offer_id = Uuid::new_v4();
        let metadata_id = Uuid::new_v4();
        let offer = create_offer_with_metadata(offer_id, metadata_id);

        let store = MockOfferStore::new(Some(offer));
        let provider = StoreOfferProvider::new(store);
        let result = provider
            .offer("example.com", "default", &offer_id)
            .await
            .unwrap();

        assert!(result.is_some());
        let offer = result.unwrap();

        // Verify basic offer fields
        assert_eq!(offer.id, offer_id);
        assert_eq!(offer.max_sendable, 5000000);
        assert_eq!(offer.min_sendable, 1000);

        // Verify metadata_json_string is in LNURL format and contains the expected metadata
        assert!(offer.metadata_json_string.starts_with("["));
        assert!(offer.metadata_json_string.contains("Test LNURL offer"));
        assert!(offer.metadata_json_string.contains("test@lnurl.com"));

        // Verify the JSON string can be deserialized back to LnUrlOfferMetadata
        let parsed_metadata: LnUrlOfferMetadata =
            serde_json::from_str(&offer.metadata_json_string).unwrap();
        assert_eq!(parsed_metadata.0.text, "Test LNURL offer");
        assert_eq!(
            parsed_metadata.0.long_text,
            Some("This is a comprehensive test of the LNURL offer system".to_string())
        );

        // Verify hash is calculated correctly
        let expected_hash = sha2::Sha256::digest(offer.metadata_json_string.as_bytes());
        assert_eq!(offer.metadata_json_hash, expected_hash.as_ref());
    }

    #[tokio::test]
    async fn test_offer_provider_offer_not_found() {
        let store = MockOfferStore::new(None);
        let provider = StoreOfferProvider::new(store);

        let non_existent_id = Uuid::new_v4();
        let result = provider
            .offer("example.com", "default", &non_existent_id)
            .await
            .unwrap();

        assert!(result.is_none());
    }
}
