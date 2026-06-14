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

    /// HSP: Helheim Secure Protocol Encryption (Reversible)
    /// Wraps payload in a Noise Shell.
    pub fn encrypt_packet(input: &str) -> String {
        let mut rng = rand::rng();
        let raw_bytes = input.as_bytes();
        let mut buffer = Vec::with_capacity(raw_bytes.len() + 1);

        let key: u8 = rng.random_range(1..255);
        buffer.push(key); // Header: [KEY]

        for (i, &b) in raw_bytes.iter().enumerate() {
            // Cipher: Byte XOR Key XOR PositionRotator
            let cipher_byte = b ^ key ^ (i as u8 % 11);
            buffer.push(cipher_byte);
        }

        STANDARD.encode(&buffer)
    }

    /// HSP: De-Noise & Decrypt
    pub fn decrypt_packet(input: &str) -> anyhow::Result<String> {
        let decoded = STANDARD.decode(input)?;
        if decoded.is_empty() {
            return Err(anyhow::anyhow!("Empty Packet"));
        }

        let key = decoded[0];
        let payload = &decoded[1..];
        let mut decrypted = Vec::with_capacity(payload.len());

        for (i, &b) in payload.iter().enumerate() {
            let plain_byte = b ^ key ^ (i as u8 % 11);
            decrypted.push(plain_byte);
        }

        Ok(String::from_utf8(decrypted)?)
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
