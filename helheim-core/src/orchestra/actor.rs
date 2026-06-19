//! Actor / Message-Passing System for Helheim (Vraag 2)
//! Zero-overhead local messaging via Tokio MPSC.
//! Remote via existing distributed Swarm (ast_json serialization of HelValue).
//! Each actor ("Ziel") runs fully isolated in its own tokio task with private MemoryManager scope.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use tokio::sync::mpsc;
use anyhow::Result;

use helheim_lang::ast::CodeTaal;
use crate::orchestra::memory::{MemoryManager, HelheimType, ScopeGuard};
use crate::orchestra::executor::Executor; // for context
use crate::common::context::ExecutionContext;
use base64::Engine;

/// Unique identifier for an actor.
pub type ActorId = u64;

use crate::orchestra::continuation::{SerializableContinuation, capture_continuation};

// [W·AG·AF] C1 Review: Actor message passing remote detection and scope safety
pub async fn resume_from_serialized(
    executor: &Executor,
    cont: SerializableContinuation,
    resume_value: HelheimType,
    memory: Arc<MemoryManager>,
    rx: &mut mpsc::Receiver<HelValueWrapper>,
) -> Result<Option<String>> {
    memory.restore_snapshot(&cont.captured_memory);
    memory.set_var_native("resume_value".to_string(), resume_value);

    let remaining: Vec<CodeTaal> = serde_json::from_str(&cont.captured_stack_json).unwrap_or_default();
    evaluate_remaining_stack(executor, &remaining, memory.clone(), ExecutionContext::default_privileged(), rx).await
}

pub async fn evaluate_remaining_stack<'a>(
    executor: &'a Executor,
    statements: &'a [CodeTaal],
    memory: Arc<MemoryManager>,
    ctx: ExecutionContext,
    rx: &'a mut mpsc::Receiver<HelValueWrapper>,
) -> Result<Option<String>> {
    let mut last = None;
    for stmt in statements {
        if let Some(res) = evaluate_actor_statement(executor, stmt, memory.clone(), ctx.clone(), rx).await? {
            last = Some(res);
        }
    }
    Ok(last)
}

/// Strategy for handling actor failures (Supervisor Hierarchy)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupervisionStrategy {
    /// Simply log the failure and terminate (default)
    Stop,
    /// Restart the actor from its initial state (requires capturing initial AST)
    Restart,
    /// Escalate the failure to the parent supervisor/actor
    Escalate,
}

#[derive(Clone, Debug)]
pub struct ActorMetadata {
    pub parent_id: Option<ActorId>,
    pub strategy: SupervisionStrategy,
}

/// Handle returned when spawning an actor.
#[derive(Clone)]
pub struct ActorHandle {
    pub id: ActorId,
    pub name: Option<String>,
    pub metadata: ActorMetadata,
}

/// Wrapper so we can send either local HelheimType (zero-copy) or serialized for remote.
#[derive(Clone, Debug)]
pub enum HelValueWrapper {
    /// Fast local path - direct value (no serialization).
    Local(HelheimType),
    /// For remote actors over Swarm (serialized as before).
    Remote(String), // JSON of the value
}

impl From<HelheimType> for HelValueWrapper {
    fn from(v: HelheimType) -> Self {
        HelValueWrapper::Local(v)
    }
}

/// Central registry for all live actors on this node.
/// Lock-free reads via DashMap. Used by both local send and remote routing.
#[derive(Clone)]
pub struct ActorRegistry {
    next_id: Arc<AtomicU64>,
    /// id -> sender for local delivery
    actors: Arc<DashMap<ActorId, mpsc::Sender<HelValueWrapper>>>,
    /// optional name -> id for ergonomic addressing
    name_map: Arc<DashMap<String, ActorId>>,
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            next_id: Arc::new(AtomicU64::new(1)),
            actors: Arc::new(DashMap::new()),
            name_map: Arc::new(DashMap::new()),
        }
    }

    /// Spawn a new actor entry in the registry.
    /// Returns the handle and the receiver that the actor task should own.
    pub fn register(&self, name: Option<String>, metadata: ActorMetadata) -> (ActorHandle, mpsc::Receiver<HelValueWrapper>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(1024); // bounded for back-pressure / isolation

        self.actors.insert(id, tx.clone());

        if let Some(ref n) = name {
            self.name_map.insert(n.clone(), id);
        }

        let handle = ActorHandle {
            id,
            name: name.clone(),
            metadata,
        };

        (handle, rx)
    }

    /// Resolve a target expression result to a sender.
    /// Supports numeric ID (as string or int) or registered name.
    pub fn resolve_sender(&self, target: &str) -> Option<mpsc::Sender<HelValueWrapper>> {
        // Try as ID
        if let Ok(id) = target.parse::<u64>() {
            if let Some(entry) = self.actors.get(&id) {
                return Some(entry.value().clone());
            }
        }

        // Try as name
        if let Some(entry) = self.name_map.get(target) {
            let id = *entry.value();
            if let Some(sender_entry) = self.actors.get(&id) {
                return Some(sender_entry.value().clone());
            }
        }

        None
    }

    /// Remove actor (called on actor task exit for cleanup).
    pub fn unregister(&self, id: ActorId) {
        self.actors.remove(&id);
        // Note: name_map cleanup can be lazy or explicit on drop
        self.name_map.retain(|_, v| *v != id);
    }

    /// For remote routing: check if this is a local actor.
    pub fn is_local(&self, target: &str) -> bool {
        self.resolve_sender(target).is_some()
    }
}

