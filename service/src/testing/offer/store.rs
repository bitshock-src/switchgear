use crate::testing::error::TestError;
use async_trait::async_trait;
use indexmap::IndexMap;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use switchgear_service_api::lnurl::LnUrlOfferMetadata;
use switchgear_service_api::offer::{
    Offer, OfferMetadata, OfferMetadataStore, OfferProvider, OfferRecord, OfferStore,
};
use switchgear_service_api::service::ServiceErrorSource;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Simplified in-memory offer store for unit tests.
/// This is a minimal implementation designed to replace MemoryOfferStore
/// in service crate tests. Uses IndexMap to preserve insertion order.
#[derive(Clone, Debug)]
pub struct TestOfferStore {
    offer: Arc<Mutex<IndexMap<(String, Uuid), OfferRecord>>>,
    metadata: Arc<Mutex<IndexMap<(String, Uuid), OfferMetadata>>>,
}

impl TestOfferStore {
    pub fn new() -> Self {
        Self {
            offer: Arc::new(Mutex::new(IndexMap::new())),
            metadata: Arc::new(Mutex::new(IndexMap::new())),
        }
    }
}

impl Default for TestOfferStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OfferStore for TestOfferStore {
    type Error = TestError;

    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
        sparse: Option<bool>,
    ) -> Result<Option<OfferRecord>, Self::Error> {
        let sparse = sparse.unwrap_or(true);
        let metadata_store = self.metadata.lock().await;
        let store = self.offer.lock().await;

        Ok(store.get(&(partition.to_string(), *id)).and_then(|offer| {
            if sparse {
                Some(offer.clone())
            } else {
                metadata_store
                    .get(&(partition.to_string(), offer.offer.metadata_id))
                    .map(|metadata| {
                        let mut offer = offer.clone();
                        offer.offer.metadata = Some(metadata.metadata.clone());
                        offer
                    })
            }
        }))
    }

    async fn get_offers(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferRecord>, Self::Error> {
        let store = self.offer.lock().await;
        // IndexMap preserves insertion order
        let offers: Vec<OfferRecord> = store
            .iter()
            .filter(|((p, _), _)| p == partition)
            .skip(start)
            .take(count)
            .map(|(_, offer)| offer.clone())
            .collect();

        Ok(offers)
    }

    async fn post_offer(&self, offer: OfferRecord) -> Result<Option<Uuid>, Self::Error> {
        let metadata_store = self.metadata.lock().await;
        let mut store = self.offer.lock().await;

        if !metadata_store.contains_key(&(offer.partition.to_string(), offer.offer.metadata_id)) {
            return Err(TestError::error(
                format!(
                    "metadata {} not found for offer {}",
                    offer.offer.metadata_id, offer.id
                ),
                ServiceErrorSource::Downstream,
                format!("post offer {offer:?}"),
            ));
        }

        if let indexmap::map::Entry::Vacant(e) =
            store.entry((offer.partition.to_string(), offer.id))
        {
            e.insert(offer.clone());
            Ok(Some(offer.id))
        } else {
            Ok(None)
        }
    }

    async fn put_offer(&self, offer: OfferRecord) -> Result<bool, Self::Error> {
        let metadata_store = self.metadata.lock().await;
        let mut store = self.offer.lock().await;

        if !metadata_store.contains_key(&(offer.partition.to_string(), offer.offer.metadata_id)) {
            return Err(TestError::error(
                format!(
                    "metadata {} not found for offer {}",
                    offer.offer.metadata_id, offer.id
                ),
                ServiceErrorSource::Downstream,
                format!("put offer {offer:?}"),
            ));
        }

        let was_new = store
            .insert((offer.partition.to_string(), offer.id), offer)
            .is_none();
        Ok(was_new)
    }

    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let mut store = self.offer.lock().await;
        Ok(store.swap_remove(&(partition.to_string(), *id)).is_some())
    }
}

#[async_trait]
impl OfferMetadataStore for TestOfferStore {
    type Error = TestError;

    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferMetadata>, Self::Error> {
        let store = self.metadata.lock().await;
        Ok(store.get(&(partition.to_string(), *id)).cloned())
    }

    async fn get_all_metadata(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferMetadata>, Self::Error> {
        let store = self.metadata.lock().await;
        // IndexMap preserves insertion order
        let metadata: Vec<OfferMetadata> = store
            .iter()
            .filter(|((p, _), _)| p == partition)
            .skip(start)
            .take(count)
            .map(|(_, metadata)| metadata.clone())
            .collect();

        Ok(metadata)
    }

    async fn post_metadata(&self, metadata: OfferMetadata) -> Result<Option<Uuid>, Self::Error> {
        let mut store = self.metadata.lock().await;
        if let indexmap::map::Entry::Vacant(e) =
            store.entry((metadata.partition.to_string(), metadata.id))
        {
            e.insert(metadata.clone());
            Ok(Some(metadata.id))
        } else {
            Ok(None)
        }
    }

    async fn put_metadata(&self, metadata: OfferMetadata) -> Result<bool, Self::Error> {
        let mut store = self.metadata.lock().await;
        let was_new = store
            .insert((metadata.partition.to_string(), metadata.id), metadata)
            .is_none();
        Ok(was_new)
    }

    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let offer_store = self.offer.lock().await;
        let mut metadata_store = self.metadata.lock().await;

        let metadata_in_use = offer_store
            .values()
            .any(|offer| offer.partition == partition && offer.offer.metadata_id == *id);

        if metadata_in_use {
            return Err(TestError::error(
                format!("metadata {} is referenced by existing offers", id),
                ServiceErrorSource::Downstream,
                format!("delete metadata {partition}/{id}"),
            ));
        }

        Ok(metadata_store
            .swap_remove(&(partition.to_string(), *id))
            .is_some())
    }
}

#[async_trait]
impl OfferProvider for TestOfferStore {
    type Error = TestError;

    async fn offer(
        &self,
        _hostname: &str,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<Offer>, Self::Error> {
        if let Some(offer) = self.get_offer(partition, id, Some(false)).await? {
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
                TestError::error(
                    format!("serialization error: {e}"),
                    ServiceErrorSource::Internal,
                    format!(
                        "serializing LnUrlOfferMetadata while building LNURL offer response for {offer:?}"
                    ),
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
