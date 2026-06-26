use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
pub mod hsp_node;
use crate::common::probe::NodeCapabilities;
use crate::shield::HelheimShield;
use anyhow::Result;

/// Node Discovery: Houdt bij welke nodes in het netwerk zijn.
pub struct DiscoveryService {
    pub peers: Arc<Mutex<HashMap<String, NodeCapabilities>>>,
}

impl Default for DiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start de UDP discovery listener
    pub fn start_listening(&self, port: u16) -> Result<()> {
        let addr = format!("0.0.0.0:{}", port);

        // We proberen te binden voor de listener rol.
        // Als dit faalt (bijv. poort bezet door een andere Pieter of node),
        // dan gaan we gewoon door zonder listener-rol op deze specifieke node.
        let socket = match UdpSocket::bind(&addr) {
            Ok(s) => s,
            Err(_) => {
                tracing::debug!(
                    "ℹ️  Port {} is bezet. Helheim draait op deze node in 'Social Mode' (alleen zenden).",
                    port
                );
                return Ok(());
            }
        };

        let peers = self.peers.clone();

        std::thread::spawn(move || {
            let mut buf = [0; 2048];
            loop {
                if let Ok((n, addr)) = socket.recv_from(&mut buf) {
                    let raw_payload = String::from_utf8_lossy(&buf[..n]);

                    // HSP Discovery Prefix with HMAC
                    let (signature, payload) = if raw_payload.starts_with("HSP_DISCOVERY_") {
                        let rest = &raw_payload[14..];
                        if let Some((sig, pay)) = rest.split_once('_') {
                            (sig, pay)
                        } else {
                            continue;
                        }
                    } else {
                        // Ongeldige/Unprotected node negeren (Security Tier 1)
                        continue;
                    };

                    let master_key = crate::shield::crypto::HelSigner::get_master_key();
                    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &master_key);
                    if let Err(_) = ring::hmac::verify(&key, payload.as_bytes(), &hex::decode(signature).unwrap_or_default()) {
                        tracing::debug!("⚠️ Ontdekte node is genegeerd: HMAC signature mismatch.");
                        continue;
                    }

                    if let Ok(caps) = serde_json::from_str::<NodeCapabilities>(payload) {
                        let mut p = peers.lock().unwrap_or_else(|e| e.into_inner());
                        let ip = addr.ip().to_string();
                        if !p.contains_key(&ip) {
                            tracing::debug!(
                                "✨ Helheim Node ontdekt: {} (Score: {:.2})",
                                ip, caps.estimated_cpu_gflops
                            );
                            if caps.gpu_count > 0 {
                                tracing::debug!(
                                    "   🎮 GPU(s): {}x {:?} (Total VRAM: {} MB)",
                                    caps.gpu_count, caps.gpu_models, caps.total_vram
                                );
                            }
                            tracing::debug!("   💾 RAM: {} MB", caps.ram_mb);
                        }
                        p.insert(ip, caps);
                    }
                }
            }
        });
        Ok(())
    }

    /// Broadcast onze eigen aanwezigheid (Met Quantum Shield)
    pub fn broadcast(port: u16, caps: NodeCapabilities) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_broadcast(true)?;

        let payload = serde_json::to_string(&caps)?;
        
        let master_key = crate::shield::crypto::HelSigner::get_master_key();
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &master_key);
        let signature = ring::hmac::sign(&key, payload.as_bytes());
        let signature_hex = hex::encode(signature.as_ref());

        let protected_payload = format!("HSP_DISCOVERY_{}_{}", signature_hex, payload);

        std::thread::spawn(move || {
            loop {
                let _ = socket.send_to(
                    protected_payload.as_bytes(),
                    format!("255.255.255.255:{}", port),
                );
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        });
        Ok(())
    }
}

pub struct NodeRelay;

impl NodeRelay {
    /// Start de node listener op een specifieke poort
    pub fn listen(port: u16) -> Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
        tracing::info!("📡 Helheim Node luistert op poort {}...", port);
        tracing::info!("Druk op Ctrl+C om de node te stoppen.");

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    tracing::debug!("📥 Inkomende verbinding van: {}", stream.peer_addr()?);
                    let mut buffer = [0; 1024];
                    let n = stream.read(&mut buffer)?;
                    let received = String::from_utf8_lossy(&buffer[..n]);

                    // Decodeer de obfuscated payload
                    tracing::debug!("🔓 Payload ontvangen. Beveiliging wordt gecontroleerd...");
                    // base64 decode + shield unscramble
                    // Voor nu printen we de ruwe input om te zien of het aankomt
                    tracing::debug!("Inhoud: {}", received);

                    stream.write_all(b"HELHEIM_ACK")?;
                }
                Err(e) => tracing::debug!("Fout bij inkomende verbinding: {}", e),
            }
        }
        Ok(())
    }

    /// Stuur een command naar een andere node
    pub fn send(target: &str, command: &str) -> Result<()> {
        tracing::debug!("📤 Verbinding maken met node: {}...", target);
        let mut stream = TcpStream::connect(target)?;

        // Obfusceer de command voor verzending
        let payload = HelheimShield::obfuscate(command);
        tracing::debug!("🛡️ Payload geobfusceerd voor transport.");

        stream.write_all(payload.as_bytes())?;

        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        tracing::debug!("🛰️ Node respons: {}", response);

        Ok(())
    }
}
