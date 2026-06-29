use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use lazy_static::lazy_static;
use rand::Rng;

pub mod cage;
pub mod crypto;
pub mod wire;

/// De kern van de Helheim Shield: Chaos & Bescherming
pub struct HelheimShield;

impl HelheimShield {
    /// Geavanceerde obfuscatie: XOR met roterende sleutel + Junk-data injectie
    pub fn obfuscate(input: &str) -> String {
        let mut rng = rand::rng();
        let raw_bytes = input.as_bytes();
        let mut obfuscated = Vec::with_capacity(raw_bytes.len() * 2);

        let dynamic_key: u8 = rng.random_range(1..254);
        obfuscated.push(dynamic_key); // Eerste byte is de key voor deze payload

        for (i, &b) in raw_bytes.iter().enumerate() {
            // Roterende XOR + positie afhankelijkheid
            let val = b ^ dynamic_key ^ (i as u8 % 7);
            obfuscated.push(val);

            // Injecteer junk data om scrapers te verwarren
            if rng.random_bool(0.3) {
                obfuscated.push(rng.random());
            }
        }

        STANDARD.encode(&obfuscated)
    }



    /// HSP: Helheim Secure Protocol Encryption (ChaCha20-Poly1305 AEAD) with specific session key
    pub fn encrypt_packet_with_key(input: &str, key: &[u8; 32]) -> String {
        use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, SealingKey, UnboundKey, CHACHA20_POLY1305};
        let mut rng = rand::rng();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes);
        
        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, key)
            .expect("ChaCha20 key init: key is exactly 32 bytes by type");

        struct OneNonceSequence(Option<Nonce>);
        impl NonceSequence for OneNonceSequence {
            fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
                self.0.take().ok_or(ring::error::Unspecified)
            }
        }

        let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .expect("ChaCha20 nonce init: nonce is exactly 12 bytes by type");
        let mut sealing_key = SealingKey::new(unbound_key, OneNonceSequence(Some(nonce)));

        let mut in_out = input.as_bytes().to_vec();
        sealing_key.seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .expect("ChaCha20 seal: fresh nonce sequence cannot be exhausted");
        
        let mut result = nonce_bytes.to_vec();
        result.extend(in_out);
        STANDARD.encode(&result)
    }

    /// HSP: De-Noise & Decrypt (ChaCha20-Poly1305 AEAD) with specific session key
    pub fn decrypt_packet_with_key(input: &str, key: &[u8; 32]) -> anyhow::Result<String> {
        use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, UnboundKey, CHACHA20_POLY1305};
        let decoded = STANDARD.decode(input)?;
        if decoded.len() < 12 + CHACHA20_POLY1305.tag_len() {
            return Err(anyhow::anyhow!("Packet too short"));
        }
        
        let nonce_bytes: [u8; 12] = decoded[0..12].try_into()
            .map_err(|_| anyhow::anyhow!("Malformed packet: nonce slice wrong size"))?;
        let mut in_out = decoded[12..].to_vec();

        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, key)
            .map_err(|_| anyhow::anyhow!("ChaCha20 key init failed: invalid key size"))?;
        struct OneNonceSequence(Option<Nonce>);
        impl NonceSequence for OneNonceSequence {
            fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
                self.0.take().ok_or(ring::error::Unspecified)
            }
        }
        let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .map_err(|_| anyhow::anyhow!("ChaCha20 nonce init failed: invalid nonce size"))?;
        let mut opening_key = OpeningKey::new(unbound_key, OneNonceSequence(Some(nonce)));
        
        let decrypted_data = opening_key.open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("Decryption failed"))?;
            
        Ok(String::from_utf8(decrypted_data.to_vec())?)
    }
}

pub struct EcdhSession {
    pub private_key: Option<ring::agreement::EphemeralPrivateKey>,
    pub public_key: Vec<u8>,
}

impl EcdhSession {
    pub fn new() -> Self {
        use ring::agreement;
        use ring::rand::SystemRandom;
        let rng = SystemRandom::new();
        let private_key = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng)
            .expect("ECDH keygen: OS RNG unavailable");
        let mut public_key = vec![0u8; 32];
        public_key.copy_from_slice(private_key.compute_public_key()
            .expect("ECDH pubkey: X25519 compute_public_key is infallible").as_ref());
        Self { private_key: Some(private_key), public_key }
    }

    pub fn derive_shared_key(&mut self, peer_pub_key: &[u8]) -> anyhow::Result<[u8; 32]> {
        use ring::agreement;
        let peer_pub = agreement::UnparsedPublicKey::new(&agreement::X25519, peer_pub_key);
        let priv_key = self.private_key.take().ok_or_else(|| anyhow::anyhow!("Session al gebruikt!"))?;
        
        agreement::agree_ephemeral(priv_key, &peer_pub, |key_material| {
            use ring::hkdf;
            let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, b"HelheimSwarmHandshake");
            let prk = salt.extract(key_material);
            let info = [b"session_key".as_ref()];
            let okm = prk.expand(&info, hkdf::HKDF_SHA256)
                .expect("HKDF expand: known-good output length");
            let mut derived = [0u8; 32];
            okm.fill(&mut derived)
                .expect("HKDF fill: output length matches HKDF_SHA256 constraint");
            derived
        }).map_err(|_| anyhow::anyhow!("ECDH Agreement failed"))
    }
}

lazy_static! {
    static ref IS_UNLOCKED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
}

/// [BOSS] HelheimLock: Voorkomt dat onbevoegden de rauwe laag (Rune/Quantum) aanpassen.
pub struct HelheimLock;
impl HelheimLock {
    /// Onze eigen 'Hel-Hash' - Onbreekbaar door bit-shuffling & alchemistische constanten.
    pub fn hel_hash(input: &str) -> u64 {
        let mut h: u64 = 0xDEADC0DEBAADF00D;
        for (i, &b) in input.as_bytes().iter().enumerate() {
            h ^= (b as u64) << (i % 8 * 8);
            h = h.rotate_left(13).wrapping_add(0x9E3779B97F4A7C15);
            h ^= h >> 33;
            h = h.wrapping_mul(0xFF51AFD7ED558CCD);
            h ^= h >> 33;
            h = h.wrapping_mul(0xC4CEB9FE1A85EC53);
            h ^= h >> 33;
        }
        h
    }

    pub fn unlock(key: &str) -> bool {
        use crate::shield::crypto::HelSigner;
        if let Ok(sig_bytes) = base64::engine::general_purpose::STANDARD.decode(key.trim()) {
            if HelSigner::verify_update(b"UNLOCK_COMMAND", &sig_bytes).is_ok() {
                IS_UNLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
                return true;
            }
        }
        false
    }

    pub fn is_authorized() -> bool {
        // Check memory only (Fix for K3: removed world-readable /tmp token file)
        IS_UNLOCKED.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Hel-Modus: Memory Scrambling. Houdt data veilig in RAM.
#[allow(dead_code)]
pub struct MemScrambler;

impl MemScrambler {
    pub unsafe fn scramble(ptr: *mut u8, len: usize, key: u8) {
        unsafe {
            for i in 0..len {
                let p = ptr.add(i);
                *p = *p ^ key ^ (i as u8 % 17);
            }
        }
    }
    pub unsafe fn unscramble(ptr: *mut u8, len: usize, key: u8) {
        unsafe {
            Self::scramble(ptr, len, key);
        }
    }
}

pub fn shield_encrypt_helheim(input: &str) -> String {
    HelheimShield::obfuscate(input)
}
