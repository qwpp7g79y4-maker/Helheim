use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use lazy_static::lazy_static;
use rand::Rng;

pub mod cage;
pub mod crypto;
pub mod governor;
pub mod trap;
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
        if decoded.len() < 1 {
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

    /// Geavanceerde Honeypot: Genereert extreem frustrerende "poep" data voor bots.
    pub fn generate_chaos_trap() -> String {
        let mut rng = rand::rng();
        let trap_type: u8 = rng.random_range(0..4);

        match trap_type {
            0 => {
                let user = format!("admin_{}", rng.random_range(1000..9999));
                let pass = STANDARD.encode(rng.random::<[u8; 16]>());
                format!("# CONFIG_VERSION: 1.33.7\nDB_USER={}\nDB_PASS={}\nDB_HOST=10.0.0.{} \n# LOGOUT_ON_SUCCESS=false", user, pass, rng.random_range(1..254))
            }
            1 => {
                let body = STANDARD.encode(rng.random::<[u8; 64]>());
                format!(
                    "-----BEGIN RSA PRIVATE KEY-----\n{}\n-----END RSA PRIVATE KEY-----",
                    body
                )
            }
            2 => {
                let mut data = "HELHEIM_INTERNAL_CORE_DUMP_0x00FF".to_string();
                for _ in 0..3 {
                    data = STANDARD.encode(data);
                }
                format!("[RECURSIVE_ENCRYPTED_STREAM]: {}", data)
            }
            _ => {
                let trash: String = (0..128).map(|_| rng.random::<char>()).collect();
                format!("[FATAL_KERNEL_ERROR]: {}", STANDARD.encode(trash))
            }
        }
    }

    /// De "Eliminator": genereert een oneindige stroom data die nooit stopt.
    pub fn infinite_stream_trap() -> impl Iterator<Item = String> {
        std::iter::repeat_with(|| {
            let mut rng = rand::rng();
            let chunk: String = (0..512).map(|_| rng.random::<char>()).collect();
            STANDARD.encode(chunk)
        })
    }

    /// Herken verdachte patronen
    pub fn is_suspicious(input: &str) -> bool {
        let input_lc = input.to_lowercase();
        let patterns = [
            "admin", "root", "password", "config", "passwd", "../", "eval", "sh ", "exec",
            "system", "sql", "select", "insert", "drop", "delete", "sudo",
        ];
        patterns.iter().any(|&p| input_lc.contains(p))
    }

    /// Dynamische Blacklist manager
    pub fn trigger_blacklist(identity: &str) {
        println!(
            "🚫 [ELIMINATIE]: Identiteit {} op de zwarte lijst gezet.",
            identity
        );
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
        // De 'Master Hash' berekend met onze hel_hash
        let target_hash: u64 = Self::hel_hash("HELL-MASTER-2026");

        if Self::hel_hash(key) == target_hash {
            // Persistent unlock via file token
            if let Ok(mut file) = std::fs::File::create("/tmp/helheim.token") {
                use std::io::Write;
                let _ = file.write_all(key.as_bytes());
            }
            IS_UNLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub fn is_authorized() -> bool {
        // Check memory first
        if IS_UNLOCKED.load(std::sync::atomic::Ordering::SeqCst) {
            return true;
        }

        // Check persistent file token
        if let Ok(content) = std::fs::read_to_string("/tmp/helheim.token") {
            let target_hash: u64 = Self::hel_hash("HELL-MASTER-2026");
            if Self::hel_hash(content.trim()) == target_hash {
                IS_UNLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
                return true;
            }
        }
        false
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
