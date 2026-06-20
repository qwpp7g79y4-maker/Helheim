use std::sync::atomic::{AtomicU64, Ordering};
use helheim_lang::ast::CodeTaal;
use crate::orchestra::memory::{MemoryManager, HelheimType};
use base64::Engine;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SerializableContinuation {
    pub id: u64,
    pub captured_memory: crate::orchestra::memory::MemorySnapshot,
    pub captured_stack_json: String, // GESERIALISEERDE RESTERENDE AST
    pub effect: String,
    pub resume_value_hint: Option<String>,
    pub source_node: String,
    pub source_pubkey: Option<Vec<u8>>,
    pub lamport: u64,
    pub resource_requirements: Vec<String>,
    pub signature: Option<String>,
    pub is_privileged: bool,
}

pub fn generate_continuation_id() -> u64 {
    static NEXT_CONT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_CONT_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn get_rest_ast_key() -> String {
    format!("__REST_AST_{:?}_{:?}", tokio::task::try_id(), std::thread::current().id())
}

pub fn set_rest_ast(memory: &MemoryManager, remaining: &[CodeTaal]) {
    let rest_key = get_rest_ast_key();
    let json = serde_json::to_string(remaining).unwrap_or_default();
    memory.set_var_native(rest_key, HelheimType::String(json));
}

pub fn capture_continuation(
    stmt: &CodeTaal,
    memory: &MemoryManager,
    effect: &str,
    distributed: &crate::orchestra::distributed::DistributedMemory,
    is_privileged: bool,
) -> anyhow::Result<SerializableContinuation> {
    // Weiger migrate als er open handles zijn
    let open_handles: Vec<u64> = crate::orchestra::tcp_resources::RESOURCE_TABLE
        .iter()
        .map(|e| *e.key())
        .collect();
    if !open_handles.is_empty() {
        return Err(anyhow::anyhow!(
            "migrate geblokkeerd: {} open handle(s) actief. Sluit ze eerst.", 
            open_handles.len()
        ));
    }
    let mut stack = vec![stmt.clone()];
    let rest_key = get_rest_ast_key();
    if let Some(HelheimType::String(rest_json)) = memory.get_var_native(&rest_key) {
        if let Ok(mut rest_ast) = serde_json::from_str::<Vec<CodeTaal>>(&rest_json) {
            stack.append(&mut rest_ast);
        }
    }

    let mut cont = SerializableContinuation {
        id: generate_continuation_id(),
        captured_memory: memory.take_snapshot(),
        captured_stack_json: serde_json::to_string(&stack).unwrap_or_default(),
        effect: effect.to_string(),
        resume_value_hint: None,
        source_node: distributed.node_id.clone(),
        source_pubkey: Some(crate::shield::crypto::SwarmSigner::public_key()),
        lamport: distributed.bump(),
        resource_requirements: vec![],
        signature: None,
        is_privileged,
    };
    
    // Cryptografische ondertekening (Swarm Node Security)
    if let Ok(json) = serde_json::to_string(&cont) {
        let sig_bytes = crate::shield::crypto::SwarmSigner::sign(json.as_bytes());
        cont.signature = Some(base64::engine::general_purpose::STANDARD.encode(sig_bytes));
    }
    
    Ok(cont)
}
