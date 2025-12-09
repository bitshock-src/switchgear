use chrono::{Timelike, Utc};
use switchgear_components::offer::error::{OfferStoreError, OfferStoreErrorSourceKind};
use switchgear_service_api::offer::{
    OfferMetadata, OfferMetadataIdentifier, OfferMetadataImage, OfferMetadataSparse,
    OfferMetadataStore, OfferRecord, OfferRecordSparse, OfferStore,
};
use switchgear_service_api::service::ServiceErrorSource;
use uuid::Uuid;

// Test data generators
pub fn create_test_offer_with_existing_metadata(id: Uuid, metadata_id: Uuid) -> OfferRecord {
    // Truncate timestamps to second precision to match MySQL's TIMESTAMP behavior
    let now = Utc::now().with_nanosecond(0).unwrap();
    let expires = (now + chrono::Duration::seconds(3600))
        .with_nanosecond(0)
        .unwrap();

    OfferRecord {
        partition: "default".to_string(),
        id,
        offer: OfferRecordSparse {
            max_sendable: 1000,
            min_sendable: 100,
            metadata_id,
            metadata: None,
            timestamp: now,
            expires: Some(expires),
        },
    }
}

pub fn create_test_offer_metadata(id: Uuid) -> OfferMetadata {
    OfferMetadata {
        id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: "test metadata".to_string(),
            long_text: Some("test long metadata".to_string()),
            image: Some(OfferMetadataImage::Png(vec![1, 2, 3])),
            identifier: Some(OfferMetadataIdentifier::Email(
                "test@example.com".parse().unwrap(),
            )),
        },
    }
}

pub fn create_test_offer_with_metadata_id(offer_id: Uuid, metadata_id: Uuid) -> OfferRecord {
    OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable: 5000000,
            min_sendable: 1000,
            metadata_id,
            metadata: None,
            timestamp: Utc::now(),
            expires: Some(Utc::now() + chrono::Duration::hours(24)),
        },
    }
}

// Helper function to create metadata and offer together for database constraint compliance
pub async fn create_test_offer_with_metadata<S>(
    store: &S,
    offer_id: Uuid,
) -> (OfferRecord, OfferMetadata)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    // Create metadata first
    let metadata_id = Uuid::new_v4();
    let metadata = create_test_offer_metadata(metadata_id);
    store.post_metadata(metadata.clone()).await.unwrap();

    // Create offer that references the metadata
    let offer = create_test_offer_with_existing_metadata(offer_id, metadata_id);

    (offer, metadata)
}

// OfferStore tests
pub async fn test_get_nonexistent_offer<S>(store: S)
where
    S: OfferStore,
    <S as OfferStore>::Error: std::fmt::Debug,
{
    let id = Uuid::new_v4();
    let result = store.get_offer("default", &id, None).await.unwrap();
    assert!(result.is_none());
}

pub async fn test_post_new_offer<S>(store: S)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let offer_id = Uuid::new_v4();
    let (offer, _metadata) = create_test_offer_with_metadata(&store, offer_id).await;

    let result = store.post_offer(offer.clone()).await.unwrap();
    assert_eq!(result, Some(offer.id));

    let retrieved = store.get_offer("default", &offer_id, None).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, offer_id);
}

pub async fn test_post_existing_offer<S>(store: S)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let offer_id = Uuid::new_v4();
    let (offer, _metadata) = create_test_offer_with_metadata(&store, offer_id).await;

    let result1 = store.post_offer(offer.clone()).await.unwrap();
    assert_eq!(result1, Some(offer.id));

    let result2 = store.post_offer(offer.clone()).await.unwrap();
    assert_eq!(result2, None);
}

pub async fn test_put_new_offer<S>(store: S)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let offer_id = Uuid::new_v4();
    let (offer, _metadata) = create_test_offer_with_metadata(&store, offer_id).await;

    let was_created = store.put_offer(offer.clone()).await.unwrap();
    assert!(was_created);

    let retrieved = store.get_offer("default", &offer_id, None).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, offer_id);
}

