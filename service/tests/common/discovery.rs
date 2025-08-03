use std::collections::HashSet;
use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
    DiscoveryBackendSparse, DiscoveryBackendStore,
};
use url::Url;

pub fn gen_backends() -> (DiscoveryBackend, DiscoveryBackend, DiscoveryBackend) {
    let new_backend1 = DiscoveryBackend {
        partition: "default".to_string(),
        address: DiscoveryBackendAddress::Url(Url::parse("https://192.168.1.1:8080").unwrap()),
        backend: DiscoveryBackendSparse {
            weight: 100,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        },
    };

    let new_backend2 = DiscoveryBackend {
        partition: "default".to_string(),
        address: DiscoveryBackendAddress::Url(Url::parse("https://192.168.1.1:8081").unwrap()),
        backend: DiscoveryBackendSparse {
            weight: 200,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        },
    };

    let modified_backend2 = DiscoveryBackend {
        partition: "default".to_string(),
        address: DiscoveryBackendAddress::Url(Url::parse("https://192.168.1.1:8081").unwrap()),
        backend: DiscoveryBackendSparse {
            weight: 10,
            enabled: false,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        },
    };

    (new_backend1, new_backend2, modified_backend2)
}

pub async fn test_post_new_backend_returns_address<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    // Test posting new backends returns their addresses
    let addr = store.post(new_backend1.clone()).await.unwrap();
    assert_eq!(addr, Some(new_backend1.address.clone()));

    let addr = store.post(new_backend2.clone()).await.unwrap();
    assert_eq!(addr, Some(new_backend2.address.clone()));

    // Test posting duplicate returns None
    let addr = store.post(modified_backend2.clone()).await.unwrap();
    assert_eq!(addr, None);
}

pub async fn test_get_returns_correct_backends<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    let _ = store.post(new_backend1.clone()).await.unwrap();
    let _ = store.post(new_backend2.clone()).await.unwrap();
    let _ = store.post(modified_backend2.clone()).await.unwrap();

    // Test individual gets
    let backend = store
        .get("default", &new_backend1.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, new_backend1);

    let backend = store
        .get("default", &new_backend2.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, new_backend2);

    // Modified backend should not have been stored
    let backend = store
        .get("default", &modified_backend2.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, new_backend2);
    assert_ne!(backend, modified_backend2);

    // Test get_all returns all backends (order-independent)
    let all_backends = store.get_all("default").await.unwrap();
    assert_eq!(all_backends.len(), 2);
    let backend_addresses: HashSet<_> = all_backends.iter().map(|b| &b.address).collect();
    assert!(backend_addresses.contains(&new_backend1.address));
    assert!(backend_addresses.contains(&new_backend2.address));
}

pub async fn test_delete_removes_target<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    let _ = store.post(new_backend1.clone()).await.unwrap();
    let _ = store.post(new_backend2.clone()).await.unwrap();
    let _ = store.post(modified_backend2.clone()).await.unwrap();

    // Delete and verify return values
    let deleted = store
        .delete("default", &new_backend1.address)
        .await
        .unwrap();
    assert!(deleted);

    let deleted = store
        .delete("default", &new_backend2.address)
        .await
        .unwrap();
    assert!(deleted);

    // Modified backend was never stored, so delete returns None
    let deleted = store
        .delete("default", &modified_backend2.address)
        .await
        .unwrap();
    assert!(!deleted);

    // Verify all backends are gone
    assert!(store
        .get("default", &new_backend1.address)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .get("default", &new_backend2.address)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .get("default", &modified_backend2.address)
        .await
        .unwrap()
        .is_none());
}

pub async fn test_put_new_backend_returns_true<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    // Put on non-existent backends returns true (created)
    assert!(store.put(new_backend1.clone()).await.unwrap());
    assert!(store.put(new_backend2.clone()).await.unwrap());

    // Put on existing backend returns false (modified)
    assert!(!store.put(modified_backend2.clone()).await.unwrap());
}

pub async fn test_put_existing_backend_updates_and_returns_false<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    // Initial puts return true (created)
    assert!(store.put(new_backend1.clone()).await.unwrap());
    assert!(store.put(new_backend2.clone()).await.unwrap());

    // Verify initial state
    let backend = store
        .get("default", &new_backend1.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, new_backend1);

    let backend = store
        .get("default", &new_backend2.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, new_backend2);

    let all_backends = store.get_all("default").await.unwrap();
    assert_eq!(all_backends.len(), 2);
    let backend_addresses: HashSet<_> = all_backends.iter().map(|b| &b.address).collect();
    assert!(backend_addresses.contains(&new_backend1.address));
    assert!(backend_addresses.contains(&new_backend2.address));

    // Update backend2 returns false (modified)
    assert!(!store.put(modified_backend2.clone()).await.unwrap());

    // Verify update
    let backend = store
        .get("default", &modified_backend2.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, modified_backend2);
}
