use crate::offer::error::OfferStoreError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use switchgear_service_api::offer::{OfferMetadata, OfferMetadataStore, OfferRecord, OfferStore};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug)]
struct OfferRecordTimestamped {
    created: chrono::DateTime<chrono::Utc>,
    offer: OfferRecord,
}

#[derive(Clone, Debug)]
struct OfferMetadataTimestamped {
    created: chrono::DateTime<chrono::Utc>,
    metadata: OfferMetadata,
}

#[derive(Clone, Debug)]
pub struct MemoryOfferStore {
    offer: Arc<Mutex<HashMap<(String, Uuid), OfferRecordTimestamped>>>,
    metadata: Arc<Mutex<HashMap<(String, Uuid), OfferMetadataTimestamped>>>,
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
        sparse: Option<bool>,
    ) -> Result<Option<OfferRecord>, Self::Error> {
        let sparse = sparse.unwrap_or(true);
        let metadata_store = self.metadata.lock().await;
        let store = self.offer.lock().await;

        Ok(store.get(&(partition.to_string(), *id)).and_then(|offer| {
            if sparse {
                Some(offer.offer.clone())
            } else {
                metadata_store
                    .get(&(partition.to_string(), offer.offer.offer.metadata_id))
                    .map(|metadata| {
                        let mut offer = offer.offer.clone();
                        offer.offer.metadata = Some(metadata.metadata.metadata.clone());
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
        let mut offers: Vec<OfferRecordTimestamped> = store
            .iter()
            .filter(|((p, _), _)| p == partition)
            .map(|(_, offer)| offer.clone())
            .collect();

        offers.sort_by(|a, b| {
            a.created
                .cmp(&b.created)
                .then_with(|| a.offer.id.cmp(&b.offer.id))
        });

        let offers = offers
            .into_iter()
            .skip(start)
            .take(count)
            .map(|o| o.offer)
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
            e.insert(OfferRecordTimestamped {
                created: chrono::Utc::now(),
                offer: offer.clone(),
            });
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
            .insert(
                (offer.partition.to_string(), offer.id),
                OfferRecordTimestamped {
                    created: chrono::Utc::now(),
                    offer,
                },
            )
            .is_none();
        Ok(was_new)
    }

    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let mut store = self.offer.lock().await;
        Ok(store.remove(&(partition.to_string(), *id)).is_some())
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
        Ok(store
            .get(&(partition.to_string(), *id))
            .map(|o| o.metadata.clone()))
    }

    async fn get_all_metadata(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferMetadata>, Self::Error> {
        let store = self.metadata.lock().await;
        let mut metadata: Vec<OfferMetadataTimestamped> = store
            .iter()
            .filter(|((p, _), _)| p == partition)
            .map(|(_, metadata)| metadata.clone())
            .collect();

        metadata.sort_by(|a, b| {
            a.created
                .cmp(&b.created)
                .then_with(|| a.metadata.id.cmp(&b.metadata.id))
        });

        let metadata = metadata
            .into_iter()
            .skip(start)
            .take(count)
            .map(|o| o.metadata)
            .collect();

        Ok(metadata)
    }

    async fn post_metadata(&self, metadata: OfferMetadata) -> Result<Option<Uuid>, Self::Error> {
        let mut store = self.metadata.lock().await;
        if let std::collections::hash_map::Entry::Vacant(e) =
            store.entry((metadata.partition.to_string(), metadata.id))
        {
            e.insert(OfferMetadataTimestamped {
                created: chrono::Utc::now(),
                metadata: metadata.clone(),
            });

            Ok(Some(metadata.id))
        } else {
            Ok(None)
        }
    }

    async fn put_metadata(&self, metadata: OfferMetadata) -> Result<bool, Self::Error> {
        let mut store = self.metadata.lock().await;
        let was_new = store
            .insert(
                (metadata.partition.to_string(), metadata.id),
                OfferMetadataTimestamped {
                    created: chrono::Utc::now(),
                    metadata,
                },
            )
            .is_none();
        Ok(was_new)
    }

    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let offer_store = self.offer.lock().await;
        let mut metadata_store = self.metadata.lock().await;

        let metadata_in_use = offer_store.values().any(|offer| {
            offer.offer.partition == partition && offer.offer.offer.metadata_id == *id
        });

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