/// Lightweight runner for a single actor.
/// This runs in its own tokio::spawn task.
/// The actor's main body (passed at spawn) is executed once.
/// Any CodeTaal::Receive nodes inside will suspend this task by awaiting on the mailbox.
pub async fn run_actor(
    id: ActorId,
    mut rx: mpsc::Receiver<HelValueWrapper>,
    body: CodeTaal,
    registry: Arc<ActorRegistry>,
    base_memory: Arc<MemoryManager>,
    base_ctx: ExecutionContext,
    executor: Arc<crate::orchestra::executor::Executor>,
    metadata: ActorMetadata,
) {
    loop {
        // Each actor gets a completely isolated MemoryManager (daemon style)
        let actor_memory = base_memory.spawn_daemon_memory();

        // Convenience variables available inside the actor
        actor_memory.set_var_native("self".to_string(), HelheimType::Int(id as i64));

        let local_ctx = base_ctx.clone();

        // Execute the actor's main body using the actor-specific evaluator.
        // This is where Receive nodes will actually await on the mailbox.
        if let Err(e) = evaluate_actor_body(executor.as_ref(), &body, actor_memory.clone(), local_ctx, &mut rx).await {
            tracing::error!("[ACTOR {}] Actor body error: {}", id, e);
            
            match metadata.strategy {
                SupervisionStrategy::Stop => {
                    tracing::debug!("[ACTOR {}] Stopping due to failure.", id);
                    break;
                }
                SupervisionStrategy::Restart => {
                    tracing::warn!("[ACTOR {}] Restarting due to failure according to Supervisor strategy.", id);
                    continue;
                }
                // Fully implemented per G1 sprint 2
                SupervisionStrategy::Escalate => {
                    tracing::error!("[ACTOR {}] Escalating failure to parent.", id);
                    if let Some(parent) = metadata.parent_id {
                        let err_msg = crate::orchestra::memory::HelheimType::String(format!("ESCALATION_ERROR van {}: {}", id, e));
                        let _ = send_message(&registry, &executor.distributed, &parent.to_string(), err_msg, false).await;
                    }
                    break;
                }
            }
        } else {
            // Completed without error
            break;
        }
    }

    registry.unregister(id);
    tracing::debug!("[ACTOR {}] Actor terminated", id);
}

/// Helper to send a message (local fast path or remote).
pub async fn send_message(
    registry: &ActorRegistry,
    _distributed: &crate::orchestra::distributed::DistributedMemory,
    target: &str,
    message: HelheimType,
    is_remote: bool,
) -> Result<()> {
    if let Some(sender) = registry.resolve_sender(target) {
        // Zero-overhead local path
        let _ = sender.send(message.into()).await;
        Ok(())
    } else if is_remote {
        // Remote: serialize and use existing Swarm/distributed machinery
        // Target format: id_or_name@ip:port (e.g. "my_actor@127.0.0.1:8080")
        if let Some((id_or_name, host_port)) = target.split_once('@') {
            if let Some((ip, port_str)) = host_port.split_once(':') {
                if let Ok(port) = port_str.parse::<u16>() {
                    let json = match &message {
                        HelheimType::String(s) => format!("\"{}\"", s.replace("\"", "\\\"")),
                        HelheimType::Bytes(b) => {
                            let hex: Vec<String> = b.iter().map(|byte| format!("{:02x}", byte)).collect();
                            format!("b[{}]", hex.join(" "))
                        }
                        other => other.to_string(),
                    };
                    let payload = format!("stuur_bericht \"{}\" {}", id_or_name, json);
                    match crate::network::hsp_node::SwarmEngine::dispatch(ip, port, &payload).await {
                        Ok(_) => return Ok(()),
                        Err(e) => anyhow::bail!("Remote send mislukt: {}", e),
                    }
                }
            }
        }
        anyhow::bail!("Invalid remote target format. Expected 'id_or_name@ip:port', got '{}'", target)
    } else {
        anyhow::bail!("Actor '{}' not found (local or remote)", target)
    }
}

