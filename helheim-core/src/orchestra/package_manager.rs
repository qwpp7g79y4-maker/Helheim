//! Package Manager + Post-Quantum / Ed25519 Signing (Vraag 4)
//! Decentralized Helheim "Cargo".
//! - Fetches .so (FFI) and .hel packages from local path, HTTP, or Swarm.
//! - Verifies signature using existing Shield/Crypto (HelSigner::verify_update with Ed25519 master key).
//!   (Blueprint mentions Kyber/Dilithium; current impl uses the embedded Ed25519 master for compatibility.
//!    Easy to extend with pqcrypto when Dilithium support is wired in shield/crypto.rs).
//! - Only after successful verification: hands off to NativeModuleLoader.
//! - Zero-overhead after import: verified modules are cached exactly like before.
//! - P2P via existing DistributedMemory / Swarm (request package over HSP).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use dashmap::DashMap;
use tokio::fs;

use crate::ffi::WasmModuleLoader;
use crate::orchestra::distributed::DistributedMemory;
use crate::shield::crypto::HelSigner;

/// A verified, signed module ready for loading.
#[derive(Clone)]
pub struct VerifiedModule {
    pub name: String,
    pub version: String,
    pub data: Vec<u8>,           // the raw .so bytes (or .hel for pure)
    pub signature: Vec<u8>,
    pub is_native: bool,         // true = .wasm FFI, false = pure .hel
}

/// Package manifest (embedded or sidecar).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub kind: String,            // "wasm" or "hel"
    pub description: Option<String>,
}

/// The PackageManager.
/// Owns a cache of verified modules and wraps the WasmModuleLoader.
#[derive(Clone)]
pub struct PackageManager {
    verified_cache: Arc<DashMap<String, VerifiedModule>>,
    loader: Arc<tokio::sync::Mutex<WasmModuleLoader>>,
    search_paths: Vec<PathBuf>,
}

