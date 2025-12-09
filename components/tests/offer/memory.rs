use switchgear_components::offer::memory::MemoryOfferStore;

use crate::common::offer;

#[tokio::test]
async fn test_memory_get_nonexistent_offer() {
    let store = MemoryOfferStore::default();
    offer::test_get_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_memory_post_new_offer() {
    let store = MemoryOfferStore::default();
    offer::test_post_new_offer(store).await;
}

#[tokio::test]
async fn test_memory_post_existing_offer() {
    let store = MemoryOfferStore::default();
    offer::test_post_existing_offer(store).await;
}

#[tokio::test]
async fn test_memory_put_new_offer() {
    let store = MemoryOfferStore::default();
    offer::test_put_new_offer(store).await;
}

#[tokio::test]
async fn test_memory_put_existing_offer() {
    let store = MemoryOfferStore::default();
    offer::test_put_existing_offer(store).await;
}

#[tokio::test]
async fn test_memory_delete_existing_offer() {
    let store = MemoryOfferStore::default();
    offer::test_delete_existing_offer(store).await;
}

#[tokio::test]
async fn test_memory_delete_nonexistent_offer() {
    let store = MemoryOfferStore::default();
    offer::test_delete_nonexistent_offer(store).await;
}

#[tokio::test]
async fn test_memory_get_offers() {
    let store = MemoryOfferStore::default();
    offer::test_get_offers(store).await;
}

#[tokio::test]
async fn test_memory_get_nonexistent_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_get_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_post_new_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_post_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_post_existing_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_post_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_put_new_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_put_new_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_put_existing_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_put_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_delete_existing_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_delete_existing_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_delete_nonexistent_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_delete_nonexistent_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_get_all_offer_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_get_all_offer_metadata(store).await;
}

#[tokio::test]
async fn test_memory_post_offer_with_missing_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_post_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_memory_put_offer_with_missing_metadata() {
    let store = MemoryOfferStore::default();
    offer::test_put_offer_with_missing_metadata(store).await;
}

#[tokio::test]
async fn test_memory_delete_metadata_with_referencing_offers() {
    let store = MemoryOfferStore::default();
    offer::test_delete_metadata_with_referencing_offers(store).await;
}
