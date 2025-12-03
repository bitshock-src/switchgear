use rand::Rng;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
    DiscoveryBackendPatch, DiscoveryBackendPatchSparse, DiscoveryBackendSparse,
    DiscoveryBackendStore,
};

pub fn gen_backends() -> (DiscoveryBackend, DiscoveryBackend, DiscoveryBackend) {
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();

    // Generate two random secret keys and derive public keys
    let secret_key1 = SecretKey::from_byte_array(rng.gen::<[u8; 32]>()).unwrap();
    let public_key1 = PublicKey::from_secret_key(&secp, &secret_key1);

    let secret_key2 = SecretKey::from_byte_array(rng.gen::<[u8; 32]>()).unwrap();
    let public_key2 = PublicKey::from_secret_key(&secp, &secret_key2);

    // Create the two distinct backends
    let backend1 = DiscoveryBackend {
        address: DiscoveryBackendAddress::PublicKey(public_key1),
        backend: DiscoveryBackendSparse {
            name: None,
            partitions: ["default".to_string()].into(),
            weight: 100,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        },
    };

    let backend2 = DiscoveryBackend {
        address: DiscoveryBackendAddress::PublicKey(public_key2),
        backend: DiscoveryBackendSparse {
            name: Some("new_backend2".to_string()),
            partitions: ["default".to_string()].into(),
            weight: 200,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        },
    };

    // Sort the two backends by address string representation
    let mut backends = [backend1, backend2];
    backends.sort_by(|a, b| a.address.to_string().cmp(&b.address.to_string()));

    let new_backend1 = backends[0].clone();
    let new_backend2 = backends[1].clone();

    // Create modified_backend2 with the same address as new_backend2
    let modified_backend2 = DiscoveryBackend {
        address: new_backend2.address.clone(),
        backend: DiscoveryBackendSparse {
            name: Some("new_backend2_modified".to_string()),
            partitions: ["default".to_string()].into(),
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
    let backend = store.get(&new_backend1.address).await.unwrap().unwrap();
    assert_eq!(backend, new_backend1);

    let backend = store.get(&new_backend2.address).await.unwrap().unwrap();
    assert_eq!(backend, new_backend2);

    // Modified backend should not have been stored
    let backend = store
        .get(&modified_backend2.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, new_backend2);
    assert_ne!(backend, modified_backend2);

    // Test get_all returns all backends in sorted order
    let actual_backends = store.get_all(None).await.unwrap().backends.unwrap();
    let expected_backends = vec![new_backend1.clone(), new_backend2.clone()];
    assert_eq!(actual_backends, expected_backends);
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
    let deleted = store.delete(&new_backend1.address).await.unwrap();
    assert!(deleted);

    let deleted = store.delete(&new_backend2.address).await.unwrap();
    assert!(deleted);

    // Modified backend was never stored, so delete returns None
    let deleted = store.delete(&modified_backend2.address).await.unwrap();
    assert!(!deleted);

    // Verify all backends are gone
    assert!(store.get(&new_backend1.address).await.unwrap().is_none());
    assert!(store.get(&new_backend2.address).await.unwrap().is_none());
    assert!(store
        .get(&modified_backend2.address)
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
    let backend = store.get(&new_backend1.address).await.unwrap().unwrap();
    assert_eq!(backend, new_backend1);

    let backend = store.get(&new_backend2.address).await.unwrap().unwrap();
    assert_eq!(backend, new_backend2);

    let actual_backends = store.get_all(None).await.unwrap().backends.unwrap();
    let expected_backends = vec![new_backend1.clone(), new_backend2.clone()];
    assert_eq!(actual_backends, expected_backends);

    // Update backend2 returns false (modified)
    assert!(!store.put(modified_backend2.clone()).await.unwrap());

    // Verify update
    let backend = store
        .get(&modified_backend2.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, modified_backend2);
}

pub async fn test_patch_backend<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    // Initial puts return true (created)
    assert!(store.put(new_backend1.clone()).await.unwrap());
    assert!(store.put(new_backend2.clone()).await.unwrap());

    // Verify initial state
    let backend = store.get(&new_backend1.address).await.unwrap().unwrap();
    assert_eq!(backend, new_backend1);

    let backend = store.get(&new_backend2.address).await.unwrap().unwrap();
    assert_eq!(backend, new_backend2);

    let actual_backends = store.get_all(None).await.unwrap().backends.unwrap();
    let expected_backends = vec![new_backend1.clone(), new_backend2.clone()];
    assert_eq!(actual_backends, expected_backends);

    // Patch backend2
    let backend_patch = DiscoveryBackendPatch {
        address: modified_backend2.address.clone(),
        backend: DiscoveryBackendPatchSparse {
            name: Some(modified_backend2.backend.name.clone()),
            partitions: None,
            weight: Some(modified_backend2.backend.weight),
            enabled: Some(modified_backend2.backend.enabled),
        },
    };
    let patched = store.patch(backend_patch).await.unwrap();
    assert!(patched);

    // Verify update
    let backend = store
        .get(&modified_backend2.address)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend, modified_backend2);
}

pub async fn test_patch_missing_backend<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, _, modified_backend2) = gen_backends();

    // Initial puts return true (created)
    assert!(store.put(new_backend1.clone()).await.unwrap());

    // Verify initial state
    let backend = store.get(&new_backend1.address).await.unwrap().unwrap();
    assert_eq!(backend, new_backend1);

    let actual_backends = store.get_all(None).await.unwrap().backends.unwrap();
    let expected_backends = vec![new_backend1.clone()];
    assert_eq!(actual_backends, expected_backends);

    // Patch backend2
    let backend_patch = DiscoveryBackendPatch {
        address: modified_backend2.address.clone(),
        backend: DiscoveryBackendPatchSparse {
            name: Some(modified_backend2.backend.name.clone()),
            partitions: None,
            weight: Some(modified_backend2.backend.weight),
            enabled: Some(modified_backend2.backend.enabled),
        },
    };
    let patched = store.patch(backend_patch).await.unwrap();
    assert!(!patched);
}

