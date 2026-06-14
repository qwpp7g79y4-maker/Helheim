use anyhow::Result;
use colored::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use dashmap::DashMap;

pub struct SwarmEngine;

lazy_static::lazy_static! {
    static ref CONNECTIONS: DashMap<String, tokio::net::TcpStream> = DashMap::new();
}

impl SwarmEngine {
    /// Start de Async Swarm Listener (TCP)
    /// Non-blocking, draait op de achtergrond.
    pub async fn ignite(
        port: u16,
        orchestrator: std::sync::Arc<crate::orchestra::Orchestrator>,
    ) -> Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        println!(
            "{}",
            format!("[SWARM]: HSP Node Active on port {}", port)
                .green()
                .bold()
        );

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut socket, addr)) => {
                        println!("⚡ Persistente Swarm Verbinding geaccepteerd: {}", addr);
                        let orchestrator_clone = orchestrator.clone();
                        tokio::spawn(async move {
                            let mut buf = vec![0; 1024 * 1024]; // 1MB buffer

                            loop {
                                // 1. Read Payload
                                match socket.read(&mut buf).await {
                                    Ok(n) if n == 0 => {
                                        println!("💤 Swarm Node losgekoppeld: {}", addr);
                                        return; // Connection gracefully closed
                                    }
                                    Ok(n) => {
                                        let raw_payload = String::from_utf8_lossy(&buf[..n]);

                                        // [HSP] Decryptie Layer
                                        match crate::shield::HelheimShield::decrypt_packet(
                                            &raw_payload,
                                        ) {
                                            Ok(decrypted) => {
                                                println!(
                                                    "[HSP]: 🔓 Payload ontsleuteld ({} bytes)",
                                                    decrypted.len()
                                                );

                                                let mut ctx = crate::common::context::ExecutionContext::sandbox();
                                                let mut execution_script = decrypted.as_str();

                                                if decrypted.starts_with("SIGNED: ") {
                                                    if let Some((sig_part, script_part)) = decrypted[8..].split_once(" | ") {
                                                        use base64::Engine;
                                                        if let Ok(sig_bytes) = base64::engine::general_purpose::STANDARD.decode(sig_part.trim()) {
                                                            if crate::shield::crypto::HelSigner::verify_update(script_part.as_bytes(), &sig_bytes).is_ok() {
                                                                println!("[SWARM]: ✅ Geldige Master Key handtekening. Elevated Privileges geactiveerd.");
                                                                ctx = crate::common::context::ExecutionContext::default_privileged();
                                                                execution_script = script_part;
                                                            } else {
                                                                println!("[SWARM]: ⚠️ Ongeldige handtekening. Fallback naar Sandbox.");
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    println!("[SWARM]: 🛡️ Ongesigneerd verzoek. Executie in Sandbox Mode.");
                                                }

                                                // Actively process the command via the Orchestrator
                                                let exec_result = orchestrator_clone
                                                    .process_command(execution_script, ctx)
                                                    .await;

                                                let (ack, _secure_resp) = match exec_result {
                                                    Ok(_) => ("SWARM_ACK_SUCCESS", "Success"),
                                                    Err(e) => {
                                                        println!(
                                                            "[SWARM]: ❌ Remote execution gefaald: {}",
                                                            e
                                                        );
                                                        ("SWARM_ACK_ERROR", "Error")
                                                    }
                                                };

                                                let payload =
                                                    crate::shield::HelheimShield::encrypt_packet(
                                                        ack,
                                                    );
                                                let _ = socket.write_all(payload.as_bytes()).await;
                                            }
                                            Err(e) => println!(
                                                "[HSP]: ⚠️  Ongeldig pakket geweigerd. Error: {}",
                                                e
                                            ),
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Stream Error: {}", e);
                                        return;
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => eprintln!("Accept Error: {}", e),
                }
            }
        });
        Ok(())
    }

    /// Telepathische dispatch naar een andere node (SECURED + PERSISTENT)
    pub async fn dispatch(target_ip: &str, port: u16, command: &str) -> Result<String> {
        let addr = format!("{}:{}", target_ip, port);

        // 1. [HSP] Encrypt Payload (Chaos-XOR)
        let protected = crate::shield::HelheimShield::encrypt_packet(command);

        // Setup Persistent Socket
        let mut stream = match CONNECTIONS.remove(&addr) {
            Some((_, s)) => s, // Grab from pool
            None => {
                println!(
                    "{}",
                    format!(
                        "[SWARM]: 🆕 Nieuwe Persistente TCP Verbinding naar {}...",
                        addr
                    )
                    .cyan()
                );
                TcpStream::connect(&addr)
                    .await
                    .map_err(|e| anyhow::anyhow!("Connection Failed: {}", e))?
            }
        };

        // Network I/O
        match stream.write_all(protected.as_bytes()).await {
            Ok(_) => {}
            Err(_) => {
                // Pool socket died, reconnect once
                println!(
                    "{}",
                    format!("[SWARM]: ⚠️ Socket timeout op {}. Herstellen...", addr).yellow()
                );
                stream = TcpStream::connect(&addr)
                    .await
                    .map_err(|e| anyhow::anyhow!("Reconnect Failed: {}", e))?;
                stream.write_all(protected.as_bytes()).await?;
            }
        }

        // Await ACK/Response
        let mut buf = vec![0; 1024 * 1024]; // 1MB buffer
        let n = stream.read(&mut buf).await?;
        let raw_response = String::from_utf8_lossy(&buf[..n]);

        // Save back to Pool
        CONNECTIONS.insert(addr, stream);

        // [HSP] Decrypt Response
        let decrypted_response = crate::shield::HelheimShield::decrypt_packet(&raw_response)?;

        Ok(decrypted_response)
    }
}
