use helheim_core::orchestra::continuation::SerializableContinuation;
use helheim_core::shield::crypto::SwarmSigner;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_hsp_security_signature_and_lamport() {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Start dummy node luisteraar
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    
    let discovery = std::sync::Arc::new(helheim_core::network::DiscoveryService::new());
    let orchestrator = std::sync::Arc::new(helheim_core::orchestra::Orchestrator::new(discovery));
    
    // Start SwarmEngine listener
    let orchestrator_clone = orchestrator.clone();
    tokio::spawn(async move {
        let _ = helheim_core::network::hsp_node::SwarmEngine::ignite(port, orchestrator_clone).await;
    });
    
    // Geef luisteraar even tijd om op te starten
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Test 1: Continuation zonder geldige signature (verkeerde lamport clock)
    // Actually we will dispatch via SwarmEngine. It sets signature automatically.
    // Wait, SwarmEngine::dispatch handles encryption and signature.
    // Let's create an artificial bad payload and connect directly.

    let target = format!("127.0.0.1:{}", port);
    
    // Echte handshake simuleren
    let mut stream = tokio::net::TcpStream::connect(&target).await.unwrap();
    
    let mut ecdh = helheim_core::shield::EcdhSession::new();
    stream.write_all(&ecdh.public_key).await.unwrap();
    let mut peer_pub = [0u8; 32];
    stream.read_exact(&mut peer_pub).await.unwrap();
    let session_key = ecdh.derive_shared_key(&peer_pub).unwrap();
    
    let mut cont = SerializableContinuation {
        id: 999,
        captured_memory: helheim_core::orchestra::memory::MemoryManager::new().take_snapshot(),
        captured_stack_json: "[]".to_string(),
        effect: "Migratie".to_string(),
        resume_value_hint: None,
        source_node: "test_node".to_string(),
        source_pubkey: Some(SwarmSigner::public_key()),
        lamport: 10,
        resource_requirements: vec![],
        signature: None,
        is_privileged: false,
    };
    
    // Ongeldige signature
    cont.signature = Some("bad_signature_base64".to_string());
    
    let payload = serde_json::json!({
        "type": "TeleportContinuation",
        "continuation": cont
    }).to_string();
    
    let enc_payload = helheim_core::shield::HelheimShield::encrypt_packet_with_key(&payload, &session_key);
    stream.write_all(enc_payload.as_bytes()).await.unwrap();
    
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let reply = helheim_core::shield::HelheimShield::decrypt_packet_with_key(&String::from_utf8_lossy(&buf[..n]), &session_key).unwrap();
    assert_eq!(reply, "SWARM_ERR_SIG", "Ongeldige signature had afgewezen moeten worden");
    
    // Test 2: Geldige signature
    cont.lamport = 20; // Increase lamport to prevent replay of the bad sig message? Wait, bad sig was not accepted, so lamport was not incremented, but to be safe let's use 20.
    cont.signature = None;
    use base64::Engine;
    let json_bytes = serde_json::to_string(&cont).unwrap();
    let sig = SwarmSigner::sign(json_bytes.as_bytes());
    cont.signature = Some(base64::engine::general_purpose::STANDARD.encode(sig));
    
    let payload2 = serde_json::json!({
        "type": "TeleportContinuation",
        "continuation": cont
    }).to_string();
    
    let enc_payload2 = helheim_core::shield::HelheimShield::encrypt_packet_with_key(&payload2, &session_key);
    stream.write_all(enc_payload2.as_bytes()).await.unwrap();
    
    let n2 = stream.read(&mut buf).await.unwrap();
    let reply2 = helheim_core::shield::HelheimShield::decrypt_packet_with_key(&String::from_utf8_lossy(&buf[..n2]), &session_key).unwrap();
    assert_eq!(reply2, "SWARM_ACK_TELEPORT", "Geldige signature had geaccepteerd moeten worden");
    
    // Test 3: Replay attack (Lamport clock <= 10)
    let payload3 = enc_payload2.clone(); // Replay exact same message
    stream.write_all(payload3.as_bytes()).await.unwrap();
    
    let n3 = stream.read(&mut buf).await.unwrap();
    let reply3 = helheim_core::shield::HelheimShield::decrypt_packet_with_key(&String::from_utf8_lossy(&buf[..n3]), &session_key).unwrap();
    assert_eq!(reply3, "SWARM_ERR_REPLAY", "Replay attack had afgewezen moeten worden op lamport clock");
    
    // Netjes afsluiten
    helheim_core::network::hsp_node::SwarmEngine::clear_pool();
}