pub async fn test_etag_changes_on_mutations_get_all<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    // Get initial etag (should be for empty store)
    let initial_result = store.get_all(None).await.unwrap();
    let initial_etag = initial_result.etag;

    // Add first backend - etag should change
    let _ = store.post(new_backend1.clone()).await.unwrap();
    let result_after_add1 = store.get_all(None).await.unwrap();
    let etag_after_add1 = result_after_add1.etag;
    assert_ne!(
        etag_after_add1, initial_etag,
        "etag should change after adding first backend"
    );

    // Add second backend - etag should change again
    let _ = store.post(new_backend2.clone()).await.unwrap();
    let result_after_add2 = store.get_all(None).await.unwrap();
    let etag_after_add2 = result_after_add2.etag;
    assert_ne!(
        etag_after_add2, etag_after_add1,
        "etag should change after adding second backend"
    );

    // Modify backend using put - etag should change
    let _ = store.put(modified_backend2.clone()).await.unwrap();
    let result_after_put = store.get_all(None).await.unwrap();
    let etag_after_put = result_after_put.etag;
    assert_ne!(
        etag_after_put, etag_after_add2,
        "etag should change after modifying backend with put"
    );

    // Modify backend using patch - etag should change
    let backend_patch = DiscoveryBackendPatch {
        address: new_backend1.address.clone(),
        backend: DiscoveryBackendPatchSparse {
            name: Some(Some("patched_backend1".to_string())),
            partitions: None,
            weight: Some(150),
            enabled: None,
        },
    };
    let _ = store.patch(backend_patch).await.unwrap();
    let result_after_patch = store.get_all(None).await.unwrap();
    let etag_after_patch = result_after_patch.etag;
    assert_ne!(
        etag_after_patch, etag_after_put,
        "etag should change after patching backend"
    );

    // Modify backend using put again - etag should change
    let another_modified_backend2 = DiscoveryBackend {
        address: modified_backend2.address.clone(),
        backend: DiscoveryBackendSparse {
            name: Some("backend2_modified_again".to_string()),
            weight: 5,
            ..modified_backend2.backend.clone()
        },
    };
    let _ = store.put(another_modified_backend2).await.unwrap();
    let result_after_put2 = store.get_all(None).await.unwrap();
    let etag_after_put2 = result_after_put2.etag;
    assert_ne!(
        etag_after_put2, etag_after_patch,
        "etag should change after putting backend again"
    );

    // Delete a backend - etag should change
    let _ = store.delete(&new_backend1.address).await.unwrap();
    let result_after_delete = store.get_all(None).await.unwrap();
    let etag_after_delete = result_after_delete.etag;
    assert_ne!(
        etag_after_delete, etag_after_put2,
        "etag should change after deleting backend"
    );
}

