use anyhow::Result;
use ring::signature::{ED25519, UnparsedPublicKey};

pub struct HelSigner;

/// 🔑 HELHEIM MASTER PUBLIC KEY (Ed25519) - Default Fallback
const HELHEIM_MASTER_KEY_DEFAULT: [u8; 32] = [
    0xff, 0xe7, 0xce, 0x5d, 0x2c, 0xbd, 0xfe, 0x0e, 0x70, 0xc2, 0xcd, 0x7d, 0x1c, 0x3f, 0xbd, 0xb8,
    0x8b, 0x52, 0xfc, 0x5d, 0x25, 0x2b, 0x8d, 0x9c, 0x9b, 0xbe, 0x1e, 0xd4, 0x5b, 0x77, 0x50, 0x47,
];

impl HelSigner {
    pub fn get_master_key() -> [u8; 32] {
        if let Ok(key_hex) = std::env::var("HELHEIM_MASTER_KEY") {
            if key_hex.len() == 64 {
                let mut key = [0u8; 32];
                for i in 0..32 {
                    if let Ok(b) = u8::from_str_radix(&key_hex[i*2..i*2+2], 16) {
                        key[i] = b;
                    }
                }
                return key;
            }
        }
        HELHEIM_MASTER_KEY_DEFAULT
    }

    /// Verify a signature against the Master Key (from ENV or embedded fallback)
    pub fn verify_update(binary_data: &[u8], signature_data: &[u8]) -> Result<()> {
        let master_key = Self::get_master_key();
        let peer_public_key = UnparsedPublicKey::new(&ED25519, &master_key);
        peer_public_key
            .verify(binary_data, signature_data)
            .map_err(|_| {
                anyhow::anyhow!("⛔ CRYPTO ALARM: Handtekening Ongeldig! Mogelijke MITM aanval.")
            })
    }

    pub fn verify_custom(public_key_bytes: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        let peer_public_key = UnparsedPublicKey::new(&ED25519, public_key_bytes);
        peer_public_key
            .verify(message, signature)
            .map_err(|_| anyhow::anyhow!("Signature Verification Failed"))
    }
}

use ring::signature::{Ed25519KeyPair, KeyPair};
use ring::rand::SystemRandom;

lazy_static::lazy_static! {
    /// Each node in the swarm generates a session keypair for signing continuations.
    /// In a real system, this could be persisted or managed by the Package Manager.
    pub static ref SWARM_KEYPAIR: Ed25519KeyPair = {
        let rng = SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).unwrap()
    };
}

pub struct SwarmSigner;

impl SwarmSigner {
    pub fn sign(message: &[u8]) -> Vec<u8> {
        SWARM_KEYPAIR.sign(message).as_ref().to_vec()
    }

    pub fn public_key() -> Vec<u8> {
        SWARM_KEYPAIR.public_key().as_ref().to_vec()
    }

    pub fn verify_peer(peer_pub: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        let peer_public_key = UnparsedPublicKey::new(&ED25519, peer_pub);
        peer_public_key
            .verify(message, signature)
            .map_err(|_| anyhow::anyhow!("⛔ SWARM ALARM: Continuation Handtekening Ongeldig! Payload is gemanipuleerd."))
    }
}

