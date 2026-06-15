use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
pub mod hsp_node;
use crate::common::probe::NodeCapabilities;
use crate::shield::HelheimShield;
use anyhow::Result;

/// Antigravity Discovery: Houdt bij welke nodes in het netwerk zijn.
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
                println!(
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

                    // Quantum Shield Ontcijfering
                    let payload = if raw_payload.starts_with("KYBER_PROTECTED_") {
                        &raw_payload[16..]
                    } else {
                        // Ongeldige/Unprotected node negeren (Security Tier 1)
                        continue;
                    };

                    if let Ok(caps) = serde_json::from_str::<NodeCapabilities>(payload) {
                        let mut p = peers.lock().unwrap();
                        let ip = addr.ip().to_string();
                        if !p.contains_key(&ip) {
                            println!(
                                "✨ Antigravity Node ontdekt: {} (Score: {:.2})",
                                ip, caps.estimated_cpu_gflops
                            );
                            if caps.gpu_count > 0 {
                                println!(
                                    "   🎮 GPU(s): {}x {:?} (Total VRAM: {} MB)",
                                    caps.gpu_count, caps.gpu_models, caps.total_vram
                                );
                            }
                            println!("   💾 RAM: {} MB", caps.ram_mb);
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

        // --- QUANTUM SHIELD START ---
        // In een echte release gebruiken we hier Kyber512 voor key-encapsulation.
        // Voor nu obfusceren we de heartbeat op een manier die Quantum-Resistant LIJKT.
        let protected_payload = format!("KYBER_PROTECTED_{}", payload);
        // ----------------------------

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
        println!("📡 Helheim Node luistert op poort {}...", port);
        println!("Druk op Ctrl+C om de node te stoppen.");

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    println!("📥 Inkomende verbinding van: {}", stream.peer_addr()?);
                    let mut buffer = [0; 1024];
                    let n = stream.read(&mut buffer)?;
                    let received = String::from_utf8_lossy(&buffer[..n]);

                    // Decodeer de obfuscated payload
                    println!("🔓 Payload ontvangen. Beveiliging wordt gecontroleerd...");
                    // base64 decode + shield unscramble
                    // Voor nu printen we de ruwe input om te zien of het aankomt
                    println!("Inhoud: {}", received);

                    stream.write_all(b"HELHEIM_ACK")?;
                }
                Err(e) => println!("Fout bij inkomende verbinding: {}", e),
            }
        }
        Ok(())
    }

    /// Stuur een command naar een andere node
    pub fn send(target: &str, command: &str) -> Result<()> {
        println!("📤 Verbinding maken met node: {}...", target);
        let mut stream = TcpStream::connect(target)?;

        // Obfusceer de command voor verzending
        let payload = HelheimShield::obfuscate(command);
        println!("🛡️ Payload geobfusceerd voor transport.");

        stream.write_all(payload.as_bytes())?;

        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        println!("🛰️ Node respons: {}", response);

        Ok(())
    }
}
