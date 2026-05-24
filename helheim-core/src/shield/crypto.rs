use ring::signature::{UnparsedPublicKey, ED25519};
use anyhow::Result;

pub struct HelSigner;

/// 🔑 HELHEIM MASTER PUBLIC KEY (Ed25519)
/// Generated 2026-02-02. DO NOT CHANGE unless performing a Hard Fork.
const HELHEIM_MASTER_KEY: [u8; 32] = [
    0xff, 0xe7, 0xce, 0x5d, 0x2c, 0xbd, 0xfe, 0x0e, 
    0x70, 0xc2, 0xcd, 0x7d, 0x1c, 0x3f, 0xbd, 0xb8, 
    0x8b, 0x52, 0xfc, 0x5d, 0x25, 0x2b, 0x8d, 0x9c, 
    0x9b, 0xbe, 0x1e, 0xd4, 0x5b, 0x77, 0x50, 0x47
];

impl HelSigner {
    /// Verify a signature against the EMBEDDED Master Key
    pub fn verify_update(binary_data: &[u8], signature_data: &[u8]) -> Result<()> {
        let peer_public_key = UnparsedPublicKey::new(&ED25519, &HELHEIM_MASTER_KEY);
        peer_public_key.verify(binary_data, signature_data)
            .map_err(|_| anyhow::anyhow!("⛔ CRYPTO ALARM: Handtekening Ongeldig! Mogelijke MITM aanval."))
    }

    /// (Legacy/Debug) Verify with custom key
    pub fn verify_custom(public_key_bytes: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        let peer_public_key = UnparsedPublicKey::new(&ED25519, public_key_bytes);
        peer_public_key.verify(message, signature)
            .map_err(|_| anyhow::anyhow!("Signature Verification Failed"))
    }
}
