use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
    DiscoveryBackendSparse, DiscoveryBackendStore,
};
use switchgear_service::components::discovery::file::FileDiscoveryBackendStore;
use tempfile::TempDir;
use url::Url;

use crate::common::discovery;

fn create_temp_store() -> (FileDiscoveryBackendStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = FileDiscoveryBackendStore::new(temp_dir.path());
    (store, temp_dir)
}

#[tokio::test]
async fn test_file_post_new_backend_returns_address() {
    let (store, _temp_dir) = create_temp_store();
    discovery::test_post_new_backend_returns_address(store).await;
}

#[tokio::test]
async fn test_file_get_returns_correct_backends() {
    let (store, _temp_dir) = create_temp_store();
    discovery::test_get_returns_correct_backends(store).await;
}

#[tokio::test]
async fn test_file_delete_removes_and_returns_backends() {
    let (store, _temp_dir) = create_temp_store();
    discovery::test_delete_removes_target(store).await;
}

#[tokio::test]
async fn test_file_put_new_backend_returns_true() {
    let (store, _temp_dir) = create_temp_store();
    discovery::test_put_new_backend_returns_true(store).await;
}

#[tokio::test]
async fn test_file_put_existing_backend_updates_and_returns_false() {
    let (store, _temp_dir) = create_temp_store();
    discovery::test_put_existing_backend_updates_and_returns_false(store).await;
}

// Tests for shared file access between two FileDiscoveryBackendStore instances

fn create_temp_store_pair() -> (
    FileDiscoveryBackendStore,
    FileDiscoveryBackendStore,
    TempDir,
) {
    let temp_dir = TempDir::new().unwrap();
    let store1 = FileDiscoveryBackendStore::new(temp_dir.path());
    let store2 = FileDiscoveryBackendStore::new(temp_dir.path());
    (store1, store2, temp_dir)
}

fn create_test_backend(
    partition: &str,
    port: u16,
    weight: usize,
    enabled: bool,
) -> DiscoveryBackend {
    DiscoveryBackend {
        address: DiscoveryBackendAddress::Url(
            Url::parse(&format!("https://192.168.1.1:{port}")).unwrap(),
        ),
        backend: DiscoveryBackendSparse {
            partitions: [partition.to_string()].into(),
            weight,
            enabled,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        },
    }
}

#[tokio::test]
async fn test_file_shared_post_operations_between_clones() {
    let (store1, store2, _temp_dir) = create_temp_store_pair();

    let backend1 = create_test_backend("default", 8080, 100, true);
    let backend2 = create_test_backend("default", 8081, 200, true);

    // Post backend1 to store1
    let addr = store1.post(backend1.clone()).await.unwrap();
    assert_eq!(addr, Some(backend1.address.clone()));

    // Verify store2 can see backend1
    let retrieved = store2.get(&backend1.address).await.unwrap().unwrap();
    assert_eq!(retrieved, backend1);

    // Post backend2 to store2
    let addr = store2.post(backend2.clone()).await.unwrap();
    assert_eq!(addr, Some(backend2.address.clone()));

    // Verify store1 can see backend2
    let retrieved = store1.get(&backend2.address).await.unwrap().unwrap();
    assert_eq!(retrieved, backend2);

    // Verify both stores see both backends
    let all_from_store1 = store1.get_all().await.unwrap();
    let all_from_store2 = store2.get_all().await.unwrap();

    assert_eq!(all_from_store1.len(), 2);
    assert_eq!(all_from_store2.len(), 2);

    let addrs_from_store1: std::collections::HashSet<_> =
        all_from_store1.iter().map(|b| &b.address).collect();
    let addrs_from_store2: std::collections::HashSet<_> =
        all_from_store2.iter().map(|b| &b.address).collect();

    assert_eq!(addrs_from_store1, addrs_from_store2);
    assert!(addrs_from_store1.contains(&backend1.address));
    assert!(addrs_from_store1.contains(&backend2.address));
}

