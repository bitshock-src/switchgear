use crate::api::lnurl::LnUrlOfferMetadata;
use crate::api::offer::{
    Offer, OfferMetadata, OfferMetadataStore, OfferProvider, OfferRecord, OfferStore,
};
use crate::api::service::ServiceErrorSource;
use crate::components::offer::error::OfferStoreError;
use async_trait::async_trait;
use sha2::Digest;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct MemoryOfferStore {
    offer: Arc<Mutex<HashMap<(String, Uuid), OfferRecord>>>,
    metadata: Arc<Mutex<HashMap<(String, Uuid), OfferMetadata>>>,
}

impl MemoryOfferStore {
    pub fn new() -> Self {
        Self {
            offer: Arc::new(Mutex::new(HashMap::new())),
            metadata: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for MemoryOfferStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OfferStore for MemoryOfferStore {
    type Error = OfferStoreError;

    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferRecord>, Self::Error> {
        let store = self.offer.lock().await;
        Ok(store.get(&(partition.to_string(), *id)).cloned())
    }

    async fn get_offers(&self, partition: &str) -> Result<Vec<OfferRecord>, Self::Error> {
        let store = self.offer.lock().await;
        let offers: Vec<OfferRecord> = store
            .iter()
            .filter(|((p, _), _)| p == partition)
            .map(|(_, offer)| offer.clone())
            .collect();
        Ok(offers)
    }

    async fn post_offer(&self, offer: OfferRecord) -> Result<Option<Uuid>, Self::Error> {
        let metadata_store = self.metadata.lock().await;
        let mut store = self.offer.lock().await;

        if !metadata_store.contains_key(&(offer.partition.to_string(), offer.offer.metadata_id)) {
            return Err(OfferStoreError::invalid_input_error(
                format!("post offer {offer:?}"),
                format!(
                    "metadata {} not found for offer {}",
                    offer.offer.metadata_id, offer.id
                ),
            ));
        }

        if let std::collections::hash_map::Entry::Vacant(e) =
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
            return Err(OfferStoreError::invalid_input_error(
                format!("put offer {offer:?}"),
                format!(
                    "metadata {} not found for offer {}",
                    offer.offer.metadata_id, offer.id
                ),
            ));
        }

        let was_new = store
            .insert((offer.partition.to_string(), offer.id), offer)
            .is_none();
        Ok(was_new)
    }

    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let mut store = self.offer.lock().await;
        Ok(store.remove(&(partition.to_string(), *id)).is_some())
    }
}

#[async_trait]
impl OfferProvider for MemoryOfferStore {
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
                    format!(
                        "serializing LnUrlOfferMetadata while building LNURL offer response for {offer:?}"
                    ),
                    e,
                )
            })?;

            let metadata_json_hash = sha2::Sha256::digest(metadata_json_string.as_bytes())
                .to_vec()
                .try_into()
                .map_err(|_| {
                    OfferStoreError::hash_conversion_error(
                        ServiceErrorSource::Internal,
                        format!(
                            "hashing LnUrlOfferMetadata json string {metadata_json_string} while building LNURL offer response for {offer:?}"
                        ),
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
impl OfferMetadataStore for MemoryOfferStore {
    type Error = OfferStoreError;

    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferMetadata>, Self::Error> {
        let store = self.metadata.lock().await;
        Ok(store.get(&(partition.to_string(), *id)).cloned())
    }

    async fn get_all_metadata(&self, partition: &str) -> Result<Vec<OfferMetadata>, Self::Error> {
        let store = self.metadata.lock().await;
        let offers: Vec<OfferMetadata> = store
            .iter()
            .filter(|((p, _), _)| p == partition)
            .map(|(_, metadata)| metadata.clone())
            .collect();
        Ok(offers)
    }

    async fn post_metadata(&self, offer: OfferMetadata) -> Result<Option<Uuid>, Self::Error> {
        let mut store = self.metadata.lock().await;
        if let std::collections::hash_map::Entry::Vacant(e) =
            store.entry((offer.partition.to_string(), offer.id))
        {
            e.insert(offer.clone());
            Ok(Some(offer.id))
        } else {
            Ok(None)
        }
    }

    async fn put_metadata(&self, offer: OfferMetadata) -> Result<bool, Self::Error> {
        let mut store = self.metadata.lock().await;
        let was_new = store
            .insert((offer.partition.to_string(), offer.id), offer)
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
            return Err(OfferStoreError::invalid_input_error(
                format!("delete metadata {partition}/{id}"),
                format!("metadata {} is referenced by existing offers", id),
            ));
        }

        Ok(metadata_store
            .remove(&(partition.to_string(), *id))
            .is_some())
    }
}