pub async fn test_put_existing_offer<S>(store: S)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let offer_id = Uuid::new_v4();
    let (mut offer, _metadata) = create_test_offer_with_metadata(&store, offer_id).await;

    let was_created1 = store.put_offer(offer.clone()).await.unwrap();
    assert!(was_created1);

    offer.offer.max_sendable = 2000;
    let was_created2 = store.put_offer(offer.clone()).await.unwrap();
    assert!(!was_created2);

    let retrieved = store.get_offer("default", &offer_id, None).await.unwrap();
    assert_eq!(retrieved.unwrap().offer.max_sendable, 2000);
}

pub async fn test_delete_existing_offer<S>(store: S)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let offer_id = Uuid::new_v4();
    let (offer, _metadata) = create_test_offer_with_metadata(&store, offer_id).await;

    store.put_offer(offer.clone()).await.unwrap();

    let deleted = store.delete_offer("default", &offer_id).await.unwrap();
    assert!(deleted);

    let retrieved = store.get_offer("default", &offer_id, None).await.unwrap();
    assert!(retrieved.is_none());
}

pub async fn test_delete_nonexistent_offer<S>(store: S)
where
    S: OfferStore,
    <S as OfferStore>::Error: std::fmt::Debug,
{
    let id = Uuid::new_v4();

    let deleted = store.delete_offer("default", &id).await.unwrap();
    assert!(!deleted);
}

pub async fn test_get_offers<S>(store: S)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let mut expected_offers = Vec::new();

    for i in 0..10 {
        let offer_id = Uuid::from_u128(i as u128);
        let (offer, _metadata) = create_test_offer_with_metadata(&store, offer_id).await;
        store.put_offer(offer.clone()).await.unwrap();
        expected_offers.push(offer);
    }

    let all_offers = store.get_offers("default", 0, 100).await.unwrap();
    assert_eq!(all_offers.as_slice(), expected_offers.as_slice());

    let first_five = store.get_offers("default", 0, 5).await.unwrap();
    assert_eq!(first_five.as_slice(), &expected_offers[0..5]);

    let next_five = store.get_offers("default", 5, 5).await.unwrap();
    assert_eq!(next_five.as_slice(), &expected_offers[5..10]);

    let middle_offers = store.get_offers("default", 3, 4).await.unwrap();
    assert_eq!(middle_offers.as_slice(), &expected_offers[3..7]);

    let last_offers = store.get_offers("default", 8, 10).await.unwrap();
    assert_eq!(last_offers.as_slice(), &expected_offers[8..10]);

    let beyond_offers = store.get_offers("default", 15, 5).await.unwrap();
    assert_eq!(beyond_offers.len(), 0);

    let zero_count = store.get_offers("default", 0, 0).await.unwrap();
    assert_eq!(zero_count.len(), 0);

    let single_offer = store.get_offers("default", 5, 1).await.unwrap();
    assert_eq!(single_offer.as_slice(), &expected_offers[5..6]);
}

// OfferMetadataStore tests
pub async fn test_get_nonexistent_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let id = Uuid::new_v4();
    let result = store.get_metadata("default", &id).await.unwrap();
    assert!(result.is_none());
}

pub async fn test_post_new_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let metadata = create_test_offer_metadata(Uuid::new_v4());
    let metadata_id = metadata.id;

    let result = store.post_metadata(metadata.clone()).await.unwrap();
    assert_eq!(result, Some(metadata.id));

    let retrieved = store.get_metadata("default", &metadata_id).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, metadata_id);
}

pub async fn test_post_existing_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let metadata = create_test_offer_metadata(Uuid::new_v4());

    let result1 = store.post_metadata(metadata.clone()).await.unwrap();
    assert_eq!(result1, Some(metadata.id));

    let result2 = store.post_metadata(metadata.clone()).await.unwrap();
    assert_eq!(result2, None);
}