/// Custom async evaluator for actor bodies.
/// This walks the CodeTaal AST and can suspend on Receive nodes by awaiting the mailbox.
/// It is called from the actor's dedicated task (run_actor).
/// Most non-suspending logic delegates to the main Executor to avoid duplication.
use std::future::Future;
use std::pin::Pin;

pub fn evaluate_actor_body<'a>(
    executor: &'a crate::orchestra::executor::Executor,
    body: &'a CodeTaal,
    memory: Arc<MemoryManager>,
    ctx: ExecutionContext,
    rx: &'a mut mpsc::Receiver<HelValueWrapper>,
) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + 'a>> {
    Box::pin(async move {
    match body {
        CodeTaal::Block { statements } => {
            let mut last_result = None;
            for stmt in statements {
                if let Some(res) = evaluate_actor_statement(executor, stmt, memory.clone(), ctx.clone(), rx).await? {
                    last_result = Some(res);
                }
            }
            Ok(last_result)
        }
        other => evaluate_actor_statement(executor, other, memory.clone(), ctx, rx).await,
    }
    })
}

/// Evaluates a single statement in the context of an actor task.
/// This is where suspension happens for Receive.
fn evaluate_actor_statement<'a>(
    executor_ref: &'a crate::orchestra::executor::Executor,
    stmt: &'a CodeTaal,
    memory: Arc<MemoryManager>,
    ctx: ExecutionContext,
    rx: &'a mut mpsc::Receiver<HelValueWrapper>,
) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + 'a>> {
    Box::pin(async move {
        let mut local_executor = (*executor_ref).clone();
        local_executor.memory = memory.clone();
        let executor = &local_executor;
        
    match stmt {
        // === THE SUSPENSION POINT ===
        CodeTaal::Receive { var, timeout, body } => {
            // Await the per-actor mailbox. This pauses ONLY this actor's task.
            // Other actors and the main executor continue running.
            let received_wrapper = if let Some(timeout_ast) = timeout {
                // Evaluate timeout expression (e.g. number of ms)
                let timeout_str = executor.evaluate_ast_expr(timeout_ast, ctx.clone()).await?;
                let millis = timeout_str.parse::<u64>().unwrap_or(0);
                let duration = std::time::Duration::from_millis(millis);

                match tokio::time::timeout(duration, rx.recv()).await {
                    Ok(Some(wrapper)) => wrapper,
                    _ => HelValueWrapper::Local(HelheimType::Null), // timeout -> null
                }
            } else {
                // No timeout: block until message
                rx.recv()
                    .await
                    .unwrap_or(HelValueWrapper::Local(HelheimType::Null))
            };

            // Convert to HelheimType (zero-copy for local messages)
            let received_value = match received_wrapper {
                HelValueWrapper::Local(v) => v,
                HelValueWrapper::Remote(json) => HelheimType::parse(&json),
            };

            // Bind the received value to the requested variable in a fresh local scope
            let _guard = ScopeGuard::new(memory.as_ref());
            memory.set_var_native(var.clone(), received_value);

            // Execute the body associated with this receive (can contain more code, loops, etc.)
            evaluate_actor_body(executor, body, memory.clone(), ctx.clone(), rx).await
        }

        // Handle blocks recursively so nested Receives work
        CodeTaal::Block { statements } => {
            let mut last = None;
            for i in 0..statements.len() {
                let s = &statements[i];
                let remaining = statements[i + 1..].to_vec();
                crate::orchestra::continuation::set_rest_ast(memory.as_ref(), &remaining);
                
                if let Some(r) = evaluate_actor_statement(executor, s, memory.clone(), ctx.clone(), rx).await? {
                    last = Some(r);
                }
            }
            Ok(last)
        }

        // Basic control flow to support Receive inside loops/conditionals
        CodeTaal::Loop { condition, body } => {
            let mut last = None;
            loop {
                let cond_val = executor.evaluate_ast_expr(condition, ctx.clone()).await?;
                if cond_val == "onwaar" || cond_val == "false" || cond_val == "0" {
                    break;
                }
                if let Some(res) = evaluate_actor_body(executor, body, memory.clone(), ctx.clone(), rx).await? {
                    last = Some(res);
                }
            }
            Ok(last)
        }

        CodeTaal::If { condition, then, else_block } => {
            let cond_val = executor.evaluate_ast_expr(condition, ctx.clone()).await?;
            let is_true = cond_val != "onwaar" && cond_val != "false" && cond_val != "0" && !cond_val.is_empty();
            if is_true {
                evaluate_actor_body(executor, then, memory.clone(), ctx, rx).await
            } else if let Some(else_b) = else_block {
                evaluate_actor_body(executor, else_b, memory.clone(), ctx, rx).await
            } else {
                Ok(None)
            }
        }

        // === ALGEBRAIC EFFECTS (Vraag 6) ===
        CodeTaal::EffectDef { .. } => {
            // Definitie wordt al door semantic analyzer opgepakt.
            Ok(None)
        }
        CodeTaal::LinearResource { .. } => {
            // Lineaire resource wordt in type-checker gevalideerd.
            Ok(None)
        }
        CodeTaal::Handle { effect, handlers, body } => {
            // Push handler in scope
            let _guard = ScopeGuard::new(memory.as_ref());
            for (op_name, h_body) in handlers {
                let handler_var = format!("__effect_handler_{}_{}", effect, op_name);
                let ast_str = serde_json::to_string(h_body).unwrap_or_default();
                memory.set_var_native(handler_var, HelheimType::String(ast_str));
            }
            evaluate_actor_body(executor, body, memory.clone(), ctx, rx).await
        }
        CodeTaal::Perform { effect, operation, args } => {
            let handler_var = format!("__effect_handler_{}_{}", effect, operation);
            if let Some(HelheimType::String(ast_str)) = memory.get_var_native(&handler_var) {
                if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                    // Create true continuation capture
                    let continuation = capture_continuation(stmt, memory.as_ref(), effect, &executor.distributed)?;
                    
                    // Bind continuation to scope as Base64 to prevent injection
                    let _guard = ScopeGuard::new(memory.as_ref());
                    let json_str = serde_json::to_string(&continuation).unwrap_or_default();
                    let b64_str = base64::engine::general_purpose::STANDARD.encode(json_str);
                    memory.set_var_native("resume_k".to_string(), HelheimType::String(b64_str));
                    return evaluate_actor_body(executor, &handler_ast, memory.clone(), ctx, rx).await;
                } else {
                    anyhow::bail!("Invalid handler AST")
                }
            } else {
                // No handler -> direct native dispatch (zero-overhead path)
                match (effect.as_str(), operation.as_str()) {
                    ("Tcp", _) => {
                        // TCP fallback via executor logic
                        let res = executor.evaluate_tcp_primitive(operation, stmt, memory.as_ref(), ctx.clone()).await?;
                        Ok(Some(res))
                    }
                    ("Actor", "send") => {
                        if args.len() == 2 {
                            let temp_stmt = CodeTaal::SendMessage {
                                target: Box::new(args[0].clone()),
                                message: Box::new(args[1].clone()),
                            };
                            return Box::pin(executor.execute_ast(vec![temp_stmt], ctx.clone())).await;
                        }
                        Ok(None)
                    }
                    ("Actor", "spawn") => {
                        let temp_stmt = CodeTaal::Perform {
                            effect: "Actor".to_string(),
                            operation: "spawn".to_string(),
                            args: args.clone(),
                        };
                        return Box::pin(executor.execute_ast(vec![temp_stmt], ctx.clone())).await;
                    }
                    ("Trace", "record") => {
                        if args.len() == 3 {
                            if let (Ok(k_str), Ok(n_str), Ok(p_str)) = (
                                executor.evaluate_ast_expr(&args[0], ctx.clone()).await,
                                executor.evaluate_ast_expr(&args[1], ctx.clone()).await,
                                executor.evaluate_ast_expr(&args[2], ctx.clone()).await
                            ) {
                                if let (Ok(kind), Ok(node), Ok(payload)) = (k_str.parse::<u8>(), n_str.parse::<u64>(), p_str.parse::<u64>()) {
                                    let trace_kind = unsafe { std::mem::transmute::<u8, crate::orchestra::flight_recorder::TraceKind>(kind.min(11)) }; // safe fallback
                                    crate::orchestra::flight_recorder::record(trace_kind, node, payload);
                                }
                            }
                        }
                        Ok(None)
                    }
                    ("Asm", "inline") => {
                        if args.len() >= 2 {
                            let target = executor.evaluate_ast_expr(&args[0], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let code = executor.evaluate_ast_expr(&args[1], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let temp_stmt = CodeTaal::InlineAssembly {
                                target,
                                code,
                                inputs: vec![],
                                outputs: vec![],
                                clobbers: vec![],
                                fallback: None,
                            };
                            return Box::pin(executor.execute_ast(vec![temp_stmt], ctx.clone())).await;
                        }
                        Err(anyhow::anyhow!("Asm.inline vereist target en code"))
                    }
                    ("Ffi", "call") => {
                        if args.len() >= 2 {
                            let path = executor.evaluate_ast_expr(&args[0], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let func = executor.evaluate_ast_expr(&args[1], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let mut name = path;
                            name.push_str("::");
                            name.push_str(&func);
                            let temp_stmt = CodeTaal::FunctionCall {
                                name,
                                args: args[2..].to_vec(),
                            };
                            return Box::pin(executor.execute_ast(vec![temp_stmt], ctx.clone())).await;
                        }
                        Err(anyhow::anyhow!("Ffi.call vereist path en func"))
                    }
                    ("Swarm", "dispatch") => {
                        if args.len() >= 3 {
                            let ip = executor.evaluate_ast_expr(&args[0], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let port_str = executor.evaluate_ast_expr(&args[1], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let port: u16 = port_str.parse().unwrap_or(8080);
                            
                            // Als we een ruwe object-string binnenkrijgen, pak de pure string, anders evalueer de code
                            let payload = executor.evaluate_ast_expr(&args[2], ctx.clone()).await.unwrap_or_default();
                            
                            match crate::network::hsp_node::SwarmEngine::dispatch(&ip, port, &payload).await {
                                Ok(response) => return Ok(Some(response)),
                                Err(e) => return Err(anyhow::anyhow!("Swarm dispatch gefaald: {}", e)),
                            }
                        }
                        Err(anyhow::anyhow!("Swarm.dispatch vereist ip, port, payload"))
                    }
                    ("Swarm", "migrate") => {
                        if args.len() >= 2 {
                            let ip = executor.evaluate_ast_expr(&args[0], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let port_str = executor.evaluate_ast_expr(&args[1], ctx.clone()).await.unwrap_or_default().trim_matches('"').to_string();
                            let port: u16 = port_str.parse().unwrap_or(8080);
                            
                            let continuation = crate::orchestra::continuation::capture_continuation(stmt, memory.as_ref(), effect, &executor.distributed)?;
                            let wrapper = serde_json::json!({
                                "type": "TeleportContinuation",
                                "continuation": continuation
                            });
                            
                            match crate::network::hsp_node::SwarmEngine::dispatch(&ip, port, &wrapper.to_string()).await {
                                Ok(_) => return Ok(None), // Teleport success, abort local execution
                                Err(e) => return Err(anyhow::anyhow!("Teleport failed: {}", e)),
                            }
                        }
                        Err(anyhow::anyhow!("Swarm.migrate vereist target_ip, target_port"))
                    }
                    _ => Err(anyhow::anyhow!("Unknown effect operation: {}.{}", effect, operation)),
                }
            }
        }
        CodeTaal::Resume { continuation, value } => {
            // Echte resumption van een continuation (vanuit Base64)
            let cont_b64 = executor.evaluate_ast_expr(continuation, ctx.clone()).await?;
            let cont_bytes = base64::engine::general_purpose::STANDARD.decode(cont_b64.trim_matches('"'))
                .map_err(|e| anyhow::anyhow!("Invalid Base64 continuation: {}", e))?;
            let cont: SerializableContinuation = serde_json::from_slice(&cont_bytes)?;
            
            let resume_val_str = executor.evaluate_ast_expr(value, ctx.clone()).await?;
            let resume_val = HelheimType::parse(&resume_val_str);
            
            return resume_from_serialized(executor, cont, resume_val, memory.clone(), rx).await;
        }

        // For everything else (VarDef, Print, SendMessage inside actor, FunctionCall, etc.)
        // delegate to the main (non-suspending) executor logic.
        // This re-uses all existing evaluation, FFI, TCP, etc.
        // If a sub-body is passed (e.g. in FunctionDef), it will go through normal path
        // (Receives in called functions are not actor-suspended in this simple model;
        //  for full power, functions called from actors can also use the actor evaluator).
        other => {
            // Execute as a single-statement "program" using an executor clone with actor's memory
            executor.execute_ast(vec![other.clone()], ctx).await
        }
    }
    })
}