pub async fn test_etag_conditional_get_all<S>(store: S)
where
    S: DiscoveryBackendStore,
    S::Error: std::fmt::Debug,
{
    let (new_backend1, new_backend2, modified_backend2) = gen_backends();

    // Get initial etag and backends
    let result1 = store.get_all(None).await.unwrap();
    let etag1 = result1.etag;

    // Add first backend
    let _ = store.post(new_backend1.clone()).await.unwrap();
    let result2 = store.get_all(None).await.unwrap();
    let etag2 = result2.etag;
    assert!(
        result2.backends.is_some(),
        "backends should be present when called with None"
    );

    // Calling get_all with the current etag should return None for backends
    let result_with_current_etag = store.get_all(Some(etag2)).await.unwrap();
    assert_eq!(
        result_with_current_etag.etag, etag2,
        "etag should be the same"
    );
    assert!(
        result_with_current_etag.backends.is_none(),
        "backends should be None when etag matches"
    );

    // Calling get_all with old etag should return backends (since data changed)
    let result_with_old_etag = store.get_all(Some(etag1)).await.unwrap();
    assert_eq!(
        result_with_old_etag.etag, etag2,
        "etag should be the current etag"
    );
    assert!(
        result_with_old_etag.backends.is_some(),
        "backends should be present when etag doesn't match"
    );
    assert_eq!(
        result_with_old_etag.backends.unwrap(),
        vec![new_backend1.clone()]
    );

    // Add second backend
    let _ = store.post(new_backend2.clone()).await.unwrap();
    let result3 = store.get_all(None).await.unwrap();
    let etag3 = result3.etag;

    // Old etag should now return updated backends
    let result_with_old_etag2 = store.get_all(Some(etag2)).await.unwrap();
    assert_eq!(
        result_with_old_etag2.etag, etag3,
        "etag should be the current etag"
    );
    assert!(
        result_with_old_etag2.backends.is_some(),
        "backends should be present when etag doesn't match"
    );
    let backends = result_with_old_etag2.backends.unwrap();
    assert_eq!(backends, vec![new_backend1.clone(), new_backend2.clone()]);

    // Current etag should return None for backends
    let result_with_current_etag3 = store.get_all(Some(etag3)).await.unwrap();
    assert_eq!(result_with_current_etag3.etag, etag3);
    assert!(
        result_with_current_etag3.backends.is_none(),
        "backends should be None when etag matches"
    );

    // Modify a backend with put
    let _ = store.put(modified_backend2.clone()).await.unwrap();
    let result4 = store.get_all(None).await.unwrap();
    let etag4 = result4.etag;

    // Old etag should return modified backends
    let result_with_old_etag3 = store.get_all(Some(etag3)).await.unwrap();
    assert_eq!(result_with_old_etag3.etag, etag4);
    assert!(
        result_with_old_etag3.backends.is_some(),
        "backends should be present after modification"
    );

    // Current etag should return None
    let result_with_current_etag4 = store.get_all(Some(etag4)).await.unwrap();
    assert!(
        result_with_current_etag4.backends.is_none(),
        "backends should be None when etag matches"
    );

    // Delete a backend
    let _ = store.delete(&new_backend1.address).await.unwrap();
    let result5 = store.get_all(None).await.unwrap();
    let etag5 = result5.etag;

    // Old etag should return updated backends after deletion
    let result_with_old_etag4 = store.get_all(Some(etag4)).await.unwrap();
    assert_eq!(result_with_old_etag4.etag, etag5);
    assert!(
        result_with_old_etag4.backends.is_some(),
        "backends should be present after deletion"
    );
    let backends_after_delete = result_with_old_etag4.backends.unwrap();
    assert_eq!(backends_after_delete, vec![modified_backend2.clone()]);

    // Current etag should return None
    let result_with_current_etag5 = store.get_all(Some(etag5)).await.unwrap();
    assert!(
        result_with_current_etag5.backends.is_none(),
        "backends should be None when etag matches"
    );
}