pub async fn test_put_new_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let metadata = create_test_offer_metadata(Uuid::new_v4());
    let metadata_id = metadata.id;

    let was_created = store.put_metadata(metadata.clone()).await.unwrap();
    assert!(was_created);

    let retrieved = store.get_metadata("default", &metadata_id).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, metadata_id);
}

pub async fn test_put_existing_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let mut metadata = create_test_offer_metadata(Uuid::new_v4());
    let metadata_id = metadata.id;

    let was_created1 = store.put_metadata(metadata.clone()).await.unwrap();
    assert!(was_created1);

    metadata.metadata.text = "updated text".to_string();
    let was_created2 = store.put_metadata(metadata.clone()).await.unwrap();
    assert!(!was_created2);

    let retrieved = store.get_metadata("default", &metadata_id).await.unwrap();
    assert_eq!(retrieved.unwrap().metadata.text, "updated text");
}

pub async fn test_delete_existing_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let metadata = create_test_offer_metadata(Uuid::new_v4());
    let metadata_id = metadata.id;

    store.put_metadata(metadata.clone()).await.unwrap();

    let deleted = store
        .delete_metadata("default", &metadata_id)
        .await
        .unwrap();
    assert!(deleted);

    let retrieved = store.get_metadata("default", &metadata_id).await.unwrap();
    assert!(retrieved.is_none());
}

pub async fn test_delete_nonexistent_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let id = Uuid::new_v4();

    let deleted = store.delete_metadata("default", &id).await.unwrap();
    assert!(!deleted);
}

pub async fn test_get_all_offer_metadata<S>(store: S)
where
    S: OfferMetadataStore,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
{
    let mut expected_metadata = Vec::new();

    for i in 0..10 {
        let metadata_id = Uuid::from_u128(i as u128);
        let metadata = create_test_offer_metadata(metadata_id);
        store.put_metadata(metadata.clone()).await.unwrap();
        expected_metadata.push(metadata);
    }

    let all_metadata = store.get_all_metadata("default", 0, 100).await.unwrap();
    assert_eq!(all_metadata.as_slice(), expected_metadata.as_slice());

    let first_five = store.get_all_metadata("default", 0, 5).await.unwrap();
    assert_eq!(first_five.as_slice(), &expected_metadata[0..5]);

    let next_five = store.get_all_metadata("default", 5, 5).await.unwrap();
    assert_eq!(next_five.as_slice(), &expected_metadata[5..10]);

    let middle_metadata = store.get_all_metadata("default", 3, 4).await.unwrap();
    assert_eq!(middle_metadata.as_slice(), &expected_metadata[3..7]);

    let last_metadata = store.get_all_metadata("default", 8, 10).await.unwrap();
    assert_eq!(last_metadata.as_slice(), &expected_metadata[8..10]);

    let beyond_metadata = store.get_all_metadata("default", 15, 5).await.unwrap();
    assert_eq!(beyond_metadata.len(), 0);

    let zero_count = store.get_all_metadata("default", 0, 0).await.unwrap();
    assert_eq!(zero_count.len(), 0);

    let single_metadata = store.get_all_metadata("default", 5, 1).await.unwrap();
    assert_eq!(single_metadata.as_slice(), &expected_metadata[5..6]);
}

pub async fn setup_store_with_offer_and_metadata<S>(store: S) -> (S, Uuid, Uuid)
where
    S: OfferStore + OfferMetadataStore,
{
    // Create metadata first
    let metadata_id = Uuid::new_v4();
    let metadata = OfferMetadata {
        id: metadata_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: "Test LNURL offer".to_string(),
            long_text: Some("This is a comprehensive test of the LNURL offer system".to_string()),
            image: Some(OfferMetadataImage::Png(vec![0x89, 0x50, 0x4E, 0x47])),
            identifier: Some(OfferMetadataIdentifier::Email(
                "test@lnurl.com".parse().unwrap(),
            )),
        },
    };
    store.post_metadata(metadata).await.unwrap();

    // Create offer that references the metadata
    let offer_id = Uuid::new_v4();
    let offer = create_test_offer_with_metadata_id(offer_id, metadata_id);
    store.post_offer(offer).await.unwrap();

    (store, offer_id, metadata_id)
}

