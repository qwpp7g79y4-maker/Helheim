use anyhow::Result;
use colored::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use dashmap::DashMap;

pub struct SwarmEngine;

lazy_static::lazy_static! {
    static ref CONNECTIONS: DashMap<String, (tokio::net::TcpStream, [u8; 32])> = DashMap::new();
}

impl SwarmEngine {
    /// Start de Async Swarm Listener (TCP)
    /// Non-blocking, draait op de achtergrond.
    pub async fn ignite(
        port: u16,
        orchestrator: std::sync::Arc<crate::orchestra::Orchestrator>,
    ) -> Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        tracing::debug!(
            "{}",
            format!("[SWARM]: HSP Node Active on port {}", port)
                .green()
                .bold()
        );

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut socket, addr)) => {
                        tracing::debug!("⚡ Persistente Swarm Verbinding geaccepteerd: {}", addr);
                        let orchestrator_clone = orchestrator.clone();
                        tokio::spawn(async move {
                            // --- ECDH HANDSHAKE (Server-side) ---
                            let mut ecdh = crate::shield::EcdhSession::new();
                            let mut peer_pub = [0u8; 32];
                            if socket.read_exact(&mut peer_pub).await.is_err() {
                                tracing::debug!("❌ Swarm ECDH gefaald: kon public key niet lezen van {}", addr);
                                return;
                            }
                            if socket.write_all(&ecdh.public_key).await.is_err() {
                                return;
                            }
                            let session_key = match ecdh.derive_shared_key(&peer_pub) {
                                Ok(k) => k,
                                Err(e) => {
                                    tracing::debug!("❌ Swarm ECDH gefaald (Sleutel afleiding): {}", e);
                                    return;
                                }
                            };
                            tracing::debug!("[SWARM-ECDH]: 🔐 Sessiesleutel succesvol overeengekomen met {}", addr);

                            let mut buf = vec![0; 1024 * 1024]; // 1MB buffer

                            loop {
                                // 1. Read Payload
                                match socket.read(&mut buf).await {
                                    Ok(n) if n == 0 => {
                                        tracing::debug!("💤 Swarm Node losgekoppeld: {}", addr);
                                        return; // Connection gracefully closed
                                    }
                                    Ok(n) => {
                                        let raw_payload = String::from_utf8_lossy(&buf[..n]);

                                        // [HSP] Decryptie Layer met Session Key
                                        match crate::shield::HelheimShield::decrypt_packet_with_key(
                                            &raw_payload,
                                            &session_key
                                        ) {
                                            Ok(decrypted) => {
                                                tracing::debug!(
                                                    "[HSP]: 🔓 Payload ontsleuteld via Ephemeral Key ({} bytes)",
                                                    decrypted.len()
                                                );

                                                let ctx = crate::common::context::ExecutionContext::sandbox();
                                                let mut execution_script = decrypted.as_str();

                                                // C-3: Geen remote privilege escalation meer toegestaan, zelfs niet met handtekening (voorkomt replay attacks).
                                                // Alle inkomende netwerk-executies zijn strict Sandbox mode.
                                                if decrypted.starts_with("SIGNED: ") {
                                                    tracing::warn!("SIGNED request genegeerd. Remote execution is altijd Sandbox.");
                                                    if let Some((_, script_part)) = decrypted[8..].split_once(" | ") {
                                                        execution_script = script_part;
                                                    }
                                                } else {
                                                    tracing::debug!("[SWARM]: 🛡️ Executie in Sandbox Mode.");
                                                }

                                                // Intercept TeleportContinuation VOOR process_command
                                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&decrypted) {
                                                    if json.get("type").and_then(|v| v.as_str()) == Some("TeleportContinuation") {
                                                        if let Some(cont_json) = json.get("continuation") {
                                                            if let Ok(mut cont) = serde_json::from_value::<crate::orchestra::continuation::SerializableContinuation>(cont_json.clone()) {
                                                                // Verifieer SwarmSigner signature (replay protection)
                                                                let mut valid_signature = false;
                                                                if let Some(sig_str) = cont.signature.take() {
                                                                    use base64::Engine;
                                                                    if let Ok(sig_bytes) = base64::engine::general_purpose::STANDARD.decode(&sig_str) {
                                                                        if let Ok(json_without_sig) = serde_json::to_string(&cont) {
                                                                            if let Some(pub_key) = &cont.source_pubkey {
                                                                                if crate::shield::crypto::SwarmSigner::verify_peer(pub_key, json_without_sig.as_bytes(), &sig_bytes).is_ok() {
                                                                                    valid_signature = true;
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                
                                                                if valid_signature {
                                                                    tracing::debug!("[SWARM]: 🌟 Geldige TeleportContinuation ontvangen van {}. Hervatten...", cont.source_node);
                                                                    
                                                                    let isolated_memory = helheim_lang::memory::MemoryManager::spawn_isolated(&cont.captured_memory);
                                                                    // We voeren het uit met privileges omdat het afkomstig is van een geverifieerde, trusted node in het Swarm netwerk
                                                                    let resume_ctx = crate::common::context::ExecutionContext::default_privileged();
                                                                    let isolated_executor = crate::orchestra::executor::Executor::new(
                                                                        isolated_memory,
                                                                        orchestrator_clone.discovery.clone(),
                                                                        orchestrator_clone.distributed.clone(),
                                                                    );
                                                                    use base64::Engine;
                                                                    let b64_cont = base64::engine::general_purpose::STANDARD.encode(serde_json::to_string(&cont).unwrap_or_default());
                                                                    let resume_stmt = helheim_lang::ast::CodeTaal::Resume {
                                                                        continuation: Box::new(helheim_lang::ast::CodeTaal::Literal(helheim_lang::ast::LiteralValue::String(format!("\"{}\"", b64_cont)))),
                                                                        value: Box::new(helheim_lang::ast::CodeTaal::Literal(helheim_lang::ast::LiteralValue::String(String::new()))),
                                                                    };
                                                                    // Run completely isolated, concurrent from main host
                                                                    let _ = Box::pin(isolated_executor.execute_ast(vec![resume_stmt], resume_ctx)).await;
                                                                    
                                                                    let payload = crate::shield::HelheimShield::encrypt_packet_with_key("SWARM_ACK_TELEPORT", &session_key);
                                                                    let _ = socket.write_all(payload.as_bytes()).await;
                                                                    continue;
                                                                } else {
                                                                    tracing::error!("TeleportContinuation ongeldige handtekening!");
                                                                    let payload = crate::shield::HelheimShield::encrypt_packet_with_key("SWARM_ERR_SIG", &session_key);
                                                                    let _ = socket.write_all(payload.as_bytes()).await;
                                                                    continue;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                // Actively process the command via the Orchestrator
                                                let exec_result = orchestrator_clone
                                                    .process_command(execution_script, ctx)
                                                    .await;

                                                let (ack, _secure_resp) = match exec_result {
                                                    Ok(_) => ("SWARM_ACK_SUCCESS", "Success"),
                                                    Err(e) => {
                                                        tracing::error!("Remote execution gefaald: {}",
                                                            e
                                                        );
                                                        ("SWARM_ACK_ERROR", "Error")
                                                    }
                                                };

                                                let payload =
                                                    crate::shield::HelheimShield::encrypt_packet_with_key(
                                                        ack,
                                                        &session_key
                                                    );
                                                let _ = socket.write_all(payload.as_bytes()).await;
                                            }
                                            Err(e) => tracing::debug!(
                                                "[HSP]: ⚠️  Ongeldig pakket geweigerd. Error: {}",
                                                e
                                            ),
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("Stream Error: {}", e);
                                        return;
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => tracing::error!("Accept Error: {}", e),
                }
            }
        });
        Ok(())
    }

    /// Telepathische dispatch naar een andere node (SECURED + PERSISTENT)
    pub async fn dispatch(target_ip: &str, port: u16, command: &str) -> Result<String> {
        let addr = format!("{}:{}", target_ip, port);

        // Helper voor connectie + ECDH Handshake (Client)
        async fn connect_and_handshake(addr: &str) -> Result<(TcpStream, [u8; 32])> {
            let mut stream = TcpStream::connect(addr).await
                .map_err(|e| anyhow::anyhow!("Connection Failed: {}", e))?;
                
            let mut ecdh = crate::shield::EcdhSession::new();
            stream.write_all(&ecdh.public_key).await?;
            
            let mut peer_pub = [0u8; 32];
            stream.read_exact(&mut peer_pub).await?;
            
            let session_key = ecdh.derive_shared_key(&peer_pub)?;
            Ok((stream, session_key))
        }

        // Setup Persistent Socket
        let (mut stream, session_key) = match CONNECTIONS.remove(&addr) {
            Some((_, s)) => s, // Grab from pool
            None => {
                tracing::debug!("{}", format!("[SWARM]: 🆕 Nieuwe Persistente TCP Verbinding naar {}...", addr).cyan());
                connect_and_handshake(&addr).await?
            }
        };

        // 1. [HSP] Encrypt Payload met Ephemeral Key
        let protected = crate::shield::HelheimShield::encrypt_packet_with_key(command, &session_key);

        // Network I/O
        let (mut final_stream, final_key) = match stream.write_all(protected.as_bytes()).await {
            Ok(_) => (stream, session_key),
            Err(_) => {
                // Pool socket died, reconnect once
                tracing::debug!("{}", format!("[SWARM]: ⚠️ Socket timeout op {}. Herstellen en nieuwe ECDH...", addr).yellow());
                let (mut new_stream, new_key) = connect_and_handshake(&addr).await?;
                let new_protected = crate::shield::HelheimShield::encrypt_packet_with_key(command, &new_key);
                new_stream.write_all(new_protected.as_bytes()).await?;
                (new_stream, new_key)
            }
        };

        // Await ACK/Response
        let mut buf = vec![0; 1024 * 1024]; // 1MB buffer
        let n = final_stream.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            return Err(anyhow::anyhow!("Connection closed by peer during dispatch"));
        }
        let raw_response = String::from_utf8_lossy(&buf[..n]);

        // Save back to Pool only if it's healthy
        CONNECTIONS.insert(addr, (final_stream, final_key));

        // [HSP] Decrypt Response
        let decrypted_response = crate::shield::HelheimShield::decrypt_packet_with_key(&raw_response, &final_key)?;

        Ok(decrypted_response)
    }
}
