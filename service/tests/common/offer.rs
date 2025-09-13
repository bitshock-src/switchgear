use chrono::Utc;
use sha2::Digest;
use std::collections::HashSet;
use switchgear_service::api::lnurl::LnUrlOfferMetadata;
use switchgear_service::api::offer::{
    OfferMetadata, OfferMetadataIdentifier, OfferMetadataImage, OfferMetadataSparse,
    OfferMetadataStore, OfferProvider, OfferRecord, OfferRecordSparse, OfferStore,
};
use switchgear_service::api::service::ServiceErrorSource;
use switchgear_service::components::offer::error::{OfferStoreError, OfferStoreErrorSourceKind};
use uuid::Uuid;

// Test data generators
pub fn create_test_offer_with_existing_metadata(id: Uuid, metadata_id: Uuid) -> OfferRecord {
    OfferRecord {
        partition: "default".to_string(),
        id,
        offer: OfferRecordSparse {
            max_sendable: 1000,
            min_sendable: 100,
            metadata_id,
            timestamp: Utc::now(),
            expires: Some(Utc::now() + chrono::Duration::seconds(3600)),
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
    let result = store.get_offer("default", &id).await.unwrap();
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

    let retrieved = store.get_offer("default", &offer_id).await.unwrap();
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

    let retrieved = store.get_offer("default", &offer_id).await.unwrap();
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

    let retrieved = store.get_offer("default", &offer_id).await.unwrap();
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

    let retrieved = store.get_offer("default", &offer_id).await.unwrap();
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
    let offer1_id = Uuid::new_v4();
    let offer2_id = Uuid::new_v4();
    let offer3_id = Uuid::new_v4();

    let (offer1, _metadata1) = create_test_offer_with_metadata(&store, offer1_id).await;
    let (offer2, _metadata2) = create_test_offer_with_metadata(&store, offer2_id).await;
    let (offer3, _metadata3) = create_test_offer_with_metadata(&store, offer3_id).await;

    store.put_offer(offer1.clone()).await.unwrap();
    store.put_offer(offer2.clone()).await.unwrap();
    store.put_offer(offer3.clone()).await.unwrap();

    let all_offers = store.get_offers("default").await.unwrap();
    assert_eq!(all_offers.len(), 3);

    let ids: HashSet<Uuid> = all_offers.iter().map(|o| o.id).collect();
    assert!(ids.contains(&offer1.id));
    assert!(ids.contains(&offer2.id));
    assert!(ids.contains(&offer3.id));
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
    let metadata1 = create_test_offer_metadata(Uuid::new_v4());
    let metadata2 = create_test_offer_metadata(Uuid::new_v4());
    let metadata3 = create_test_offer_metadata(Uuid::new_v4());

    store.put_metadata(metadata1.clone()).await.unwrap();
    store.put_metadata(metadata2.clone()).await.unwrap();
    store.put_metadata(metadata3.clone()).await.unwrap();

    let all_metadata = store.get_all_metadata("default").await.unwrap();
    assert_eq!(all_metadata.len(), 3);

    let ids: HashSet<Uuid> = all_metadata.iter().map(|m| m.id).collect();
    assert!(ids.contains(&metadata1.id));
    assert!(ids.contains(&metadata2.id));
    assert!(ids.contains(&metadata3.id));
}

// OfferProvider tests
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

pub async fn test_offer_provider_successful_retrieval<S>(store: S)
where
    S: OfferProvider + OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
    <S as OfferProvider>::Error: std::fmt::Debug,
{
    let (store, offer_id, _metadata_id) = setup_store_with_offer_and_metadata(store).await;

    let result = store
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
    assert_eq!(offer.metadata_json_hash, expected_hash.as_slice());
}

pub async fn test_offer_provider_offer_not_found<S>(store: S)
where
    S: OfferProvider,
    <S as OfferProvider>::Error: std::fmt::Debug,
{
    let non_existent_id = Uuid::new_v4();

    let result = store
        .offer("example.com", "default", &non_existent_id)
        .await
        .unwrap();

    assert!(result.is_none());
}

pub async fn test_offer_provider_metadata_not_found_or_foreign_key_constraint<S>(store: S)
where
    S: OfferProvider + OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug + Into<OfferStoreError>,
    <S as OfferProvider>::Error: std::fmt::Debug,
{
    // Create offer with valid metadata first
    let (store, offer_id, metadata_id) = setup_store_with_offer_and_metadata(store).await;

    // Delete the metadata to simulate missing metadata scenario
    // proper database will error here
    if let Err(e) = store.delete_metadata("default", &metadata_id).await {
        let e: OfferStoreError = e.into();
        if let OfferStoreErrorSourceKind::Database(d) = e.source() {
            if d.to_string().to_lowercase().contains("constraint") {
                return;
            }
        }
        panic!("Failed to delete metadata: {e}");
    }

    let result = store
        .offer("example.com", "default", &offer_id)
        .await
        .unwrap();

    // With missing metadata, should now return None instead of error
    assert!(result.is_none());
}

pub async fn test_offer_provider_hash_consistency<S>(store: S)
where
    S: OfferProvider + OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
    <S as OfferProvider>::Error: std::fmt::Debug,
{
    let (store, offer_id, _) = setup_store_with_offer_and_metadata(store).await;

    // Call offer method multiple times
    let result1 = store
        .offer("example.com", "default", &offer_id)
        .await
        .unwrap()
        .unwrap();
    let result2 = store
        .offer("example.com", "default", &offer_id)
        .await
        .unwrap()
        .unwrap();

    // Hash should be consistent across calls
    assert_eq!(result1.metadata_json_hash, result2.metadata_json_hash);
    assert_eq!(result1.metadata_json_string, result2.metadata_json_string);

    // Verify hash matches manual calculation
    let manual_hash = sha2::Sha256::digest(result1.metadata_json_string.as_bytes());
    assert_eq!(result1.metadata_json_hash, manual_hash.as_slice());
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

pub async fn test_offer_provider_different_metadata_different_hashes<S>(store: S)
where
    S: OfferProvider + OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
    <S as OfferProvider>::Error: std::fmt::Debug,
{
    // Create two different metadata entries
    let metadata1_id = Uuid::new_v4();
    let metadata1 = OfferMetadata {
        id: metadata1_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: "First offer".to_string(),
            long_text: None,
            image: None,
            identifier: None,
        },
    };
    store.post_metadata(metadata1).await.unwrap();

    let metadata2_id = Uuid::new_v4();
    let metadata2 = OfferMetadata {
        id: metadata2_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: "Second offer".to_string(),
            long_text: Some("Different description".to_string()),
            image: None,
            identifier: None,
        },
    };
    store.post_metadata(metadata2).await.unwrap();

    // Create two offers with different metadata
    let offer1_id = Uuid::new_v4();
    let offer1 = create_test_offer_with_metadata_id(offer1_id, metadata1_id);
    store.post_offer(offer1).await.unwrap();

    let offer2_id = Uuid::new_v4();
    let offer2 = create_test_offer_with_metadata_id(offer2_id, metadata2_id);
    store.post_offer(offer2).await.unwrap();

    // Retrieve both offers
    let result1 = store
        .offer("example.com", "default", &offer1_id)
        .await
        .unwrap()
        .unwrap();
    let result2 = store
        .offer("example.com", "default", &offer2_id)
        .await
        .unwrap()
        .unwrap();

    // Should have different metadata strings and hashes
    assert_ne!(result1.metadata_json_string, result2.metadata_json_string);
    assert_ne!(result1.metadata_json_hash, result2.metadata_json_hash);

    // Verify content differences
    assert!(result1.metadata_json_string.contains("First offer"));
    assert!(result2.metadata_json_string.contains("Second offer"));
    assert!(!result1
        .metadata_json_string
        .contains("Different description"));
    assert!(result2
        .metadata_json_string
        .contains("Different description"));
}

pub async fn test_offer_provider_valid_current_offer_returns_some<S>(store: S)
where
    S: OfferProvider + OfferStore + OfferMetadataStore,
    <S as OfferStore>::Error: std::fmt::Debug,
    <S as OfferMetadataStore>::Error: std::fmt::Debug,
    <S as OfferProvider>::Error: std::fmt::Debug,
{
    // Create metadata first
    let metadata_id = Uuid::new_v4();
    let metadata = OfferMetadata {
        id: metadata_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: "Valid current offer".to_string(),
            long_text: None,
            image: None,
            identifier: None,
        },
    };
    store.post_metadata(metadata).await.unwrap();

    // Create a valid current offer
    let offer_id = Uuid::new_v4();
    let valid_offer = OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable: 1000000,
            min_sendable: 1000,
            metadata_id,
            timestamp: Utc::now() - chrono::Duration::minutes(30), // Started 30 minutes ago
            expires: Some(Utc::now() + chrono::Duration::hours(1)), // Expires 1 hour from now
        },
    };
    store.post_offer(valid_offer.clone()).await.unwrap();

    // Try to get the valid offer through OfferProvider::offer
    let result = store
        .offer("example.com", "default", &offer_id)
        .await
        .unwrap();

    // Should return Some for valid offer
    assert!(result.is_some());
    let offer = result.unwrap();
    assert_eq!(offer.id, offer_id);
    assert_eq!(offer.max_sendable, valid_offer.offer.max_sendable);
    assert_eq!(offer.min_sendable, valid_offer.offer.min_sendable);
    assert!(offer.metadata_json_string.contains("Valid current offer"));
}