impl PackageManager {
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self {
            verified_cache: Arc::new(DashMap::new()),
            loader: Arc::new(tokio::sync::Mutex::new(WasmModuleLoader::new(search_paths.clone()))),
            search_paths,
        }
    }

    // DEMO ONLY - nooit in productie aanroepen.
    pub fn sign(manifest: &PackageManifest, data: &[u8], _private_key: &[u8]) -> Vec<u8> {
        // For demo we use ring to create a signature.
        // Real flow: use the master private key offline, embed only public in HelSigner.
        use ring::signature::Ed25519KeyPair;
        // NOTE: In real deployment the private key never lives in the binary.
        // This is only for the sketch.
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).expect("Systeem RNG niet beschikbaar");
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("Generatie van keypair faalde");
        
        let manifest_json = serde_json::to_vec(manifest).expect("Failed to serialize manifest");
        let mut signed_payload = Vec::with_capacity(manifest_json.len() + data.len());
        signed_payload.extend_from_slice(&manifest_json);
        signed_payload.extend_from_slice(data);
        
        key_pair.sign(&signed_payload).as_ref().to_vec()
    }

    /// Verify signature using the existing Helheim master key (Ed25519).
    /// Returns Ok(()) if the signature matches the embedded master public key.
    pub fn verify(data: &[u8], signature: &[u8]) -> Result<()> {
        HelSigner::verify_update(data, signature)
    }

    /// Fetch a package from local filesystem, HTTP, or Swarm.
    /// `source` can be:
    ///   - local package name or relative path
    ///   - http://... 
    ///   - swarm:node_id/package_name  (uses DistributedMemory)
    async fn fetch(&self, source: &str, _distributed: &DistributedMemory) -> Result<(Vec<u8>, PackageManifest, Vec<u8>)> {
        if source.starts_with("swarm:") {
            // [PARKED] Phase 2 - HSP P2P packages
            anyhow::bail!("[PARKED] P2P package fetching over HSP ('swarm://') is explicitly parked for a future Swarm release.");
        } else if source.starts_with("http") || source.starts_with("https") {
            // [W·AG·AF] SSRF Protection: Restrict HTTP fetching to official registries only
            if !source.starts_with("https://pkg.helheim.dev/") && !source.starts_with("https://registry.helheim.dev/") {
                anyhow::bail!("SSRF Protection: Packages can only be downloaded from official Helheim registries (https://pkg.helheim.dev/ of https://registry.helheim.dev/). Local network or arbitrary domain fetching is prohibited.");
            }
            let resp = reqwest::get(source).await?.bytes().await?;
            let (manifest, sig, data) = self.parse_signed_blob(&resp)?;
            Ok((data, manifest, sig))
        } else {
            // Package resolving using configured search paths
            if source.contains("..") || source.starts_with('/') || source.contains('\\') || source.contains(':') {
                anyhow::bail!("Path traversal detected. Package source '{}' must be a valid package name or relative filename, not an absolute path or traversal sequence.", source);
            }
            
            let mut found_path = None;
            for base in &self.search_paths {
                let candidate = base.join(source);
                
                // Extra security: Ensure the resolved path is strictly inside the search path
                if let Ok(canonical_base) = std::fs::canonicalize(base) {
                    if let Ok(canonical_candidate) = std::fs::canonicalize(&candidate) {
                        if canonical_candidate.starts_with(&canonical_base) && canonical_candidate.is_file() {
                            found_path = Some(candidate);
                            break;
                        }
                    } else if candidate.is_file() {
                        // Fallback if canonicalize fails but file exists (e.g. symlinks)
                        // Make sure the absolute path still starts with the base absolute path
                        let abs_candidate = std::env::current_dir().unwrap_or_default().join(&candidate);
                        let abs_base = std::env::current_dir().unwrap_or_default().join(base);
                        if abs_candidate.starts_with(&abs_base) {
                            found_path = Some(candidate);
                            break;
                        }
                    }
                }
            }

            let path = found_path.ok_or_else(|| anyhow::anyhow!("Package '{}' not found in any search path.", source))?;
            
            let mut data = fs::read(&path).await?;
            let sig_path = path.with_extension("sig");
            let sig = if sig_path.exists() {
                fs::read(&sig_path).await?
            } else {
                if data.len() > 64 {
                    let extracted_sig = data[data.len()-64..].to_vec();
                    data.truncate(data.len() - 64);
                    extracted_sig
                } else {
                    vec![]
                }
            };
            let manifest = self.try_parse_manifest(&data).unwrap_or_else(|| PackageManifest {
                name: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "unknown".to_string()),
                version: "0.0.0".into(),
                kind: if path.extension().map_or(false, |e| e == "wasm") { "wasm".into() } else { "hel".into() },
                description: None,
            });
            Ok((data, manifest, sig))
        }
    }

    fn parse_signed_blob(&self, blob: &[u8]) -> Result<(PackageManifest, Vec<u8>, Vec<u8>)> {
        if blob.len() < 8 {
            anyhow::bail!("Blob too small");
        }
        let mut cursor = 0usize;
        
        if cursor + 4 > blob.len() { anyhow::bail!("Malformed blob: missing manifest length"); }
        let manifest_len = u32::from_le_bytes(
            blob[cursor..cursor+4].try_into()
                .map_err(|_| anyhow::anyhow!("Malformed blob: manifest length slice error"))?)
            as usize;
        cursor += 4;

        if cursor + manifest_len > blob.len() { anyhow::bail!("Malformed blob: incomplete manifest"); }
        let manifest_json = &blob[cursor..cursor+manifest_len];
        cursor += manifest_len;

        if cursor + 4 > blob.len() { anyhow::bail!("Malformed blob: missing signature length"); }
        let sig_len = u32::from_le_bytes(
            blob[cursor..cursor+4].try_into()
                .map_err(|_| anyhow::anyhow!("Malformed blob: signature length slice error"))?)
            as usize;
        cursor += 4;
        
        if cursor + sig_len > blob.len() { anyhow::bail!("Malformed blob: incomplete signature"); }
        let signature = blob[cursor..cursor+sig_len].to_vec();
        cursor += sig_len;
        
        let data = blob[cursor..].to_vec();

        let manifest: PackageManifest = serde_json::from_slice(manifest_json)?;
        Ok((manifest, signature, data))
    }

    fn try_parse_manifest(&self, data: &[u8]) -> Option<PackageManifest> {
        if let Ok(manifest) = serde_json::from_slice::<PackageManifest>(data) {
            Some(manifest)
        } else {
            None
        }
    }

    pub async fn import_signed(
        &self,
        name: &str,
        source: &str,
        provided_sig: Option<&[u8]>,
        distributed: &DistributedMemory,
    ) -> Result<VerifiedModule> {
        if let Some(existing) = self.verified_cache.get(name) {
            return Ok(existing.clone());
        }

        let (data, manifest, fetched_sig) = self.fetch(source, distributed).await?;

        let signature = if let Some(sig) = provided_sig {
            sig.to_vec()
        } else {
            fetched_sig
        };

        // [W·AG·AF] C1 Review: Reconstruct the signed payload to cover BOTH manifest and data
        // This prevents attackers from spoofing the manifest name/version for a valid data blob
        let manifest_json = serde_json::to_vec(&manifest)
            .context("Failed to serialize manifest for verification")?;
        
        let mut signed_payload = Vec::with_capacity(manifest_json.len() + data.len());
        signed_payload.extend_from_slice(&manifest_json);
        signed_payload.extend_from_slice(&data);

        // Verify using the existing Shield/Crypto API over the FULL payload
        HelSigner::verify_update(&signed_payload, &signature)
            .context("Signature verification failed for package (manifest + data mismatch)")?;

        // Additional manifest sanity check (name must match)
        if manifest.name != name {
            anyhow::bail!("Package name mismatch: expected {}, got {}", name, manifest.name);
        }

        let verified = VerifiedModule {
            name: name.to_string(),
            version: manifest.version,
            data,
            signature,
            is_native: manifest.kind == "wasm" || manifest.kind == "ffi", // keep ffi for back compat during tests if needed
        };

        self.verified_cache.insert(name.to_string(), verified.clone());

        // If it's a native Wasm module, we can eagerly load it into the WasmModuleLoader
        // (verification already passed).
        if verified.is_native {
            let mut loader = self.loader.lock().await;
            // We pass a dummy user_data; real callers will provide the real HelFFIContext later.
            // The important part is that we only reach here after crypto verification.
            let _ = loader.load(name, std::ptr::null_mut())?;
        }

        tracing::info!("[PACKAGE] Successfully imported and verified signed package '{}'", name);
        Ok(verified)
    }

    /// Get a previously verified module (safe to use).
    pub fn get_verified(&self, name: &str) -> Option<VerifiedModule> {
        self.verified_cache.get(name).map(|v| v.clone())
    }

    /// Convenience: load a verified native module into the FFI layer.
    /// This is the safe gateway that the rest of the system should use instead of raw WasmModuleLoader.
    pub async fn load_verified_native(
        &self,
        name: &str,
        user_data_addr: usize,
    ) -> Result<Arc<crate::ffi::LoadedWasmModule>> {
        if self.verified_cache.get(name).is_none() {
            anyhow::bail!("Package '{}' has not been imported and verified yet. Use installeer_ondertekend first.", name);
        }

        let mut loader = self.loader.lock().await;
        loader.load(name, user_data_addr as *mut std::ffi::c_void)
    }
    /// Eagerly loads a local native module without signature verification.
    /// ONLY for use by the core engine (e.g., loading standard libraries).
    pub async fn import_local_trusted(&self, name: &str, path: &std::path::Path) -> Result<VerifiedModule> {
        let data = tokio::fs::read(path).await?;
        let verified = VerifiedModule {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            data,
            signature: vec![],
            is_native: true, // Native wasm
        };

        self.verified_cache.insert(name.to_string(), verified.clone());

        let mut loader = self.loader.lock().await;
        // Dummy user_data, real callers will provide it when running functions
        let _ = loader.load(name, std::ptr::null_mut())?;

        tracing::info!("[PACKAGE] Trusted internal module '{}' ingeladen.", name);
        Ok(verified)
    }

    /// Add a search path to the internal WasmModuleLoader
    pub async fn add_search_path(&self, path: std::path::PathBuf) {
        let mut loader = self.loader.lock().await;
        loader.add_search_path(path);
    }

    /// Proxy to get a loaded Wasm module
    pub async fn get_native(&self, name: &str) -> Option<Arc<crate::ffi::LoadedWasmModule>> {
        let loader = self.loader.lock().await;
        loader.get(name)
    }

    /// Proxy to reload a Wasm module (hot-reloading)
    pub async fn reload_native(&self, name: &str, user_data_addr: usize) -> Result<Arc<crate::ffi::LoadedWasmModule>> {
        let mut loader = self.loader.lock().await;
        loader.reload(name, user_data_addr as *mut std::ffi::c_void)
    }
}