#[tokio::test]
async fn test_file_shared_put_operations_between_clones() {
    let (store1, store2, _temp_dir) = create_temp_store_pair();

    let backend = create_test_backend("default", 8080, 100, true);
    let updated_backend = create_test_backend("default", 8080, 300, false);

    // Put backend via store1
    let was_new = store1.put(backend.clone()).await.unwrap();
    assert!(was_new);

    // Verify store2 can see the backend
    let retrieved = store2.get(&backend.address).await.unwrap().unwrap();
    assert_eq!(retrieved, backend);

    // Update backend via store2
    let was_new = store2.put(updated_backend.clone()).await.unwrap();
    assert!(!was_new); // Should be false since it's an update

    // Verify store1 can see the updated backend
    let retrieved = store1.get(&updated_backend.address).await.unwrap().unwrap();
    assert_eq!(retrieved, updated_backend);
    assert_ne!(retrieved, backend);
}

#[tokio::test]
async fn test_file_shared_delete_operations_between_clones() {
    let (store1, store2, _temp_dir) = create_temp_store_pair();

    let backend1 = create_test_backend("default", 8080, 100, true);
    let backend2 = create_test_backend("default", 8081, 200, true);

    // Add backends via store1
    store1.post(backend1.clone()).await.unwrap();
    store1.post(backend2.clone()).await.unwrap();

    // Verify store2 can see both backends
    let all_backends = store2.get_all().await.unwrap();
    assert_eq!(all_backends.len(), 2);

    // Delete backend1 via store2
    let was_deleted = store2.delete(&backend1.address).await.unwrap();
    assert!(was_deleted);

    // Verify store1 can no longer see backend1
    let retrieved = store1.get(&backend1.address).await.unwrap();
    assert!(retrieved.is_none());

    // Verify store1 can still see backend2
    let retrieved = store1.get(&backend2.address).await.unwrap().unwrap();
    assert_eq!(retrieved, backend2);

    // Verify both stores have consistent state
    let all_from_store1 = store1.get_all().await.unwrap();
    let all_from_store2 = store2.get_all().await.unwrap();

    assert_eq!(all_from_store1.len(), 1);
    assert_eq!(all_from_store2.len(), 1);

    // Check that both stores see the same remaining backend
    let addrs_from_store1: std::collections::HashSet<_> =
        all_from_store1.iter().map(|b| &b.address).collect();
    let addrs_from_store2: std::collections::HashSet<_> =
        all_from_store2.iter().map(|b| &b.address).collect();

    assert_eq!(addrs_from_store1, addrs_from_store2);
    assert!(addrs_from_store1.contains(&backend2.address));
}

#[tokio::test]
async fn test_file_shared_mixed_crud_operations_between_instances() {
    let (store1, store2, _temp_dir) = create_temp_store_pair();

    let backend1 = create_test_backend("default", 8080, 100, true);
    let backend2 = create_test_backend("default", 8081, 200, true);
    let backend3 = create_test_backend("default", 8082, 300, true);
    let updated_backend2 = create_test_backend("default", 8081, 999, false);

    // 1. Store1 posts backend1
    store1.post(backend1.clone()).await.unwrap();

    // 2. Store2 posts backend2
    store2.post(backend2.clone()).await.unwrap();

    // 3. Store1 puts backend3 (new)
    let was_new = store1.put(backend3.clone()).await.unwrap();
    assert!(was_new);

    // 4. Store2 updates backend2
    let was_new = store2.put(updated_backend2.clone()).await.unwrap();
    assert!(!was_new);

    // 5. Verify all changes are visible from store1
    let retrieved1 = store1.get(&backend1.address).await.unwrap().unwrap();
    assert_eq!(retrieved1, backend1);

    let retrieved2 = store1.get(&backend2.address).await.unwrap().unwrap();
    assert_eq!(retrieved2, updated_backend2);
    assert_ne!(retrieved2, backend2);

    let retrieved3 = store1.get(&backend3.address).await.unwrap().unwrap();
    assert_eq!(retrieved3, backend3);

    // 6. Store1 deletes backend1
    let was_deleted = store1.delete(&backend1.address).await.unwrap();
    assert!(was_deleted);

    // 7. Verify final state is consistent across both stores
    let all_from_store1 = store1.get_all().await.unwrap();
    let all_from_store2 = store2.get_all().await.unwrap();

    assert_eq!(all_from_store1.len(), 2);
    assert_eq!(all_from_store2.len(), 2);

    // Verify both stores see the same backends (order-independent)
    let addrs_from_store1: std::collections::HashSet<_> =
        all_from_store1.iter().map(|b| &b.address).collect();
    let addrs_from_store2: std::collections::HashSet<_> =
        all_from_store2.iter().map(|b| &b.address).collect();

    assert_eq!(addrs_from_store1, addrs_from_store2);
    assert!(addrs_from_store1.contains(&updated_backend2.address));
    assert!(addrs_from_store1.contains(&backend3.address));
    assert!(!addrs_from_store1.contains(&backend1.address));
}