pub async fn test_post_offer_with_missing_metadata<S>(store: S)
where
    S: OfferStore,
    <S as OfferStore>::Error: std::fmt::Debug + Into<OfferStoreError>,
{
    let offer_id = Uuid::new_v4();
    let non_existent_metadata_id = Uuid::new_v4();

    let offer = OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable: 1000,
            min_sendable: 100,
            metadata_id: non_existent_metadata_id, // This metadata doesn't exist
            metadata: None,
            timestamp: Utc::now(),
            expires: Some(Utc::now() + chrono::Duration::seconds(3600)),
        },
    };

    let result = store.post_offer(offer).await;
    assert!(result.is_err());

    let error: OfferStoreError = result.unwrap_err().into();
    match error.source() {
        OfferStoreErrorSourceKind::InvalidInput(_) => {
            assert_eq!(error.esource(), ServiceErrorSource::Downstream);
        }
        _ => panic!(
            "Expected NotFound or HttpStatus(400) error, got {:?}",
            error.source()
        ),
    }
}

pub async fn test_put_offer_with_missing_metadata<S>(store: S)
where
    S: OfferStore,
    <S as OfferStore>::Error: std::fmt::Debug + Into<OfferStoreError>,
{
    let offer_id = Uuid::new_v4();
    let non_existent_metadata_id = Uuid::new_v4();

    let offer = OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable: 1000,
            min_sendable: 100,
            metadata_id: non_existent_metadata_id, // This metadata doesn't exist
            metadata: None,
            timestamp: Utc::now(),
            expires: Some(Utc::now() + chrono::Duration::seconds(3600)),
        },
    };

    let result = store.put_offer(offer).await;
    assert!(result.is_err());

    let error: OfferStoreError = result.unwrap_err().into();
    match error.source() {
        OfferStoreErrorSourceKind::InvalidInput(_) => {
            assert_eq!(error.esource(), ServiceErrorSource::Downstream);
        }
        _ => panic!(
            "Expected NotFound or HttpStatus(400) error, got {:?}",
            error.source()
        ),
    }
}

pub async fn test_delete_metadata_with_referencing_offers<S>(store: S)
where
    S: OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug + Into<OfferStoreError>,
    <S as OfferMetadataStore>::Error: std::fmt::Debug + Into<OfferStoreError>,
{
    // Create metadata
    let metadata_id = Uuid::new_v4();
    let metadata = create_test_offer_metadata(metadata_id);
    store.put_metadata(metadata).await.unwrap();

    // Create offer that references this metadata
    let offer_id = Uuid::new_v4();
    let offer = create_test_offer_with_existing_metadata(offer_id, metadata_id);
    store.put_offer(offer).await.unwrap();

    // Try to delete the metadata - should fail because offer references it
    let result = store.delete_metadata("default", &metadata_id).await;
    assert!(result.is_err());

    let error: OfferStoreError = result.unwrap_err().into();
    match error.source() {
        OfferStoreErrorSourceKind::InvalidInput(_) => {
            assert_eq!(error.esource(), ServiceErrorSource::Downstream);
        }
        _ => panic!("Expected InvalidInput, got {:?}", error.source()),
    }

    // Delete the referencing offer
    let delete_result = store.delete_offer("default", &offer_id).await.unwrap();
    assert!(delete_result);

    // Second attempt to delete the metadata should succeed
    let second_result = store
        .delete_metadata("default", &metadata_id)
        .await
        .unwrap();
    assert!(second_result);
}
