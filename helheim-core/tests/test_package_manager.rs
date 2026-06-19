use helheim_core::orchestra::package_manager::{PackageManager, PackageManifest};
use helheim_core::orchestra::distributed::DistributedMemory;
use helheim_core::shield::crypto::HelSigner;
use std::sync::Arc;

#[tokio::test]
async fn test_package_path_traversal() {
    let pm = PackageManager::new(vec![]);
    let dist = DistributedMemory::new("test_node".to_string());
    
    // Test that 'import_signed' blocks path traversal attempts.
    let result = pm.import_signed("test", "../../../etc/passwd", None, &dist).await;
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("Path traversal detected"));
    }

    let result2 = pm.import_signed("test", "/etc/shadow", None, &dist).await;
    assert!(result2.is_err());
    if let Err(e) = result2 {
        assert!(e.to_string().contains("Path traversal detected"));
    }
}

#[tokio::test]
async fn test_manifest_spoofing_signature_failure() {
    let manifest = PackageManifest {
        name: "malicious_spoof".to_string(),
        version: "1.0.0".to_string(),
        kind: "ffi".to_string(),
        description: None,
    };
    
    // Suppose an attacker takes valid plugin data but changes the manifest to load under a spoofed name
    let fake_data = b"Some malicious payload";
    
    // We try to verify this with an invalid signature (since they don't have the master key)
    let bad_sig = vec![0u8; 64]; 
    
    // The signature check combines manifest + data
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let mut signed_payload = Vec::with_capacity(manifest_json.len() + fake_data.len());
    signed_payload.extend_from_slice(&manifest_json);
    signed_payload.extend_from_slice(fake_data);

    let result = HelSigner::verify_update(&signed_payload, &bad_sig);
    assert!(result.is_err(), "Spoofed manifest+data should fail signature verification");
}

// [W·AG·AF] Priority 7.2: Extra security tests (OOM, bounds, concurrent) added by AG
#[tokio::test]
async fn test_large_payload_signature_failure() {
    let manifest = PackageManifest {
        name: "large_payload".to_string(),
        version: "1.0.0".to_string(),
        kind: "ffi".to_string(),
        description: None,
    };
    
    // Create a 10MB malicious payload
    let fake_data = vec![0x42u8; 10 * 1024 * 1024];
    
    let bad_sig = vec![0u8; 64]; 
    
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let mut signed_payload = Vec::with_capacity(manifest_json.len() + fake_data.len());
    signed_payload.extend_from_slice(&manifest_json);
    signed_payload.extend_from_slice(&fake_data);

    let result = HelSigner::verify_update(&signed_payload, &bad_sig);
    assert!(result.is_err(), "Large spoofed manifest+data should fail signature verification cleanly without OOM");
}

#[tokio::test]
async fn test_invalid_signature_size() {
    let fake_data = b"Some malicious payload";
    let bad_sig = vec![0u8; 13]; // Invalid size (Ed25519 expects 64 bytes)
    
    let result = HelSigner::verify_update(fake_data, &bad_sig);
    assert!(result.is_err(), "Invalid signature size should fail gracefully");
}

#[tokio::test]
async fn test_concurrent_package_loads() {
    // Tests that 20 threads trying to hit the package manager concurrently (e.g. for path traversal)
    // are correctly and safely rejected without breaking internal DashMap state or causing race conditions.
    let pm = Arc::new(PackageManager::new(vec![]));
    let dist = Arc::new(DistributedMemory::new("test_node".to_string()));
    
    let mut set = tokio::task::JoinSet::new();
    
    for _ in 0..20 {
        let pm_clone = pm.clone();
        let dist_clone = dist.clone();
        set.spawn(async move {
            let result = pm_clone.import_signed("test", "../../../etc/passwd", None, &dist_clone).await;
            assert!(result.is_err());
            if let Err(e) = result {
                assert!(e.to_string().contains("Path traversal detected"));
            }
        });
    }
    
    while let Some(res) = set.join_next().await {
        res.expect("Task panicked");
    }
}
