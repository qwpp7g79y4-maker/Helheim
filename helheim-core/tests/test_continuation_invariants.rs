use helheim_core::orchestra::continuation::capture_continuation;
use helheim_core::orchestra::memory::{MemoryManager, HelheimType};
use helheim_core::orchestra::distributed::DistributedMemory;
use helheim_core::orchestra::tcp_resources::RESOURCE_TABLE;
use helheim_lang::ast::{CodeTaal, LiteralValue};
use std::sync::Arc;
use base64::Engine;

#[tokio::test]
async fn test_continuation_invariants() {
    let memory = Arc::new(MemoryManager::new());
    let distributed = DistributedMemory::new("test-node".to_string());
    
    // Set some memory state
    memory.set_var_native("foo".to_string(), HelheimType::Int(42));
    
    // Set up a mock statement
    let stmt = CodeTaal::VarDef {
        name: "bar".to_string(),
        value: Box::new(CodeTaal::Literal(LiteralValue::String("hello".to_string()))),
    };
    
    // Capture continuation
    let cont = capture_continuation(&stmt, &memory, "Migratie", &distributed, false).expect("capture failed");
    
    // 1. Stack JSON invariant
    assert!(!cont.captured_stack_json.is_empty(), "Stack JSON mag niet leeg zijn");
    let stack: Vec<CodeTaal> = serde_json::from_str(&cont.captured_stack_json).expect("Stack JSON deserialisatie faalde");
    assert_eq!(stack.len(), 1, "Verwachte 1 statement in de stack");
    match &stack[0] {
        CodeTaal::VarDef { name, .. } => assert_eq!(name, "bar"),
        _ => panic!("Verkeerde statement type in stack"),
    }
    
    // 2. Memory snapshot invariant
    assert!(cont.captured_memory.globals.contains_key("foo"), "Memory snapshot mist 'foo'");
    match cont.captured_memory.globals.get("foo").unwrap() {
        HelheimType::Int(val) => assert_eq!(*val, 42),
        _ => panic!("Verkeerde variable type in snapshot"),
    }
    
    // 3. Signing invariant
    assert!(cont.signature.is_some(), "Handtekening ontbreekt");
    let sig_str = cont.signature.as_ref().unwrap();
    let sig_bytes = base64::engine::general_purpose::STANDARD.decode(sig_str).expect("Base64 decode faalde");
    
    // Verwijder signature voor verificatie (het originele payload werd gesigned zonder)
    let mut cont_for_verification = cont.clone();
    cont_for_verification.signature = None;
    let payload_json = serde_json::to_string(&cont_for_verification).unwrap();
    
    // Verifieer handtekening
    let pub_key = cont.source_pubkey.as_ref().expect("Source pubkey ontbreekt");
    let verify_res = helheim_core::shield::crypto::SwarmSigner::verify_peer(pub_key, payload_json.as_bytes(), &sig_bytes);
    assert!(verify_res.is_ok(), "Handtekening verificatie faalde");
    
    use helheim_core::orchestra::tcp_resources::Resource;
    // Open een fake handle in de RESOURCE_TABLE
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    RESOURCE_TABLE.insert(9999, Resource::TcpListener(std::sync::Arc::new(tokio::sync::Mutex::new(listener))));
    
    // Capture should now fail
    let fail_cont = capture_continuation(&stmt, &memory, "Migratie", &distributed, false);
    assert!(fail_cont.is_err(), "Continuation had moeten falen wegens open resource handles");
    assert!(fail_cont.unwrap_err().to_string().contains("open handle(s) actief"), "Verkeerde error melding");
    
    // Cleanup
    RESOURCE_TABLE.remove(&9999);
}