#[tokio::test]
async fn test_file_shared_operations_across_different_partitions() {
    let (store1, store2, _temp_dir) = create_temp_store_pair();

    let backend_default = create_test_backend("default", 8080, 100, true);
    let backend_test = create_test_backend("test", 8080, 200, true);

    // Add backends to different partitions via different stores
    store1.post(backend_default.clone()).await.unwrap();
    store2.post(backend_test.clone()).await.unwrap();

    // Verify each store can see both partitions correctly
    let default_from_store1 = store1.get_all().await.unwrap();
    let default_from_store2 = store2.get_all().await.unwrap();
    let test_from_store1 = store1.get_all().await.unwrap();
    let test_from_store2 = store2.get_all().await.unwrap();

    assert_eq!(default_from_store1.len(), 1);
    assert_eq!(default_from_store2.len(), 1);
    assert_eq!(test_from_store1.len(), 1);
    assert_eq!(test_from_store2.len(), 1);

    // Check contents are consistent (order-independent)
    let default_addrs_1: std::collections::HashSet<_> =
        default_from_store1.iter().map(|b| &b.address).collect();
    let default_addrs_2: std::collections::HashSet<_> =
        default_from_store2.iter().map(|b| &b.address).collect();
    let test_addrs_1: std::collections::HashSet<_> =
        test_from_store1.iter().map(|b| &b.address).collect();
    let test_addrs_2: std::collections::HashSet<_> =
        test_from_store2.iter().map(|b| &b.address).collect();

    assert_eq!(default_addrs_1, default_addrs_2);
    assert_eq!(test_addrs_1, test_addrs_2);

    // Verify backends are in correct partitions
    assert_eq!(default_from_store1[0], backend_default);
    assert_eq!(test_from_store1[0], backend_test);
}

#[tokio::test]
async fn test_file_handle_filesystem_deletion() {
    let (store, temp_dir) = create_temp_store();

    let backend1 = create_test_backend("default", 8080, 100, true);
    let backend2 = create_test_backend("default", 8081, 200, true);
    let backend3 = create_test_backend("other", 8082, 300, true);

    // Add backends to two different partitions
    store.post(backend1.clone()).await.unwrap();
    store.post(backend2.clone()).await.unwrap();
    store.post(backend3.clone()).await.unwrap();

    // Verify backends are stored correctly
    let all_default = store.get_all().await.unwrap();
    assert_eq!(all_default.len(), 2);

    let all_other = store.get_all().await.unwrap();
    assert_eq!(all_other.len(), 1);

    // Delete the "default" partition file from the filesystem
    let default_file_path = temp_dir.path().join("default.json");
    assert!(default_file_path.exists());
    std::fs::remove_file(&default_file_path).unwrap();
    assert!(!default_file_path.exists());

    // Test that get operations on deleted partition return None/empty without error
    let result = store.get(&backend1.address).await.unwrap();
    assert!(result.is_none());

    let result = store.get(&backend2.address).await.unwrap();
    assert!(result.is_none());

    let all_default = store.get_all().await.unwrap();
    assert_eq!(all_default.len(), 0);

    // Test that operations on other partition still work correctly
    let result = store.get(&backend3.address).await.unwrap();
    assert_eq!(result, Some(backend3.clone()));

    let all_other = store.get_all().await.unwrap();
    assert_eq!(all_other.len(), 1);
    assert_eq!(all_other[0], backend3);

    // Test that we can add new backends to the deleted partition (recreates file)
    let backend4 = create_test_backend("default", 8083, 400, true);
    let addr = store.post(backend4.clone()).await.unwrap();
    assert_eq!(addr, Some(backend4.address.clone()));

    // Verify the partition file was recreated
    assert!(default_file_path.exists());

    // Verify the new backend is stored correctly
    let result = store.get(&backend4.address).await.unwrap();
    assert_eq!(result, Some(backend4.clone()));

    let all_default = store.get_all().await.unwrap();
    assert_eq!(all_default.len(), 1);
    assert_eq!(all_default[0], backend4);
}
