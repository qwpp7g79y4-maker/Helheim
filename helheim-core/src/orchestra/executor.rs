use colored::Colorize;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::orchestra::synthesis;
use helheim_lang::ast::CodeTaal;
use crate::orchestra::memory::{MemoryManager, HelheimType};
use crate::orchestra::system;

// use futures::future;  // was for distributed Concurrent, kept for now
use serde_json;
use base64::Engine;
use std::sync::atomic::{AtomicUsize, Ordering};

static SCHEDULER_INDEX: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct Executor {
    pub memory: Arc<MemoryManager>,
    pub discovery: Arc<crate::network::DiscoveryService>,
    pub distributed: Arc<crate::orchestra::distributed::DistributedMemory>,
    pub stdlib: Arc<crate::orchestra::stdlib_manager::StdLibManager>,
    /// ActorRegistry for zero-overhead local messaging + remote routing.
    /// Integrated for Vraag 2 Actor model.
    pub actor_registry: Arc<crate::orchestra::actor::ActorRegistry>,
    /// PackageManager for signed .so / .hel import (Vraag 4).
    /// Verifies via existing Shield/Crypto before handing to NativeModuleLoader.
    pub package_manager: Arc<crate::orchestra::package_manager::PackageManager>,
}

impl Executor {
    pub fn new(memory: Arc<MemoryManager>, discovery: Arc<crate::network::DiscoveryService>, distributed: Arc<crate::orchestra::distributed::DistributedMemory>) -> Self {
        let package_manager = Arc::new(crate::orchestra::package_manager::PackageManager::new(vec![
            std::path::PathBuf::from("stdlib/lib"),
            std::path::PathBuf::from("test_plugins"),
        ]));
        let stdlib = Arc::new(crate::orchestra::stdlib_manager::StdLibManager::new(package_manager.clone()));
        let actor_registry = Arc::new(crate::orchestra::actor::ActorRegistry::new());

        Self { memory, discovery, distributed, stdlib, actor_registry, package_manager }
    }

    /// Start the Flight Recorder background drainer (Vraag 3).
    /// Call this once after construction if you want tracing.
    /// The drainer runs in a separate task and does not block the executor.
    pub fn start_flight_recorder(&self, gpu_sink: bool, ws_tx: Option<tokio::sync::mpsc::Sender<Vec<crate::orchestra::flight_recorder::TraceEvent>>>) {
        let exec = Arc::new(self.clone());
        crate::orchestra::flight_recorder::start_background_drain(exec, gpu_sink, ws_tx);
    }

    fn schedule_statement(&self, stmt: &CodeTaal) -> Option<(String, u16)> {
        let peers = match self.discovery.peers.lock() {
            Ok(p) => p.clone(),
            Err(_) => return None,
        };
        if peers.is_empty() {
            return None;
        }

        let is_gpu = matches!(stmt, CodeTaal::GpuKernel(_) | CodeTaal::MatMul { .. });

        let candidates: Vec<String> = if is_gpu {
            peers
                .iter()
                .filter(|(_, caps)| caps.gpu_count > 0)
                .map(|(ip, _)| ip.clone())
                .collect()
        } else {
            peers.keys().cloned().collect()
        };

        let list = if candidates.is_empty() {
            peers.keys().cloned().collect()
        } else {
            candidates
        };

        let idx = SCHEDULER_INDEX.fetch_add(1, Ordering::Relaxed) % list.len();
        let ip = list[idx].clone();
        Some((ip, 9003))
    }

    pub fn enrich_error(&self, e: anyhow::Error, stmt: &CodeTaal) -> anyhow::Error {
        crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::ErrorPropagated, crate::orchestra::flight_recorder::node_id_for(stmt, Some(&self.memory)), 1);
        let msg = e.to_string();
        if msg.contains("[Fout op regel ") {
            return e;
        }
        let line = self.memory.get_var_native("__LAST_ERR_LINE__").map(|v| v.to_string()).unwrap_or_default();
        let col = self.memory.get_var_native("__LAST_ERR_COL__").map(|v| v.to_string()).unwrap_or_default();
        if !line.is_empty() && !col.is_empty() {
            anyhow::anyhow!("[Fout op regel {}:{}] {}", line, col, msg)
        } else {
            e
        }
    }

    pub fn execute_ast(
        &self,
        ast: Vec<CodeTaal>,
        ctx: crate::common::context::ExecutionContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        Box::pin(async move {
            match self.execute_ast_internal(ast, ctx).await {
                Ok(v) => Ok(v),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.starts_with("[Fout op regel") {
                        return Err(e);
                    }
                    let line = self.memory.get_var_native("__LAST_ERR_LINE__").map(|v| v.to_string()).unwrap_or_default();
                    let col = self.memory.get_var_native("__LAST_ERR_COL__").map(|v| v.to_string()).unwrap_or_default();
                    if !line.is_empty() && !col.is_empty() {
                        let enriched = format!("[Fout op regel {}:{}] {}", line, col, msg);
                        // Record propagation trace
                        if crate::orchestra::flight_recorder::is_enabled() {
                            crate::orchestra::flight_recorder::record(
                                crate::orchestra::flight_recorder::TraceKind::ErrorPropagated,
                                0, // Fallback ID as we don't have the exact stmt here, but we caught it at block boundary
                                0,
                            );
                        }
                        Err(anyhow::anyhow!("{}", enriched))
                    } else {
                        Err(e)
                    }
                }
            }
        })
    }

    fn execute_ast_internal(
        &self,
        initial_ast: Vec<CodeTaal>,
        initial_ctx: crate::common::context::ExecutionContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        Box::pin(async move {
            let mut stack = crate::orchestra::trampoline::TrampolineStack::new();
            stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                statements: initial_ast,
                pc: 0,
                ctx: initial_ctx,
            })?;

            while let Some(frame) = stack.pop() {
                let (mut statements, mut pc, ctx): (Vec<helheim_lang::ast::CodeTaal>, usize, crate::common::context::ExecutionContext) = match frame {
                    crate::orchestra::trampoline::EvalFrame::Statements { statements, pc, ctx } => (statements, pc, ctx),
                    crate::orchestra::trampoline::EvalFrame::PopScope => {
                        self.memory.pop_scope();
                        continue;
                    }
                };

                let mut break_inner = false;
                while pc < statements.len() {
                    if break_inner { break; }
                    let i = pc;
                    let stmt = std::mem::replace(&mut statements[pc], helheim_lang::ast::CodeTaal::LocationMarker { line: 0, col: 0 });
                    pc += 1;
                    
                    if let CodeTaal::LocationMarker { line, col } = stmt {
                        self.memory.set_var_native("__LAST_ERR_LINE__".to_string(), crate::orchestra::memory::HelheimType::String(line.to_string()));
                        self.memory.set_var_native("__LAST_ERR_COL__".to_string(), crate::orchestra::memory::HelheimType::String(col.to_string()));
                        continue;
                    }
                
                // Phase 2: Consume Gas for every executed statement
                if let Err(e) = ctx.consume_gas(1) {
                    return Err(e);
                }
                
                // Vraag 6: Delimited Continuations. Sla de rest van het block op zodat perform het kan vangen.
                let remaining = &statements[i + 1..];
                crate::orchestra::continuation::set_rest_ast(&self.memory, remaining);
                if let Err(e) = ctx.check_timeout() {
                    return Err(e);
                }

                // === Flight Recorder hook (Vraag 3) - zero-overhead when disabled ===
                // Relaxed load + predictable branch = ~1 cycle when tracing is off.
                if crate::orchestra::flight_recorder::is_enabled() {
                    crate::orchestra::flight_recorder::record(
                        crate::orchestra::flight_recorder::TraceKind::ExprEvalStart,
                        crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory)),
                        0,
                    );
                }

                match stmt {
                    CodeTaal::GpuKernel(kernel_def) => {
                        tracing::debug!("[EXECUTOR]: GpuKernel detectie: {}", kernel_def.name);
                        let backend = crate::gpu::get_backend();
                        match backend.compile(&kernel_def) {
                            Ok(compiled) => {
                                tracing::debug!("[EXECUTOR]: Kernel succesvol gecompileerd op {}", backend.name());
                                // We zouden hier argumenten moeten resolven naar GpuPtr
                                if let Err(e) = backend.launch(&compiled, &[]) {
                                    tracing::error!("[EXECUTOR ERROR]: Launch gefaald: {}", e);
                                } else {
                                    tracing::debug!("[EXECUTOR]: Kernel gelanceerd!");
                                    let _ = backend.synchronize();
                                }
                            }
                            Err(e) => {
                                tracing::error!("[EXECUTOR ERROR]: Compilatie gefaald: {}", e);
                            }
                        }
                    }
                    CodeTaal::MatMul { m, n, k } => {
                        tracing::debug!(
                            "[KERNEL]: Synthesis of Tiled MatMul {}x{}x{} (Shared Memory Enabled)...",
                            m, n, k
                        );
                        // 1. Synthesize PTX (JIT)
                        let ptx = synthesis::KernelSynthesisEngine::synthesize(CodeTaal::MatMul {
                            m,
                            n,
                            k,
                        })
                        .unwrap_or_else(|_| String::new());

                        // 2. Execute on Hardware
                        tracing::debug!("[GPU]: Launching Kernel on Nvidia RTX 5060 Ti...");
                        let id_a = match crate::gpu::gpu_alloc_tensor_random(m, k) {
                            Ok(id) => id,
                            Err(e) => { tracing::error!("[GPU]: Tensor A allocatie gefaald: {}", e); return Err(anyhow::anyhow!("GPU allocatie gefaald: {}", e)); }
                        };
                        let id_b = match crate::gpu::gpu_alloc_tensor_random(k, n) {
                            Ok(id) => id,
                            Err(e) => { tracing::error!("[GPU]: Tensor B allocatie gefaald: {}", e); return Err(anyhow::anyhow!("GPU allocatie gefaald: {}", e)); }
                        };
                        let id_c = match crate::gpu::gpu_alloc_tensor_empty(m, n) {
                            Ok(id) => id,
                            Err(e) => { tracing::error!("[GPU]: Tensor C allocatie gefaald: {}", e); return Err(anyhow::anyhow!("GPU allocatie gefaald: {}", e)); }
                        };
                        match crate::gpu::gpu_execute_raw_ptx_ids(&ptx, id_a, id_b, id_c, m, n, k) {
                            Ok(gflops) => tracing::debug!(
                                "[GPU]: ✅ Execution Complete. Performance: {:.2} GFLOPS",
                                gflops
                            ),
                            Err(e) => tracing::error!("[ERROR]: GPU Runtime Fail: {}", e),
                        }
                    }

                    CodeTaal::Return { value } => {
                        let eval = match value {
                            Some(box_val) => self.evaluate_ast_expr(&*box_val, ctx.clone()).await.unwrap_or_default(),
                            None => "".to_string(),
                        };
                        return Ok(Some(eval));
                    }
                    CodeTaal::Throw { ref message } => {
                        let eval = self.evaluate_expression(message);
                        return Err(self.enrich_error(anyhow::anyhow!("Uncaught exception: {}", eval), &stmt));
                    }
                    CodeTaal::RuneOp { command } => {
                        if !ctx.is_privileged {
                            return Err(anyhow::anyhow!("[SECURITY]: Rune execution requires elevated privileges."));
                        }
                        tracing::debug!("[RUNE]: Executing bare-metal Rune...");
                        match unsafe { crate::common::rune::RuneEngine::execute_raw_rune(&command) } {
                            Ok(res) => tracing::debug!("[RUNE_OUT]: {}", res),
                            Err(e) => tracing::debug!("[RUNE_ERR]: {}", e),
                        }
                    }
                    CodeTaal::Print { message } => {
                        let evaluated_value = self.evaluate_expression(&message);
                        let resolved_val = self.memory.resolve_value(&evaluated_value);
                        // Strip quotes for printing strings cleanly
                        let clean_val = resolved_val.trim_matches('"');
                        println!("{}", clean_val);
                    }
                    CodeTaal::FileOp { action, path, content } => {
                        if !ctx.is_privileged {
                            return Err(anyhow::anyhow!("[SECURITY]: Bestandstoegang vereist Elevated Privileges."));
                        }
                        // Perform the I/O (beveiligde std bib)
                        // Resolve exprs to strings where possible (sync for paths)
                        let path_str = self.code_taal_to_string_sync(&path);
                        match action.as_str() {
                            "read" => {
                                match tokio::fs::read_to_string(&path_str).await {
                                    Ok(data) => {
                                        tracing::debug!("[FS READ]: {} ({} bytes)", path_str, data.len());
                                        self.memory.set_var_native("__last_read".to_string(), crate::orchestra::memory::HelheimType::String(data.clone()));
                                        if ctx.is_distributed { self.distributed.set_global("__last_read", data.clone()); }
                                    }
                                    Err(e) => tracing::error!("[FS READ ERROR]: {} : {}", path_str, e),
                                }
                            }
                            "write" => {
                                let content_str = if let Some(c) = content {
                                    self.code_taal_to_string_sync(&c)
                                } else { String::new() };
                                match tokio::fs::write(&path_str, content_str.as_bytes()).await {
                                    Ok(_) => tracing::debug!("[FS WRITE]: {} ({} bytes)", path_str, content_str.len()),
                                    Err(e) => tracing::error!("[FS WRITE ERROR]: {} : {}", path_str, e),
                                }
                            }
                            _ => tracing::debug!("[FS]: unknown action {}", action),
                        }
                    }
                    CodeTaal::HttpOp { method, url } => {
                        let url_str = self.code_taal_to_string_sync(&url);
                        if !ctx.is_privileged && !crate::orchestra::system::is_ssrf_safe(&url_str).await {
                            return Err(anyhow::anyhow!("[SECURITY]: SSRF geblokkeerd — interne URL/DNS rebinding niet toegestaan in sandbox."));
                        }
                        if method.to_uppercase() == "GET" {
                            match tokio::task::spawn_blocking(move || ureq::get(&url_str).call()).await {
                                Ok(Ok(mut resp)) => {
                                    let body = resp.body_mut().read_to_string().unwrap_or_default();
                                    tracing::debug!("[HTTP GET]: -> {} bytes", body.len());
                                    self.memory.set_var_native("__last_http".to_string(), crate::orchestra::memory::HelheimType::String(body.clone()));
                                    if ctx.is_distributed { self.distributed.set_global("__last_http", body.clone()); }
                                }
                                Ok(Err(e)) => tracing::error!("[HTTP ERROR]: : {}", e),
                                Err(e) => tracing::error!("[HTTP ERROR]: Tokio thread pool error: {}", e),
                            }
                        } else {
                            tracing::debug!("[HTTP]: {} {} (only GET supported in this lowering)", method, url_str);
                        }
                    }
                    CodeTaal::TcpOp { action, host, data } => {
                        let host_str = self.code_taal_to_string_sync(&host);
                        if !ctx.is_privileged {
                            tracing::debug!("[SECURITY]: TCP verbindingen vereisen elevated privileges.");
                            continue;
                        }
                        match action.as_str() {
                            "connect" => {
                                match tokio::net::TcpStream::connect(&host_str).await {
                                    Ok(_) => tracing::debug!("[TCP CONNECT]: Verbonden met {}", host_str),
                                    Err(e) => tracing::debug!("[TCP ERROR]: Kan niet verbinden met {}: {}", host_str, e),
                                }
                            }
                            "listen" => {
                                match tokio::net::TcpListener::bind(&host_str).await {
                                    Ok(_) => tracing::debug!("[TCP LISTEN]: Luisteren op {}", host_str),
                                    Err(e) => tracing::debug!("[TCP ERROR]: Kan niet luisteren op {}: {}", host_str, e),
                                }
                            }
                            "send" => {
                                let data_str = if let Some(d) = data { self.code_taal_to_string_sync(&d) } else { String::new() };
                                match tokio::net::TcpStream::connect(&host_str).await {
                                    Ok(mut stream) => {
                                        use tokio::io::AsyncWriteExt;
                                        if let Err(e) = stream.write_all(data_str.as_bytes()).await {
                                            tracing::debug!("[TCP ERROR]: Schrijven naar {} mislukt: {}", host_str, e);
                                        } else {
                                            tracing::debug!("[TCP SEND]: {} bytes naar {}", data_str.len(), host_str);
                                        }
                                    }
                                    Err(e) => tracing::debug!("[TCP ERROR]: Verbinden met {} mislukt: {}", host_str, e),
                                }
                            }
                            _ => tracing::debug!("[TCP ERROR]: Onbekende TCP actie '{}'", action),
                        }
                    }
                    CodeTaal::FunctionCall { .. } | CodeTaal::QualifiedCall { .. } => {
                        let _ = self.evaluate_ast_expr(&stmt, ctx.clone()).await?;
                    }
                    // === TOP-LEVEL EFFECT DISPATCHER (Stap B) ===
                    CodeTaal::Handle { effect, handlers, body } => {
                        let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                        let full_effect = if let Some(ns) = &ctx.current_module {
                            if effect.contains("::") { effect.clone() } else { format!("{}::{}", ns, effect) }
                        } else {
                            effect.clone()
                        };
                        for (op_name, h_body) in handlers {
                            let handler_var = format!("__effect_handler_{}_{}", full_effect, op_name);
                            let ast_str = serde_json::to_string(&h_body).unwrap_or_default();
                            self.memory.set_var_native(handler_var, HelheimType::String(ast_str));
                        }
                        let res = self.execute_ast(vec![(*body).clone()], ctx.clone()).await;
                        match res {
                            Ok(Some(v)) => return Ok(Some(v)),
                            Ok(None) => {},
                            Err(e) => return Err(e),
                        }
                    }
                    CodeTaal::Perform { ref effect, ref operation, ref args } => {
                        let full_effect = if let Some(ns) = &ctx.current_module {
                            if effect.contains("::") { effect.clone() } else { format!("{}::{}", ns, effect) }
                        } else {
                            effect.clone()
                        };
                        let handler_var = format!("__effect_handler_{}_{}", full_effect, operation);
                        if let Some(HelheimType::String(ast_str)) = self.memory.get_var_native(&handler_var) {
                            if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                                let continuation = crate::orchestra::continuation::capture_continuation(&stmt, &self.memory, &effect, &self.distributed, ctx.is_privileged)?;
                                
                                let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                let json_str = serde_json::to_string(&continuation).map_err(|e| anyhow::anyhow!("Continuation serialization fail: {}", e))?;
                                let b64_str = base64::engine::general_purpose::STANDARD.encode(json_str);
                                self.memory.set_var_native("resume_k".to_string(), HelheimType::String(b64_str));
                                
                                for (idx, arg) in args.iter().enumerate() {
                                    if let Ok(arg_val) = self.evaluate_ast_expr(arg, ctx.clone()).await {
                                        self.memory.set_var_native(format!("arg{}", idx + 1), HelheimType::parse(&arg_val));
                                    }
                                }
                                
                                let _ = self.execute_ast(vec![handler_ast], ctx.clone()).await;
                            }
                        } else {
                            let base_effect = effect.rsplit("::").next().unwrap_or(effect);
                            match (base_effect, operation.as_str()) {
                                ("Tcp", _) => {
                                    let _ = self.evaluate_tcp_primitive(&operation, &stmt, &self.memory, ctx.clone()).await;
                                }
                                ("Actor", "send") => {
                                    if args.len() == 2 {
                                        let temp_stmt = CodeTaal::SendMessage { target: Box::new(args[0].clone()), message: Box::new(args[1].clone()) };
                                        let _ = self.execute_ast(vec![temp_stmt], ctx.clone()).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Actor::send: expected 2, got {}", args.len()));
                                    }
                                }
                                ("Actor", "spawn") => {
                                    if args.len() >= 1 {
                                        let code = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let body_ast = helheim_lang::parser::HelParser::parse(&code).unwrap_or_default();
                                        
                                        let mut strategy_str = "Stop".to_string();
                                        if args.len() == 2 {
                                            strategy_str = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        }

                                        let temp_stmt = CodeTaal::Spawn { name: None, body: Box::new(CodeTaal::Block { statements: body_ast }) };
                                        
                                        let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                        self.memory.set_var_native("__supervision_strategy".to_string(), crate::orchestra::memory::HelheimType::String(strategy_str));
                                        
                                        let _ = self.execute_ast(vec![temp_stmt], ctx.clone()).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Actor::spawn: expected 1 or 2, got {}", args.len()));
                                    }
                                }
                                ("Trace", "record") => {
                                    if args.len() == 3 {
                                        if let (Ok(k), Ok(n), Ok(p)) = (self.evaluate_ast_expr(&args[0], ctx.clone()).await, self.evaluate_ast_expr(&args[1], ctx.clone()).await, self.evaluate_ast_expr(&args[2], ctx.clone()).await) {
                                            if let (Ok(kind), Ok(node), Ok(payload)) = (k.parse::<u8>(), n.parse::<u64>(), p.parse::<u64>()) {
                                                let trace_kind = unsafe { std::mem::transmute::<u8, crate::orchestra::flight_recorder::TraceKind>(kind.min(11)) };
                                                crate::orchestra::flight_recorder::record(trace_kind, node, payload);
                                            }
                                        }
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Trace::record: expected 3, got {}", args.len()));
                                    }
                                }
                                ("Asm", "inline") => {
                                    if args.len() >= 2 {
                                        let target = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let code = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        let temp_stmt = CodeTaal::InlineAssembly { target, code, inputs: vec![], outputs: vec![], clobbers: vec![], fallback: None };
                                        let _ = self.execute_ast(vec![temp_stmt], ctx.clone()).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Asm::inline: expected at least 2, got {}", args.len()));
                                    }
                                }
                                ("Swarm", "dispatch") => {
                                    if args.len() >= 3 {
                                        let ip = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port_str = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port: u16 = port_str.parse().unwrap_or(8080);
                                        
                                        let payload = self.evaluate_ast_expr(&args[2], ctx.clone()).await?;
                                        
                                        let _ = crate::network::hsp_node::SwarmEngine::dispatch(&ip, port, &payload).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Swarm::dispatch: expected at least 3, got {}", args.len()));
                                    }
                                }
                                ("Swarm", "migrate") => {
                                    if args.len() >= 2 {
                                        let ip = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port_str = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port: u16 = port_str.parse().unwrap_or(8080);

                                        // Monnikenwerk 1.3: Check voor `Migratie.voor_vertrek` handler vóór capture
                                        let handler_var = "__effect_handler_Migratie_voor_vertrek";
                                        if let Some(HelheimType::String(ast_str)) = self.memory.get_var_native(handler_var) {
                                            if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                                                tracing::debug!("[MIGRATIE]: 'voor_vertrek' handler gevonden, executing...");
                                                let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                                let res = self.execute_ast(vec![handler_ast], ctx.clone()).await;
                                                if let Err(e) = res {
                                                    // [W·AG·AF] C1 Review: Error propagation from voor_vertrek blocks teleport
                                                    tracing::error!("[MIGRATIE] 'voor_vertrek' gecrasht. Teleportatie geannuleerd: {}", e);
                                                    return Err(self.enrich_error(e, &stmt));
                                                }
                                            }
                                        }

                                        let continuation = crate::orchestra::continuation::capture_continuation(&stmt, &self.memory, effect, &self.distributed, ctx.is_privileged)?;
                                        let wrapper = serde_json::json!({
                                            "type": "TeleportContinuation",
                                            "continuation": continuation
                                        });
                                        let payload_str = wrapper.to_string();
                                        crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::MigrateCapture, crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory)), payload_str.len() as u64);

                                        match crate::network::hsp_node::SwarmEngine::dispatch(&ip, port, &payload_str).await {
                                            Ok(_) => {
                                                crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::MigrateTeleport, crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory)), 1);
                                                return Ok(None); // Teleport success, abort local execution
                                            },
                                            Err(e) => {
                                                crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::MigrateTeleport, crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory)), 0);
                                                return Err(self.enrich_error(anyhow::anyhow!("Teleport failed: {}", e), &stmt));
                                            }
                                        }
                                    }
                                    return Err(anyhow::anyhow!("Swarm.migrate vereist target_ip, target_port"));
                                }
                                _ => {
                                    tracing::debug!("[EFFECT ERROR] No handler and no native fallback for {}.{}", effect, operation);
                                }
                            }
                        }
                    }
CodeTaal::QualifiedPerform { ref ns, ref effect, ref operation, ref args } => {
                        let full_effect = format!("{}::{}", ns, effect);
                        let handler_var = format!("__effect_handler_{}_{}", full_effect, operation);
                        if let Some(HelheimType::String(ast_str)) = self.memory.get_var_native(&handler_var) {
                            if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                                let continuation = crate::orchestra::continuation::capture_continuation(&stmt, &self.memory, &effect, &self.distributed, ctx.is_privileged)?;
                                
                                let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                let json_str = serde_json::to_string(&continuation).map_err(|e| anyhow::anyhow!("Continuation serialization fail: {}", e))?;
                                let b64_str = base64::engine::general_purpose::STANDARD.encode(json_str);
                                self.memory.set_var_native("resume_k".to_string(), HelheimType::String(b64_str));
                                
                                for (idx, arg) in args.iter().enumerate() {
                                    if let Ok(arg_val) = self.evaluate_ast_expr(arg, ctx.clone()).await {
                                        self.memory.set_var_native(format!("arg{}", idx + 1), HelheimType::parse(&arg_val));
                                    }
                                }
                                
                                let _ = self.execute_ast(vec![handler_ast], ctx.clone()).await;
                            }
                        } else {
                            let base_effect = effect.rsplit("::").next().unwrap_or(effect);
                            match (base_effect, operation.as_str()) {
                                ("Tcp", _) => {
                                    let _ = self.evaluate_tcp_primitive(&operation, &stmt, &self.memory, ctx.clone()).await;
                                }
                                ("Actor", "send") => {
                                    if args.len() == 2 {
                                        let temp_stmt = CodeTaal::SendMessage { target: Box::new(args[0].clone()), message: Box::new(args[1].clone()) };
                                        let _ = self.execute_ast(vec![temp_stmt], ctx.clone()).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Actor::send: expected 2, got {}", args.len()));
                                    }
                                }
                                ("Actor", "spawn") => {
                                    if args.len() >= 1 {
                                        let code = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let body_ast = helheim_lang::parser::HelParser::parse(&code).unwrap_or_default();
                                        
                                        let mut strategy_str = "Stop".to_string();
                                        if args.len() == 2 {
                                            strategy_str = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        }

                                        let temp_stmt = CodeTaal::Spawn { name: None, body: Box::new(CodeTaal::Block { statements: body_ast }) };
                                        
                                        let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                        self.memory.set_var_native("__supervision_strategy".to_string(), crate::orchestra::memory::HelheimType::String(strategy_str));
                                        
                                        let _ = self.execute_ast(vec![temp_stmt], ctx.clone()).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Actor::spawn: expected 1 or 2, got {}", args.len()));
                                    }
                                }
                                ("Trace", "record") => {
                                    if args.len() == 3 {
                                        if let (Ok(k), Ok(n), Ok(p)) = (self.evaluate_ast_expr(&args[0], ctx.clone()).await, self.evaluate_ast_expr(&args[1], ctx.clone()).await, self.evaluate_ast_expr(&args[2], ctx.clone()).await) {
                                            if let (Ok(kind), Ok(node), Ok(payload)) = (k.parse::<u8>(), n.parse::<u64>(), p.parse::<u64>()) {
                                                let trace_kind = unsafe { std::mem::transmute::<u8, crate::orchestra::flight_recorder::TraceKind>(kind.min(11)) };
                                                crate::orchestra::flight_recorder::record(trace_kind, node, payload);
                                            }
                                        }
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Trace::record: expected 3, got {}", args.len()));
                                    }
                                }
                                ("Asm", "inline") => {
                                    if args.len() >= 2 {
                                        let target = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let code = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        let temp_stmt = CodeTaal::InlineAssembly { target, code, inputs: vec![], outputs: vec![], clobbers: vec![], fallback: None };
                                        let _ = self.execute_ast(vec![temp_stmt], ctx.clone()).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Asm::inline: expected at least 2, got {}", args.len()));
                                    }
                                }
                                ("Swarm", "dispatch") => {
                                    if args.len() >= 3 {
                                        let ip = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port_str = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port: u16 = port_str.parse().unwrap_or(8080);
                                        
                                        let payload = self.evaluate_ast_expr(&args[2], ctx.clone()).await?;
                                        
                                        let _ = crate::network::hsp_node::SwarmEngine::dispatch(&ip, port, &payload).await;
                                    } else {
                                        return Err(anyhow::anyhow!("Arity mismatch for Swarm::dispatch: expected at least 3, got {}", args.len()));
                                    }
                                }
                                ("Swarm", "migrate") => {
                                    if args.len() >= 2 {
                                        let ip = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port_str = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                                        let port: u16 = port_str.parse().unwrap_or(8080);

                                        // Monnikenwerk 1.3: Check voor `Migratie.voor_vertrek` handler vóór capture
                                        let handler_var = "__effect_handler_Migratie_voor_vertrek";
                                        if let Some(HelheimType::String(ast_str)) = self.memory.get_var_native(handler_var) {
                                            if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                                                tracing::debug!("[MIGRATIE]: 'voor_vertrek' handler gevonden, executing...");
                                                let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                                let res = self.execute_ast(vec![handler_ast], ctx.clone()).await;
                                                if let Err(e) = res {
                                                    // [W·AG·AF] C1 Review: Error propagation from voor_vertrek blocks teleport
                                                    tracing::error!("[MIGRATIE] 'voor_vertrek' gecrasht. Teleportatie geannuleerd: {}", e);
                                                    return Err(self.enrich_error(e, &stmt));
                                                }
                                            }
                                        }

                                        let continuation = crate::orchestra::continuation::capture_continuation(&stmt, &self.memory, effect, &self.distributed, ctx.is_privileged)?;
                                        let wrapper = serde_json::json!({
                                            "type": "TeleportContinuation",
                                            "continuation": continuation
                                        });
                                        let payload_str = wrapper.to_string();
                                        crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::MigrateCapture, crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory)), payload_str.len() as u64);

                                        match crate::network::hsp_node::SwarmEngine::dispatch(&ip, port, &payload_str).await {
                                            Ok(_) => {
                                                crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::MigrateTeleport, crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory)), 1);
                                                return Ok(None); // Teleport success, abort local execution
                                            },
                                            Err(e) => {
                                                crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::MigrateTeleport, crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory)), 0);
                                                return Err(self.enrich_error(anyhow::anyhow!("Teleport failed: {}", e), &stmt));
                                            }
                                        }
                                    }
                                    return Err(anyhow::anyhow!("Swarm.migrate vereist target_ip, target_port"));
                                }
                                _ => {
                                    tracing::debug!("[EFFECT ERROR] No handler and no native fallback for {}.{}", effect, operation);
                                }
                            }
                        }
                    }
                    CodeTaal::Resume { ref continuation, ref value } => {
                        if let Ok(cont_b64) = self.evaluate_ast_expr(&continuation, ctx.clone()).await {
                            if let Ok(cont_bytes) = base64::engine::general_purpose::STANDARD.decode(cont_b64.trim_matches('"')) {
                                if let Ok(cont) = serde_json::from_slice::<crate::orchestra::continuation::SerializableContinuation>(&cont_bytes) {
                                    let resume_val_str = self.evaluate_ast_expr(&value, ctx.clone()).await?;
                                    let resume_val = HelheimType::parse(&resume_val_str);
                                    self.memory.restore_snapshot(&cont.captured_memory);
                                    
                                    let node_id = crate::orchestra::flight_recorder::node_id_for(&stmt, Some(&self.memory));
                                    crate::trace_event!(crate::orchestra::flight_recorder::TraceKind::MigrateResume, node_id, 1);

                                    // Monnikenwerk 1.4: Check voor na_aankomst handler NA snapshot restore
                                    let handler_var = "__effect_handler_Migratie_na_aankomst";
                                    if let Some(HelheimType::String(ast_str)) = self.memory.get_var_native(handler_var) {
                                        if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                                            tracing::debug!("[MIGRATIE]: 'na_aankomst' handler gevonden, executing op nieuwe node...");
                                            let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                            let res = self.execute_ast(vec![handler_ast], ctx.clone()).await;
                                            if let Err(e) = res {
                                                tracing::error!("[MIGRATIE] 'na_aankomst' handler faalde. Resources zijn mogelijk niet heropend: {}", e);
                                                return Err(e);
                                            }
                                            // [W·AG·AF] C1 Review: State validatie na resume
                                            tracing::debug!("[MIGRATIE]: 'na_aankomst' contract voldaan. Lokale resource-state is heropend volgens handler logic.");
                                        }
                                    }
                                    
                                    if let Ok(stack) = serde_json::from_str::<Vec<CodeTaal>>(&cont.captured_stack_json) {
                                        if !stack.is_empty() {
                                            if let CodeTaal::Perform { .. } = &stack[0] {
                                                self.memory.set_var_native("__perform_result".to_string(), resume_val);
                                                let _ = self.execute_ast(stack[1..].to_vec(), ctx.clone()).await;
                                            } else if let CodeTaal::VarDef { name, .. } = &stack[0] {
                                                self.memory.set_var(name.clone(), resume_val_str.clone());
                                                let _ = self.execute_ast(stack[1..].to_vec(), ctx.clone()).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    CodeTaal::Module { name: ns, body } => {
                        let mut new_ctx = ctx.clone();
                        new_ctx.current_module = Some(ns);
                        let _ = self.execute_ast(body, new_ctx).await;
                    }
                    CodeTaal::Gebruik { ref path, module_naam: _ } => {
                        let clean_path = path.trim_matches('"').to_string();
                        if clean_path.ends_with(".hel") {
                            // Lees en parse the .hel file
                            if let Ok(content) = tokio::fs::read_to_string(&clean_path).await {
                                if let Ok(ast) = crate::orchestra::parser::HelParser::parse(&content) {
                                    for stmt in ast {
                                        match stmt {
                                            CodeTaal::FunctionDef { name, is_pub, params, body } => {
                                                self.memory.register_ast_function(ctx.current_module.as_deref(), name, params, body, is_pub);
                                            }
                                            CodeTaal::ModelDef { name, fields } => {
                                                self.memory.register_model(ctx.current_module.as_deref(), name, fields);
                                            }
                                            _ => {
                                                // Execute any top-level statements inside the imported module context
                                                let _ = self.execute_ast(vec![stmt], ctx.clone()).await;
                                            }
                                        }
                                    }
                                    tracing::debug!("[MODULE]: Geladen Helheim module '{}'", clean_path);
                                } else {
                                    tracing::debug!("[MODULE ERROR]: Fout bij parsen van '{}'", clean_path);
                                }
                            } else {
                                tracing::debug!("[MODULE ERROR]: Kan bestand '{}' niet inlezen.", clean_path);
                            }
                        } else {
                            if !ctx.is_privileged {
                                return Err(self.enrich_error(anyhow::anyhow!("[SECURITY]: Ongeldige bevoegdheid: Sandbox mag geen native/WASM modules laden"), &stmt));
                            }
                            
                            // 5.4: Native module signing integratie - Gebruik package_manager ipv directe WasmModuleLoader bypass
                            match self.package_manager.load_verified_native(&clean_path, 0).await {
                                Ok(_module) => {
                                    tracing::info!("[AST]: Wasm module ingeladen vanaf '{}'.", clean_path);
                                }
                                Err(e) => {
                                    tracing::debug!("[EXECUTOR]: Warning: CodeTaal::Gebruik for '{}' native load failed (not installed/verified?): {}", clean_path, e);
                                }
                            }
                        }
                    }
                    CodeTaal::FunctionDef { name, is_pub, params, body } => {
                        tracing::debug!(
                            "[MEMORY]: Opslaan AST-functie '{}' met {} argumenten...",
                            name,
                            params.len()
                        );
                        self.memory.register_ast_function(ctx.current_module.as_deref(), name.clone(), params, body, is_pub);
                    }
                    CodeTaal::ModelDef { name, fields } => {
                        tracing::debug!("[MEMORY]: Blauwdruk opgeslagen voor model '{}' met {} velden.", name, fields.len());
                        self.memory.register_model(ctx.current_module.as_deref(), name, fields);
                    }
                    CodeTaal::ModelInit { model_name, args: _args } => {
                        // Not used in execute_ast natively because VarDef intercepts 'nieuw'
                        tracing::debug!("[AST]: Onverwachte losse ModelInit voor {}", model_name);
                    }
                    CodeTaal::VarDef { ref name, ref value } => {
                        // Extract literal or variable get, or resolve basic op
                        let value_str = match &**value {
                            CodeTaal::Literal(l) => {
                                match l {
                                    helheim_lang::ast::LiteralValue::String(s) => format!("\"{}\"", s),
                                    _ => l.to_string(),
                                }
                            },
                            CodeTaal::VarGet { name } => self.memory.resolve_value(name),
                            CodeTaal::Op { .. } => {
                                let context = self.build_eval_context(&*value);

                                let gpu_backend = crate::gpu::get_backend();
                                match gpu_backend.execute_lowered_block(&*value, &context) {
                                    Ok(Some(val)) => {
                                        tracing::debug!("[EXECUTOR]: Op evaluated on GPU via PTX JIT path. Result: {}", val);
                                        let result_str = val.to_string();
                                        self.memory.set_var_native(name.clone(), HelheimType::parse(&result_str));
                                        if ctx.is_distributed { self.distributed.set_global(name.as_str(), result_str.clone()); }
                                        result_str
                                    }
                                    _ => {
                                        // Let evaluate_ast_expr handle it cleanly so that nested Ops and logic work
                                        self.evaluate_ast_expr(&*value, ctx.clone()).await?
                                    }
                                }
                            }
                            CodeTaal::FileOp { .. } | CodeTaal::HttpOp { .. } | CodeTaal::TcpOp { .. } => {
                                // Perform I/O at VarDef time so `zet x = lees p` or `zet x = haal u` works
                                // We can't easily await here in the match without restructuring, so delegate to the top level handler
                                // by executing the sub expr (side effect + last read)
                                // For now fall back to the generic execution path for the value (it will run the I/O arm)
                                // and use the magic last read var.
                                let _ = Box::pin(self.execute_ast(vec![(**value).clone()], ctx.clone())).await;
                                self.memory.resolve_value("__last_read")
                            }
                            CodeTaal::ListLiteral { items } => {
                                // Set list in memory for tensors etc.
                                let mut string_items = Vec::new();
                                let json_items: Vec<serde_json::Value> = items.iter().map(|l| match l {
                                    helheim_lang::ast::LiteralValue::Bool(b) => {
                                        string_items.push(if *b { "waar" } else { "onwaar" }.to_string());
                                        serde_json::json!(if *b { "waar" } else { "onwaar" })
                                    },
                                    helheim_lang::ast::LiteralValue::Int(i) => {
                                        string_items.push(i.to_string());
                                        serde_json::json!(*i)
                                    },
                                    helheim_lang::ast::LiteralValue::Float(f) => {
                                        string_items.push(f.to_string());
                                        serde_json::json!(*f)
                                    },
                                    helheim_lang::ast::LiteralValue::String(s) => {
                                        string_items.push(format!("\"{}\"", s));
                                        serde_json::json!(s)
                                    },
                                    helheim_lang::ast::LiteralValue::List(sub) => {
                                        string_items.push("[list]".to_string());
                                        serde_json::json!(sub.iter().map(|x| x.to_string()).collect::<Vec<_>>())
                                    },
                                    helheim_lang::ast::LiteralValue::Bytes(b) => {
                                        // Bytes in list context: represent as hex string for now
                                        let hex = b.iter().map(|bb| format!("{:02x}", bb)).collect::<Vec<_>>().join("");
                                        string_items.push(format!("b[{}]", hex));
                                        serde_json::json!(hex)
                                    },
                                    helheim_lang::ast::LiteralValue::Pointer(p) => {
                                        string_items.push(format!("ptr(0x{:x})", p));
                                        serde_json::json!(p)
                                    },
                                    helheim_lang::ast::LiteralValue::Void => {
                                        string_items.push("niets".to_string());
                                        serde_json::json!(null)
                                    },
                                }).collect();
                                self.memory.set_var_native(name.clone(), HelheimType::List(json_items.clone()));
                                if ctx.is_distributed { self.distributed.set_global(&name, serde_json::to_string(&json_items).map_err(|e| anyhow::anyhow!("State sync fail: {}", e))?); }
                                format!("[{}]", string_items.join(", "))
                            }
                            CodeTaal::MatrixLiteral { rows } => {
                                // 2D matrix
                                let mut flat: Vec<serde_json::Value> = Vec::new();
                                let mut string_items = Vec::new();
                                for row in rows {
                                    for item in row {
                                        let v = match item {
                                            helheim_lang::ast::LiteralValue::Bool(b) => {
                                                string_items.push(if *b { "waar" } else { "onwaar" }.to_string());
                                                serde_json::json!(if *b { "waar" } else { "onwaar" })
                                            },
                                            helheim_lang::ast::LiteralValue::Bytes(b) => {
                                                let hex = b.iter().map(|bb| format!("{:02x}", bb)).collect::<Vec<_>>().join("");
                                                string_items.push(format!("b[{}]", hex));
                                                serde_json::json!(hex)
                                            },
                                            _ => {
                                                string_items.push(item.to_string());
                                                serde_json::json!(item.to_string())
                                            },
                                        };
                                        flat.push(v);
                                    }
                                }
                                self.memory.set_var_native(name.clone(), HelheimType::List(flat.clone()));
                                if ctx.is_distributed { self.distributed.set_global(&name, serde_json::to_string(&flat).map_err(|e| anyhow::anyhow!("State sync fail: {}", e))?); }
                                format!("[{}]", string_items.join(", "))
                            }
                            CodeTaal::Block { .. } => {
                                // Context binding + tensor packing (Host-to-Device)
                                // If a free var is a List of bools, pack on CPU into u32 bitmask and pass as Int.
                                let context = self.build_eval_context(&*value);

                                let gpu_backend = crate::gpu::get_backend();
                                match gpu_backend.execute_lowered_block(&**value, &context) {
                                    Ok(Some(val)) => {
                                        tracing::debug!("[EXECUTOR]: Expression Block evaluated on GPU via PTX JIT path. Result: {}", val);
                                        let result_str = val.to_string();
                                        self.memory.set_var_native(name.clone(), HelheimType::parse(&result_str));
                                        if ctx.is_distributed { self.distributed.set_global(name.as_str(), result_str.clone()); }
                                        result_str
                                    }
                                    _ => {
                                        // Fallback to CPU interpreter
                                        if let Some(ret) = Box::pin(self.execute_ast(vec![(**value).clone()], ctx.clone())).await.unwrap_or(None) {
                                            ret
                                        } else {
                                            "".to_string()
                                        }
                                    }
                                }
                            }
                            CodeTaal::FunctionCall { .. } | CodeTaal::QualifiedCall { .. } => {
                                self.evaluate_ast_expr(&**value, ctx.clone()).await?
                            }
                            CodeTaal::Perform { effect, operation, args } => {
                                let full_effect = if let Some(ns) = &ctx.current_module {
                                    if effect.contains("::") { effect.clone() } else { format!("{}::{}", ns, effect) }
                                } else {
                                    effect.clone()
                                };
                                let handler_var = format!("__effect_handler_{}_{}", full_effect, operation);
                                if let Some(HelheimType::String(ast_str)) = self.memory.get_var_native(&handler_var) {
                                    if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                                        let continuation = crate::orchestra::continuation::capture_continuation(&stmt, &self.memory, effect, &self.distributed, ctx.is_privileged)?;
                                        let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                        let json_str = serde_json::to_string(&continuation).map_err(|e| anyhow::anyhow!("Continuation serialization fail: {}", e))?;
                                        let b64_str = base64::engine::general_purpose::STANDARD.encode(json_str);
                                        self.memory.set_var_native("resume_k".to_string(), HelheimType::String(b64_str));
                                        for (idx, arg) in args.iter().enumerate() {
                                            if let Ok(arg_val) = self.evaluate_ast_expr(arg, ctx.clone()).await {
                                                self.memory.set_var_native(format!("arg{}", idx + 1), HelheimType::parse(&arg_val));
                                            }
                                        }
                                        let res = self.execute_ast(vec![handler_ast], ctx.clone()).await;
                                        // STOP outer loop since resume handles the rest!
                                        return res;
                                    }
                                }
                                "".to_string()
                            }
                            CodeTaal::QualifiedPerform { ns, effect, operation, args } => {
                                let full_effect = format!("{}::{}", ns, effect);
                                let handler_var = format!("__effect_handler_{}_{}", full_effect, operation);
                                if let Some(HelheimType::String(ast_str)) = self.memory.get_var_native(&handler_var) {
                                    if let Ok(handler_ast) = serde_json::from_str::<CodeTaal>(&ast_str) {
                                        let continuation = crate::orchestra::continuation::capture_continuation(&stmt, &self.memory, effect, &self.distributed, ctx.is_privileged)?;
                                        let _guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);
                                        let json_str = serde_json::to_string(&continuation).map_err(|e| anyhow::anyhow!("Continuation serialization fail: {}", e))?;
                                        let b64_str = base64::engine::general_purpose::STANDARD.encode(json_str);
                                        self.memory.set_var_native("resume_k".to_string(), HelheimType::String(b64_str));
                                        for (idx, arg) in args.iter().enumerate() {
                                            if let Ok(arg_val) = self.evaluate_ast_expr(arg, ctx.clone()).await {
                                                self.memory.set_var_native(format!("arg{}", idx + 1), HelheimType::parse(&arg_val));
                                            }
                                        }
                                        let res = self.execute_ast(vec![handler_ast], ctx.clone()).await;
                                        // STOP outer loop since resume handles the rest!
                                        return res;
                                    }
                                }
                                "".to_string()
                            }
                            _ => {
                                self.evaluate_ast_expr(&**value, ctx.clone()).await?
                            }
                        };
                        
                        let mut evaluated_value = value_str.clone();
                        let clean_val = evaluated_value.trim();
                        if clean_val.starts_with("roep_aan ") || clean_val.starts_with("invoke ") {
                            let parts = crate::orchestra::parser::HelParser::tokenize(clean_val);
                            if parts.len() >= 2 {
                                let func_name = parts[1].value.clone();
                                let mut args = Vec::new();
                                if parts.len() > 2 {
                                    args = parts[2..].iter().map(|t| t.value.clone()).collect();
                                }
                                evaluated_value = self.execute_function_call("", &func_name, args, ctx.clone()).await?;
                            }
                        } else if let Some(prompt) = ["vraag ", "ask ", "prompt ", "input "]
                            .iter().find_map(|&pfx| clean_val.strip_prefix(pfx)) {
                            let prompt = prompt.trim().trim_matches('"');
                            let resolved_prompt = self.memory.resolve_value(prompt);
                            use std::io::Write;
                            print!("{} ", resolved_prompt);
                            std::io::stdout().flush().unwrap_or(());
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input).unwrap_or(0);
                            evaluated_value = input.trim().to_string();
                        } else if let Some(path) = clean_val.strip_prefix("lees ") {
                            let path = path.trim().trim_matches('"');
                            let resolved_path = self.memory.resolve_value(path);
                            match tokio::fs::read_to_string(&resolved_path).await {
                                Ok(content) => evaluated_value = content,
                                Err(e) => {
                                    tracing::error!("[ERROR]: Kan bestand '{}' niet lezen: {}", resolved_path, e);
                                    evaluated_value = "".to_string();
                                }
                            }
                        } else {
                            evaluated_value = self.evaluate_expression(&value_str);
                        }
                        let evaluated_value = self.memory.resolve_value(&evaluated_value);
                        tracing::debug!("[MEM]: {} = {}", name, evaluated_value);
                        self.memory.set_var_native(name.clone(), HelheimType::parse(&evaluated_value));
                        if ctx.is_distributed { self.distributed.set_global(&name, evaluated_value.clone()); }
                    }
                    CodeTaal::VarGet { name } => {
                        if let Some(val) = self.memory.get_var_native(&name) {
                            tracing::debug!("[VAL]: {} = {}", name, val);
                        } else {
                            tracing::error!("[ERR]: Variabele '{}' niet gevonden.", name);
                        }
                    }
                    CodeTaal::Loop { condition, body } => {
                        if let Err(e) = ctx.check_timeout() {
                            return Err(e);
                        }
                        let should_run = self.evaluate_ast_condition(&condition, ctx.clone()).await?;
                        if should_run {
                            let mut rest = statements.split_off(pc - 1);
                            rest[0] = CodeTaal::Loop { condition: condition.clone(), body: body.clone() };
                            stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                                statements: rest,
                                pc: 0, // Loop back to evaluate condition again
                                ctx: ctx.clone(),
                            })?;
                            match *body {
                                CodeTaal::Block { statements: inner } => {
                                    stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                                        statements: inner,
                                        pc: 0,
                                        ctx: ctx.clone(),
                                    })?;
                                }
                                other => {
                                    stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                                        statements: vec![other],
                                        pc: 0,
                                        ctx: ctx.clone(),
                                    })?;
                                }
                            }
                            break_inner = true;
                        }
                    }
                    CodeTaal::ForEach {
                        iterator,
                        iterable,
                        body,
                    } => {
                        let json_val = self.evaluate_ast_expr(&iterable, ctx.clone()).await?;
                        let mut clone_statements = Vec::new();
                        if let CodeTaal::Block { statements } = *body {
                            clone_statements = statements;
                        }

                        // Try parsing JSON list
                        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&json_val) {
                            tracing::debug!(
                                "[LOOP]: 'voor elke' geactiveerd met {} iteraties over '{:?}'.",
                                arr.len(),
                                iterable
                            );
                            for v in arr {
                                if let Err(e) = ctx.check_timeout() {
                                    return Err(e);
                                }
                                let item_str = if let Some(s) = v.as_str() {
                                    s.to_string()
                                } else {
                                    v.to_string()
                                };
                                self.memory.set_var_native(iterator.clone(), HelheimType::parse(&item_str));
                                if ctx.is_distributed { self.distributed.set_global(&iterator, item_str.clone()); }
                                // Use propagate helper for return from for-each body
                                let body_block = CodeTaal::Block { statements: clone_statements.clone() };
                                if let Some(ret) = self.propagate_return(&body_block, ctx.clone()).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        } else {
                            tracing::debug!(
                                "[ERROR]: Kan '{:?}' niet itereren. Waarde is geen geldige JSON-lijst.",
                                iterable
                            );
                        }
                    }
                    CodeTaal::If {
                        condition,
                        then,
                        else_block,
                    } => {
                        stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                            statements: statements.split_off(pc),
                            pc: 0,
                            ctx: ctx.clone(),
                        })?;
                        if self.evaluate_ast_condition(&condition, ctx.clone()).await? {
                            stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                                statements: vec![*then],
                                pc: 0,
                                ctx: ctx.clone(),
                            })?;
                        } else if let Some(else_b) = else_block {
                            stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                                statements: vec![*else_b],
                                pc: 0,
                                ctx: ctx.clone(),
                            })?;
                        }
                        break_inner = true;
                    }
                    CodeTaal::ArrayPush { array_name, value } => {
                        let args = vec![array_name.clone(), value.clone()];
                        let _ = self.execute_function_call("", "voeg_toe", args, ctx.clone()).await?;
                    }
                    CodeTaal::ArrayRemove { array_name, index } => {
                        let args = vec![array_name.clone(), index.clone()];
                        let _ = self.execute_function_call("", "verwijder", args, ctx.clone()).await?;
                    }
                    CodeTaal::Concurrent { statements } => {
                        tracing::debug!(
                            "[AST]: Activeren van distributed parallelle uitvoering ({} taken)...",
                            statements.len()
                        );

                        let peers = match self.discovery.peers.lock() {
                            Ok(p) => p.clone(),
                            Err(_) => std::collections::HashMap::new(),
                        };

                        if peers.is_empty() {
                            // Lokale fallback (exact zoals huidige implementatie)
                            let mut futures_list = Vec::new();
                            for concurrent_stmt in statements {
                                let mut dist_ctx = ctx.clone();
                                dist_ctx.is_distributed = true;
                                futures_list.push(self.execute_ast(vec![concurrent_stmt.clone()], dist_ctx));
                            }
                            let results = futures::future::join_all(futures_list).await;
                            for res in results {
                                if let Err(e) = res {
                                    tracing::error!("[ERROR]: Fout in parallelle taak: {}", e);
                                }
                            }
                            continue;
                        }

                        // Distributed pad
                        let executor = self.clone();
                        let mut dispatch_futures = Vec::new();

                        for stmt in statements {
                            let stmt = stmt.clone();
                            let ctx = ctx.clone();
                            let exec = executor.clone();

                            dispatch_futures.push(async move {
                                if let Some((ip, port)) = exec.schedule_statement(&stmt) {
                                    let json = match serde_json::to_string(&vec![stmt.clone()]) {
                                        Ok(j) => j,
                                        Err(e) => {
                                            tracing::error!("[SWARM SERIALIZE ERROR]: {}", e);
                                            return exec.execute_ast(vec![stmt], ctx).await.map(|_| ());
                                        }
                                    };
                                    let cmd = format!("ast_json:{}", json);

                                    match crate::network::hsp_node::SwarmEngine::dispatch(&ip, port, &cmd).await {
                                        Ok(res) => {
                                            tracing::debug!("[SWARM]: Result from {}: {}", ip, res);
                                            Ok(())
                                        }
                                        Err(e) => {
                                            tracing::debug!("[SWARM ERROR]: Dispatch failed ({}), fallback local", e);
                                            exec.execute_ast(vec![stmt], ctx).await.map(|_| ())
                                        }
                                    }
                                } else {
                                    exec.execute_ast(vec![stmt], ctx).await.map(|_| ())
                                }
                            });
                        }

                        let results = futures::future::join_all(dispatch_futures).await;
                        for res in results {
                            if let Err(e) = res {
                                tracing::error!("[ERROR]: Distributed taak error: {}", e);
                            }
                        }

                        // Flush deltas
                        let deltas = self.distributed.flush_deltas();
                        for delta in deltas {
                            if let Ok(json) = serde_json::to_string(&delta) {
                                let cmd = format!("state_delta:{}", json);
                                let peers = match self.discovery.peers.lock() {
                                    Ok(p) => p.clone(),
                                    Err(_) => std::collections::HashMap::new(),
                                };
                                for ip in peers.keys() {
                                    let _ = crate::network::hsp_node::SwarmEngine::dispatch(ip, 9003, &cmd).await;
                                }
                            }
                        }
                    }
                    CodeTaal::Block { statements: _ } => {
                        // Context binding: resolve host variables for lowered PTX block
                        let context = self.build_eval_context(&stmt);

                        let gpu_backend = crate::gpu::get_backend();
                        match gpu_backend.execute_lowered_block(&stmt, &context) {
                            Ok(Some(val)) => {
                                tracing::debug!("[EXECUTOR]: Lowered block executed on real GPU via PTX JIT path. Return: {}", val);
                                // Tensor unpacking for direct block return
                                let mask = val.to_bits() as u32;
                                let mut bool_list = vec![];
                                for i in 0..32 {
                                    let b = (mask & (1u32 << i)) != 0;
                                    bool_list.push(if b { "waar" } else { "onwaar" });
                                }
                                let unpacked = format!("[{}]", bool_list.join(", "));
                                return Ok(Some(unpacked));
                            }
                            Ok(None) => {
                                tracing::debug!("[EXECUTOR]: Lowered block executed on real GPU via PTX JIT path. No return value.");
                            }
                            Err(e) => {
                                tracing::debug!("[EXECUTOR]: GPU lowered launch not taken ({}), falling back to interpreter", e);
                                if let CodeTaal::Block { statements: inner_statements } = stmt {
                                    stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                                        statements: statements.split_off(pc),
                                        pc: 0,
                                        ctx: ctx.clone(),
                                    })?;
                                    stack.push(crate::orchestra::trampoline::EvalFrame::Statements {
                                        statements: inner_statements,
                                        pc: 0,
                                        ctx: ctx.clone(),
                                    })?;
                                    break_inner = true;
                                }
                            }
                        }
                    }
                    CodeTaal::Daemon { body } => {
                        tracing::debug!("[AST]: Achtergrond (Daemon) proces gestart...");
                        let engine_clone = self.clone();
                        let body_clone = body.clone();
                        let ctx_clone = ctx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = engine_clone.execute_ast(vec![*body_clone], ctx_clone).await {
                                tracing::error!("[ERROR]: Fout in daemon proces: {}", e);
                            }
                        });
                    }

                    // === ACTOR / MESSAGE-PASSING (Vraag 2) - direct integrable ===
                    CodeTaal::Spawn { name, body } => {
                        let registry = self.actor_registry.clone();
                        let memory = self.memory.clone();
                        let ctx_clone = ctx.clone();
                        let body_for_task = (*body).clone();
                        let executor_for_actor = Arc::new(self.clone()); // pass self for evaluate_actor_body

                        let parent_id = if let Some(crate::orchestra::memory::HelheimType::Int(id)) = memory.get_var_native("self") {
                            Some(id as u64)
                        } else {
                            None
                        };
                        tracing::debug!("Setting parent_id={:?} for new actor", parent_id);

                        let strategy = match memory.get_var("__supervision_strategy").as_deref() {
                            Some("\"Escalate\"") | Some("Escalate") => crate::orchestra::actor::SupervisionStrategy::Escalate,
                            Some("\"Restart\"") | Some("Restart") => crate::orchestra::actor::SupervisionStrategy::Restart,
                            _ => crate::orchestra::actor::SupervisionStrategy::Stop,
                        };

                        let metadata = crate::orchestra::actor::ActorMetadata {
                            parent_id,
                            strategy,
                        };

                        let (handle, rx) = registry.register(name.clone(), metadata);

                        // Spawn isolated actor task with its own mailbox (MPSC)
                        tokio::spawn(async move {
                            crate::orchestra::actor::run_actor(
                                handle.id,
                                rx,
                                body_for_task,
                                registry,
                                memory,
                                ctx_clone,
                                executor_for_actor,
                                handle.metadata.clone(),
                            ).await;
                        });

                        tracing::debug!("[ACTOR] Spawned isolated actor id={} (name={:?})", handle.id, name);
                    }

                    CodeTaal::SendMessage { target, message } => {
                        // Evaluate target (can be ID literal, name string, or expression)
                        let target_val = self.evaluate_ast_expr(&target, ctx.clone()).await?;
                        let target_str = target_val.to_string();

                        let msg_val = self.evaluate_ast_expr(&message, ctx.clone()).await?;

                        // Fast local path (HelValueWrapper::Local) - zero copy via channel
                        // Remote detection can use discovery or explicit syntax later
                        if let Err(e) = crate::orchestra::actor::send_message(
                            &self.actor_registry,
                            &self.distributed,
                            &target_str,
                            crate::orchestra::memory::HelheimType::parse(&msg_val),
                            !self.actor_registry.is_local(&target_str),
                        ).await {
                            tracing::error!("[ACTOR] SendMessage failed to {}: {}", target_str, e);
                        }
                    }

                    CodeTaal::Receive { var: _, timeout: _, body: _ } => {
                        // Receive is meant to be executed inside a dedicated actor task (see actor.rs run_actor).
                        tracing::warn!("[ACTOR] Receive in main executor context - dit is ongeldig en wordt gestopt.");
                        return Err(anyhow::anyhow!("'ontvang' (Receive) statement is alleen geldig binnen een gespawnde acteur."));
                    }


                    // === INLINE PTX / ASM (Vraag 1) - zero-overhead lowering ===
                    CodeTaal::InlineAssembly {
                        target,
                        code,
                        inputs,
                        outputs,
                        clobbers: _,
                        fallback,
                    } => {
                        if !ctx.is_privileged {
                            return Err(anyhow::anyhow!(
                                "[SECURITY]: Inline assembly (ptx/asm) requires privileged ExecutionContext."
                            ));
                        }

                        tracing::debug!(
                            "[INLINE-ASM]: Lowering {} block ({} bytes source)",
                            target,
                            code.len()
                        );

                        if target == "ptx" || target == "asm" {
                            // Zero-overhead path: treat as lowered block.
                            // Build a small context from inputs using existing HelheimType bridge.
                            let mut asm_context: std::collections::HashMap<String, helheim_lang::ast::LiteralValue> =
                                std::collections::HashMap::new();

                            for (param_name, expr) in inputs {
                                let val_str = self.evaluate_ast_expr(&expr, ctx.clone()).await?;
                                // Bridge HelheimType -> LiteralValue voor de generator / PTX paramsator
                                let lit = match HelheimType::parse(&val_str) {
                                    HelheimType::Int(i) => helheim_lang::ast::LiteralValue::Int(i),
                                    HelheimType::Float(f) => helheim_lang::ast::LiteralValue::Float(f),
                                    HelheimType::String(s) => helheim_lang::ast::LiteralValue::String(s),
                                    HelheimType::Bool(b) => helheim_lang::ast::LiteralValue::Bool(b),
                                    HelheimType::Bytes(b) => {
                                        // For PTX we can pass as list or special; simple fallback
                                        helheim_lang::ast::LiteralValue::List(
                                            b.into_iter()
                                                .map(|v| helheim_lang::ast::LiteralValue::Int(v as i64))
                                                .collect(),
                                        )
                                    }
                                    _ => helheim_lang::ast::LiteralValue::String(val_str),
                                };
                                asm_context.insert(param_name.clone(), lit);
                            }

                            // Use existing synthesis path for lowering (zero-overhead reuse)
                            let lowered_block = CodeTaal::Block {
                                statements: vec![CodeTaal::HelBlock { raw_code: code.clone() }],
                            };

                            // Call into GeneralPtxGenerator via synthesis (or direct PTX backend)
                            let ptx_result = match crate::orchestra::synthesis::KernelSynthesisEngine::synthesize_lowered_with_context(
                                lowered_block,
                                &asm_context,
                            ) {
                                Ok(ptx) => ptx,
                                Err(e) => {
                                    // Fallback: direct NVRTC path if synthesis doesn't like the raw block
                                    tracing::debug!("[INLINE-ASM] Synthesis fallback for raw PTX: {}", e);
                                    code.clone() // use source directly if it's already valid PTX
                                }
                            };

                            // Launch via existing GPU backend (PTX JIT path) - zero extra overhead
                            let backend = crate::gpu::get_backend();
                            if let Ok(compiled) = backend.compile(&helheim_lang::ast::GpuKernelDef {
                                name: "inline_asm_kernel".to_string(),
                                attributes: vec![],
                                params: vec![],
                                body: Box::new(CodeTaal::HelBlock { raw_code: ptx_result.clone() }),
                            }) {
                                // Launch with empty args for now; real inputs come via .param in the PTX source
                                // or via the context mechanism already handled above.
                                if let Err(e) = backend.launch(&compiled, &[]) {
                                    tracing::error!("[INLINE-ASM] Launch failed: {}", e);
                                } else {
                                    tracing::debug!("[INLINE-ASM] PTX kernel launched successfully");
                                    // Write outputs back to memory using the existing resolve/set mechanism
                                    for out_name in outputs {
                                        // In real PTX the kernel would have written to .global or .param output slots.
                                        // For this sketch we simulate by resolving from a convention (e.g. last computed value).
                                        let out_val = self.memory.resolve_value(&out_name);
                                        if !out_val.is_empty() {
                                            self.memory.set_var_native(out_name.clone(), HelheimType::parse(&out_val));
                                        }
                                    }
                                }
                            } else {
                                // Pure CPU fallback or error
                                if let Some(fb) = fallback {
                                    tracing::debug!("[INLINE-ASM] Geen GPU backend gevonden. Uitvoeren van CPU fallback-blok...");
                                    self.execute_ast_internal(vec![*fb], ctx.clone()).await?;
                                } else {
                                    tracing::debug!("[INLINE-ASM] No GPU backend and no fallback provided - treating as no-op for safety");
                                }
                            }
                        } else {
                            // Future: x86 via dynasm-rs or Cranelift in a privileged native lowering step.
                            // For now reject or run fallback.
                            if let Some(fb) = fallback {
                                tracing::debug!("[INLINE-ASM] Target '{}' niet ondersteund. Uitvoeren van CPU fallback-blok...", target);
                                self.execute_ast_internal(vec![*fb], ctx.clone()).await?;
                            } else {
                                return Err(anyhow::anyhow!(
                                    "[INLINE-ASM] Only 'ptx' target supported in this build for memory-safety, and no fallback was provided."
                                ));
                            }
                        }
                    }

                    CodeTaal::HelBlock { raw_code: _ } => {
                        if !ctx.is_privileged {
                            return Err(anyhow::anyhow!("[SECURITY]: Native Hel-blocks vereisen Elevated Privileges."));
                        }
                        #[cfg(feature = "cuda")]
                        {
                            crate::gpu::gpu_execute_hel_block(&raw_code).await?;
                        }
                        #[cfg(not(feature = "cuda"))]
                        {
                            return Err(anyhow::anyhow!("Hel-block execution requires 'cuda' feature"));
                        }
                    }
                    CodeTaal::TryCatch {
                        try_block,
                        catch_block,
                        error_var,
                    } => {
                        let statements = if let CodeTaal::Block { statements } = *try_block.clone()
                        {
                            statements
                        } else {
                            Vec::new()
                        };
                        match self.execute_ast(statements, ctx.clone()).await {
                            Ok(Some(ret)) => return Ok(Some(ret)),
                            Ok(None) => {}
                            Err(e) => {
                                tracing::debug!("[VANG]: Fout afgevangen: {}", e);
                                if let Some(err_name) = error_var {
                                    let mut err_str = e.to_string();
                                    let line = self.memory.get_var_native("__LAST_ERR_LINE__").map(|v| v.to_string()).unwrap_or_default();
                                    let col = self.memory.get_var_native("__LAST_ERR_COL__").map(|v| v.to_string()).unwrap_or_default();
                                    if !line.is_empty() && !col.is_empty() && !err_str.contains("[Fout op ") {
                                        err_str = format!("[Fout op regel {}:{}] {}", line, col, err_str);
                                    }
                                    self.memory.set_var_native(err_name.clone(), HelheimType::String(err_str.clone()));
                                    if ctx.is_distributed { self.distributed.set_global(&err_name, err_str); }
                                }
                                // Propagate return from catch block as well
                                if let Some(ret) = self.propagate_return(&catch_block, ctx.clone()).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        }
                    }
                    CodeTaal::Send { target, payload } => {
                        let clean_payload = payload.trim().trim_matches('"');

                        // 1. String Interpolation (Basic: check for $vars)
                        let mut final_payload = clean_payload.to_string();
                        if final_payload.contains('$') {
                            let store = self.memory.local_stack.lock().unwrap_or_else(|e| e.into_inner());
                            for scope in store.iter().rev() {
                                for (k, v) in scope.iter() {
                                    let key = format!("${}", k);
                                    if final_payload.contains(&key) {
                                        let val_str = match v {
                                            HelheimType::String(s) => s.clone(),
                                            _ => v.to_string(),
                                        };
                                        final_payload = final_payload.replace(&key, &val_str);
                                    }
                                }
                            }
                            for entry in self.memory.globals.iter() {
                                let key = format!("${}", entry.key());
                                if final_payload.contains(&key) {
                                    let val_str = match entry.value() {
                                        HelheimType::String(s) => s.clone(),
                                        _ => entry.value().to_string(),
                                    };
                                    final_payload = final_payload.replace(&key, &val_str);
                                }
                            }
                        }

                        tracing::debug!("[AST]: Sturen naar '{}': {}", target, final_payload);

                        // 2. Broadcast Logic
                        let mut final_targets = Vec::new();
                        if target == "allemaal" || target == "all" {
                            if let Ok(peers) = self.discovery.peers.lock() {
                                for ip in peers.keys() {
                                    final_targets.push(ip.clone());
                                }
                            }
                        } else {
                            final_targets.push(target.clone());
                        }

                        // 3. Dispatch
                        for t in final_targets {
                            let _ = crate::network::hsp_node::SwarmEngine::dispatch(
                                &t,
                                9003,
                                &final_payload,
                            )
                            .await;
                        }
                    }
                    CodeTaal::SysOp { command } => {
                        let cmd_trim = command.trim();
                        if cmd_trim.starts_with("installeer_ondertekend ") || cmd_trim.starts_with("import_signed ") {
                            // Vraag 4: Package Manager + Signing (zero-overhead after verify)
                            let parts: Vec<&str> = cmd_trim.split_whitespace().collect();
                            if parts.len() < 3 {
                                return Err(anyhow::anyhow!("Usage: installeer_ondertekend <name> <source> [base64_signature]"));
                            }
                            let name = parts[1].to_string();
                            let source = parts[2].to_string();
                            let sig = if parts.len() > 3 {
                                use base64::Engine;
                                use anyhow::Context;
                                Some(base64::engine::general_purpose::STANDARD.decode(parts[3]).context("Invalid base64 signature")? )
                            } else { None };

                            // We assume PackageManager is available on the executor (add the field in struct + new())
                            let pm = &self.package_manager;
                            match pm.import_signed(&name, &source, sig.as_deref(), &self.distributed).await {
                                Ok(verified) => {
                                    tracing::debug!("[PACKAGE] Successfully verified and imported '{}' v{}", verified.name, verified.version);
                                }
                                Err(e) => return Err(e),
                            }
                            return Ok(None);
                        }

                        // We willen ELK WOORD afzonderlijk resolven, voor het geval 
                        // de gebruiker 'voer uit echo NAAM' typt zonder $ of {}
                        let mut resolved_parts = Vec::new();
                        for part in command.split_whitespace() {
                            let resolved = self.memory.resolve_value(part);
                            // Strip quotes to ensure clean bash arguments
                            let clean = resolved.trim_matches('"');
                            resolved_parts.push(clean.to_string());
                        }
                        let resolved_command = resolved_parts.join(" ");
                        
                        let mut args = vec![];
                        // If it starts with "voer uit ", strip it for native shell execution.
                        // Otherwise pass the whole command string to "systeem.shell"
                        if let Some(cmd) = ["voer uit ", "execute ", "run "]
                            .iter().find_map(|&pfx| resolved_command.strip_prefix(pfx)) {
                            args.push(cmd.to_string());
                        } else {
                            args.push(resolved_command);
                        }
                        if let Some(output) = system::SystemManager::try_execute_native(&self.memory, "systeem.shell", &args, &ctx).await? {
                            if !output.is_empty() {
                                tracing::info!("{}", output);
                            }
                        }
                        // Recursively call process_command for legacy support
                        // Note: process_command is async, so we await it.
                        
                    }
                    CodeTaal::TcpListen { addr } => {
                        if !ctx.is_privileged {
                            tracing::debug!("[SECURITY]: tcp_luister vereist elevated privileges.");
                            continue;
                        }
                        let addr_str = self.evaluate_ast_expr(&addr, ctx.clone()).await?;
                        let addr_str = addr_str.trim_matches('"').to_string();
                        match tokio::net::TcpListener::bind(&addr_str).await {
                            Ok(listener) => {
                                let id = crate::orchestra::tcp_resources::next_handle_id();
                                crate::orchestra::tcp_resources::RESOURCE_TABLE.insert(
                                    id,
                                    crate::orchestra::tcp_resources::Resource::TcpListener(
                                        std::sync::Arc::new(tokio::sync::Mutex::new(listener))
                                    )
                                );
                                tracing::debug!("[TCP LISTEN]: Luistert op {} (handle: handle(tcp_listener:{}))", addr_str, id);
                                self.memory.set_var_native("__last_tcp_listener".to_string(),
                                    HelheimType::ResourceHandle { kind: "tcp_listener".to_string(), id });
                            }
                            Err(e) => tracing::debug!("[TCP ERROR]: Luisteren op {} mislukt: {}", addr_str, e),
                        }
                    }
                    CodeTaal::TcpAccept { listener } => {
                        if !ctx.is_privileged {
                            tracing::debug!("[SECURITY]: tcp_accepteer vereist elevated privileges.");
                            continue;
                        }
                        let handle_str = self.evaluate_ast_expr(&listener, ctx.clone()).await?;
                        if let Some(id_str) = handle_str.strip_prefix("handle(tcp_listener:") {
                            if let Ok(id) = id_str.trim_end_matches(')').parse::<u64>() {
                                if let Some(res) = crate::orchestra::tcp_resources::RESOURCE_TABLE.get(&id) {
                                    if let crate::orchestra::tcp_resources::Resource::TcpListener(listener_arc) = res.value() {
                                        let listener_lock = listener_arc.lock().await;
                                        match listener_lock.accept().await {
                                            Ok((stream, peer_addr)) => {
                                                let new_id = crate::orchestra::tcp_resources::next_handle_id();
                                                crate::orchestra::tcp_resources::RESOURCE_TABLE.insert(
                                                    new_id,
                                                    crate::orchestra::tcp_resources::Resource::TcpStream(
                                                        std::sync::Arc::new(tokio::sync::Mutex::new(stream))
                                                    )
                                                );
                                                tracing::debug!("[TCP ACCEPT]: Verbonden met {} (handle: handle(tcp:{}))", peer_addr, new_id);
                                                self.memory.set_var_native("__last_tcp_stream".to_string(),
                                                    HelheimType::ResourceHandle { kind: "tcp".to_string(), id: new_id });
                                            }
                                            Err(e) => tracing::debug!("[TCP ERROR]: Accepteren mislukt: {}", e),
                                        }
                                    } else {
                                        tracing::debug!("[TCP ERROR]: Handle is geen tcp_listener.");
                                    }
                                } else {
                                    tracing::debug!("[TCP ERROR]: Listener handle {} niet gevonden.", id);
                                }
                            }
                        } else {
                            tracing::debug!("[TCP ERROR]: Ongeldige listener handle.");
                        }
                    }
                    CodeTaal::TcpConnect { addr } => {
                        if !ctx.is_privileged {
                            tracing::debug!("[SECURITY]: tcp_verbind vereist elevated privileges.");
                            continue;
                        }
                        let addr_str = self.evaluate_ast_expr(&addr, ctx.clone()).await?;
                        let addr_str = addr_str.trim_matches('"').to_string();
                        match tokio::net::TcpStream::connect(&addr_str).await {
                            Ok(stream) => {
                                let id = crate::orchestra::tcp_resources::next_handle_id();
                                crate::orchestra::tcp_resources::RESOURCE_TABLE.insert(
                                    id,
                                    crate::orchestra::tcp_resources::Resource::TcpStream(
                                        std::sync::Arc::new(tokio::sync::Mutex::new(stream))
                                    )
                                );
                                tracing::debug!("[TCP CONNECT]: Verbonden met {} (handle: handle(tcp:{}))", addr_str, id);
                                self.memory.set_var_native("__last_tcp_stream".to_string(),
                                    HelheimType::ResourceHandle { kind: "tcp".to_string(), id });
                            }
                            Err(e) => tracing::debug!("[TCP ERROR]: Verbinden met {} mislukt: {}", addr_str, e),
                        }
                    }
                    CodeTaal::TcpSend { socket, data } => {
                        if !ctx.is_privileged {
                            tracing::debug!("[SECURITY]: tcp_stuur vereist elevated privileges.");
                            continue;
                        }
                        let handle_str = self.evaluate_ast_expr(&socket, ctx.clone()).await?;
                        let data_val = self.evaluate_ast_expr(&data, ctx.clone()).await?;
                        let data_bytes = data_val.trim_matches('"').as_bytes().to_vec();

                        let id: Option<u64> = handle_str.trim_matches('"')
                            .strip_prefix("handle(tcp:")
                            .and_then(|s| s.strip_suffix(')'))
                            .and_then(|s| s.parse().ok());

                        if let Some(id) = id {
                            if let Some(res) = crate::orchestra::tcp_resources::RESOURCE_TABLE.get(&id) {
                                match res.value() {
                                    crate::orchestra::tcp_resources::Resource::TcpStream(stream) => {
                                        use tokio::io::AsyncWriteExt;
                                        let mut guard = stream.lock().await;
                                        match guard.write_all(&data_bytes).await {
                                            Ok(_) => tracing::debug!("[TCP SEND]: {} bytes → handle tcp:{}", data_bytes.len(), id),
                                            Err(e) => {
                                                tracing::debug!("[TCP ERROR]: Schrijven mislukt: {}. Handle gesloten.", e);
                                                drop(guard);
                                                crate::orchestra::tcp_resources::RESOURCE_TABLE.remove(&id);
                                            }
                                        }
                                    }
                                    _ => tracing::debug!("[TCP ERROR]: Handle {} is geen stream.", id),
                                }
                            } else {
                                tracing::debug!("[TCP ERROR]: Onbekend handle: handle(tcp:{})", id);
                            }
                        } else {
                            tracing::debug!("[TCP ERROR]: Ongeldig handle formaat: '{}'", handle_str);
                        }
                    }
                    CodeTaal::TcpReceive { socket, max_bytes } => {
                        if !ctx.is_privileged {
                            tracing::debug!("[SECURITY]: tcp_ontvang vereist elevated privileges.");
                            continue;
                        }
                        let handle_str = self.evaluate_ast_expr(&socket, ctx.clone()).await?;
                        let max = if let Some(mb) = max_bytes {
                            self.evaluate_ast_expr(&mb, ctx.clone()).await?
                                .parse::<usize>()
                                .unwrap_or(4096)
                        } else { 4096 };

                        let id: Option<u64> = handle_str.trim_matches('"')
                            .strip_prefix("handle(tcp:")
                            .and_then(|s| s.strip_suffix(')'))
                            .and_then(|s| s.parse().ok());

                        if let Some(id) = id {
                            if let Some(res) = crate::orchestra::tcp_resources::RESOURCE_TABLE.get(&id) {
                                match res.value() {
                                    crate::orchestra::tcp_resources::Resource::TcpStream(stream) => {
                                        use tokio::io::AsyncReadExt;
                                        let mut guard = stream.lock().await;
                                        let mut buf = vec![0u8; max];
                                        match guard.read(&mut buf).await {
                                            Ok(0) => {
                                                tracing::debug!("[TCP RECV]: Peer heeft verbinding gesloten. Handle tcp:{} vrijgegeven.", id);
                                                drop(guard);
                                                crate::orchestra::tcp_resources::RESOURCE_TABLE.remove(&id);
                                            }
                                            Ok(n) => {
                                                let received = buf[..n].to_vec();
                                                let as_str = String::from_utf8_lossy(&received).to_string();
                                                tracing::debug!("[TCP RECV]: {} bytes ← handle tcp:{}", n, id);
                                                self.memory.set_var_native("__last_tcp_recv".to_string(),
                                                    HelheimType::Bytes(received));
                                                self.memory.set_var_native("__last_tcp_recv_str".to_string(),
                                                    HelheimType::String(as_str));
                                            }
                                            Err(e) => {
                                                tracing::debug!("[TCP ERROR]: Lezen mislukt: {}. Handle gesloten.", e);
                                                drop(guard);
                                                crate::orchestra::tcp_resources::RESOURCE_TABLE.remove(&id);
                                            }
                                        }
                                    }
                                    _ => tracing::debug!("[TCP ERROR]: Handle {} is geen stream.", id),
                                }
                            } else {
                                tracing::debug!("[TCP ERROR]: Onbekend handle: handle(tcp:{})", id);
                            }
                        } else {
                            tracing::debug!("[TCP ERROR]: Ongeldig handle formaat: '{}'", handle_str);
                        }
                    }
                    CodeTaal::TcpClose { socket } => {
                        if !ctx.is_privileged {
                            tracing::debug!("[SECURITY]: tcp_sluit vereist elevated privileges.");
                            continue;
                        }
                        let handle_str = self.evaluate_ast_expr(&socket, ctx.clone()).await?;
                        let id: Option<u64> = handle_str.trim_matches('"')
                            .strip_prefix("handle(tcp:")
                            .and_then(|s| s.strip_suffix(')'))
                            .and_then(|s| s.parse().ok());
                        if let Some(id) = id {
                            if crate::orchestra::tcp_resources::RESOURCE_TABLE.remove(&id).is_some() {
                                tracing::debug!("[TCP CLOSE]: Handle tcp:{} gesloten.", id);
                            } else {
                                tracing::debug!("[TCP ERROR]: Onbekend of al gesloten handle: tcp:{}", id);
                            }
                        } else {
                            tracing::debug!("[TCP ERROR]: Ongeldig handle formaat: '{}'", handle_str);
                        }
                    }
                    _ => tracing::debug!("[AST]: Instructie nog niet geïmplementeerd: {:?}", stmt),
                }
                }
            }
            Ok(None)
        })
    }

    /// Compact helper for Return propagation within nested scopes.
    /// Any Return (retourneer/geef_terug/return) deep inside als/zolang/try etc. makes
    /// execute_ast return Ok(Some(value)). This helper + early returns in control arms
    /// ensure the function call stack is aborted immediately and we return the value
    /// to the original caller, while the function wrapper guarantees pop_scope on every path.
    pub async fn evaluate_tcp_primitive(&self, operation: &str, stmt: &CodeTaal, _memory: &MemoryManager, ctx: crate::common::context::ExecutionContext) -> Result<String> {
        let args = match stmt {
            CodeTaal::Perform { args, .. } => args,
            _ => return Err(anyhow::anyhow!("Expected Perform node for evaluate_tcp_primitive")),
        };

        match operation {
            "verbind" => {
                if !ctx.is_privileged { return Err(anyhow::anyhow!("Elevated privileges required for TCP.")); }
                if args.is_empty() { return Err(anyhow::anyhow!("Missing address for tcp verbind")); }
                let addr_str = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                
                match tokio::net::TcpStream::connect(&addr_str).await {
                    Ok(stream) => {
                        let id = crate::orchestra::tcp_resources::next_handle_id();
                        crate::orchestra::tcp_resources::RESOURCE_TABLE.insert(
                            id,
                            crate::orchestra::tcp_resources::Resource::TcpStream(std::sync::Arc::new(tokio::sync::Mutex::new(stream)))
                        );
                        Ok(format!("\"handle(tcp:{})\"", id))
                    }
                    Err(e) => Err(anyhow::anyhow!("TCP connect error: {}", e)),
                }
            }
            "luister" => {
                if !ctx.is_privileged { return Err(anyhow::anyhow!("Elevated privileges required for TCP.")); }
                if args.is_empty() { return Err(anyhow::anyhow!("Missing address for tcp luister")); }
                let addr_str = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                
                match tokio::net::TcpListener::bind(&addr_str).await {
                    Ok(listener) => {
                        let id = crate::orchestra::tcp_resources::next_handle_id();
                        crate::orchestra::tcp_resources::RESOURCE_TABLE.insert(
                            id,
                            crate::orchestra::tcp_resources::Resource::TcpListener(std::sync::Arc::new(tokio::sync::Mutex::new(listener)))
                        );
                        Ok(format!("\"handle(tcp_listener:{})\"", id))
                    }
                    Err(e) => Err(anyhow::anyhow!("TCP bind error: {}", e)),
                }
            }
            "accepteer" => {
                if args.is_empty() { return Err(anyhow::anyhow!("Missing handle for tcp accepteer")); }
                let handle_str = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                if let Some(id_str) = handle_str.strip_prefix("handle(tcp_listener:").and_then(|s| s.strip_suffix(")")) {
                    if let Ok(id) = id_str.parse::<u64>() {
                        if let Some(res) = crate::orchestra::tcp_resources::RESOURCE_TABLE.get(&id) {
                            if let crate::orchestra::tcp_resources::Resource::TcpListener(listener_arc) = res.value() {
                                let listener = listener_arc.lock().await;
                                match listener.accept().await {
                                    Ok((stream, _)) => {
                                        let new_id = crate::orchestra::tcp_resources::next_handle_id();
                                        crate::orchestra::tcp_resources::RESOURCE_TABLE.insert(
                                            new_id,
                                            crate::orchestra::tcp_resources::Resource::TcpStream(std::sync::Arc::new(tokio::sync::Mutex::new(stream)))
                                        );
                                        return Ok(format!("\"handle(tcp:{})\"", new_id));
                                    }
                                    Err(e) => return Err(anyhow::anyhow!("Accept error: {}", e)),
                                }
                            }
                        }
                    }
                }
                Err(anyhow::anyhow!("Invalid listener handle"))
            }
            "stuur" => {
                if args.len() < 2 { return Err(anyhow::anyhow!("Missing arguments for tcp stuur")); }
                let handle_str = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                let data = self.evaluate_ast_expr(&args[1], ctx.clone()).await?.trim_matches('"').to_string();
                
                if let Some(id_str) = handle_str.strip_prefix("handle(tcp:").and_then(|s| s.strip_suffix(")")) {
                    if let Ok(id) = id_str.parse::<u64>() {
                        if let Some(res) = crate::orchestra::tcp_resources::RESOURCE_TABLE.get(&id) {
                            if let crate::orchestra::tcp_resources::Resource::TcpStream(stream_arc) = res.value() {
                                let mut stream = stream_arc.lock().await;
                                use tokio::io::AsyncWriteExt;
                                stream.write_all(data.as_bytes()).await?;
                                return Ok("\"ok\"".to_string());
                            }
                        }
                    }
                }
                Err(anyhow::anyhow!("Invalid handle or stream not found"))
            }
            "ontvang" => {
                if args.is_empty() { return Err(anyhow::anyhow!("Missing handle for tcp ontvang")); }
                let handle_str = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                
                if let Some(id_str) = handle_str.strip_prefix("handle(tcp:").and_then(|s| s.strip_suffix(")")) {
                    if let Ok(id) = id_str.parse::<u64>() {
                        if let Some(res) = crate::orchestra::tcp_resources::RESOURCE_TABLE.get(&id) {
                            if let crate::orchestra::tcp_resources::Resource::TcpStream(stream_arc) = res.value() {
                                let mut stream = stream_arc.lock().await;
                                use tokio::io::AsyncReadExt;
                                let mut buf = vec![0u8; 4096];
                                let n = stream.read(&mut buf).await?;
                                if n > 0 {
                                    let content = String::from_utf8_lossy(&buf[..n]).to_string();
                                    return Ok(format!("\"{}\"", content.replace("\n", "\\n").replace("\"", "\\\"")));
                                } else {
                                    return Ok("\"\"".to_string());
                                }
                            }
                        }
                    }
                }
                Err(anyhow::anyhow!("Invalid handle or stream not found"))
            }
            "sluit" => {
                if args.is_empty() { return Err(anyhow::anyhow!("Missing handle for tcp sluit")); }
                let handle_str = self.evaluate_ast_expr(&args[0], ctx.clone()).await?.trim_matches('"').to_string();
                if let Some(id_str) = handle_str.strip_prefix("handle(tcp:").and_then(|s| s.strip_suffix(")")) {
                    if let Ok(id) = id_str.parse::<u64>() {
                        crate::orchestra::tcp_resources::RESOURCE_TABLE.remove(&id);
                        return Ok("\"ok\"".to_string());
                    }
                }
                Ok("\"ok\"".to_string())
            }
            _ => Err(anyhow::anyhow!("Unknown TCP operation: {}", operation)),
        }
    }

    async fn propagate_return(&self, body: &CodeTaal, ctx: crate::common::context::ExecutionContext) -> Result<Option<String>> {
        match body {
            CodeTaal::Block { statements } => self.execute_ast(statements.clone(), ctx).await,
            other => self.execute_ast(vec![other.clone()], ctx).await,
        }
    }

    async fn execute_function_call(&self, ns: &str, name: &str, args: Vec<String>, ctx: crate::common::context::ExecutionContext) -> Result<String> {
        if (name == "tekst" || name == "text" || name == "str") && args.len() == 1 {
            let inner_val = self.memory.resolve_value(&args[0]);
            return Ok(inner_val.trim_matches('"').to_string());
        }
        if (name == "nummer" || name == "number" || name == "num") && args.len() == 1 {
            let inner_val = self.memory.resolve_value(&args[0]);
            if let Ok(num) = inner_val.parse::<f64>() {
                return Ok(num.to_string());
            } else {
                return Ok("0".to_string());
            }
        }

        // --- NATIVE MODULE HOT RELOAD ---
        if name == "systeem.herlaad_module" && args.len() == 1 {
            if !ctx.is_privileged {
                return Err(anyhow::anyhow!("[SECURITY]: Ongeldige bevoegdheid: Sandbox mag geen modules herladen"));
            }
            let mod_name = self.memory.resolve_value(&args[0]).trim_matches('"').to_string();
            match self.package_manager.reload_native(&mod_name, 0).await {
                Ok(_module) => {
                    tracing::info!("[AST]: Externe Wasm module '{}' klaar voor gebruik.", mod_name);
                    return Ok("waar".to_string());
                }
                Err(e) => {
                    tracing::debug!("[HOT RELOAD]: Fout bij herladen van module '{}': {}", mod_name, e);
                    return Ok("onwaar".to_string());
                }
            }
        }

        // 1. Try Native System Library
        let full_name = if ns.is_empty() { name.to_string() } else { format!("{}.{}", ns, name) };
        if let Some(res) = system::SystemManager::try_execute_native(&self.memory, &full_name, &args, &ctx).await? {
            return Ok(res);
        }

        // 2. Try User-Defined AST Function (pure CodeTaal general path)
        let mut func_tuple = None;
        let mut target_ns = ns.to_string();
        
        // If ns was explicitly provided, use it. Otherwise try global, then fallback to current_module.
        if ns.is_empty() {
            if let Some(global_ns_map) = self.memory.ast_funcs.get("") {
                func_tuple = global_ns_map.get(name).map(|v| v.value().clone());
            }
            if func_tuple.is_none() {
                if let Some(current_ns) = &ctx.current_module {
                    target_ns = current_ns.clone();
                    if let Some(local_ns_map) = self.memory.ast_funcs.get(current_ns) {
                        func_tuple = local_ns_map.get(name).map(|v| v.value().clone());
                    }
                }
            }
        } else {
            if let Some(ns_map) = self.memory.ast_funcs.get(ns) {
                func_tuple = ns_map.get(name).map(|v| v.value().clone());
            }
        }

        if let Some((params, body, is_pub)) = func_tuple {
            // Visibility (pub/private) enforcement [W·AG·AF]
            if !target_ns.is_empty() && !is_pub {
                let mut allowed = false;
                if let Some(current_ns) = &ctx.current_module {
                    if current_ns == &target_ns {
                        allowed = true;
                    }
                }
                if !allowed {
                    return Err(anyhow::anyhow!("Functie '{}::{}' is privé en kan niet van buiten de module worden aangeroepen.", target_ns, name));
                }
            }

            let mut resolved_args = Vec::new();
            for i in 0..params.len() {
                if i < args.len() {
                    resolved_args.push(self.memory.resolve_value(&args[i]));
                } else {
                    resolved_args.push("".to_string());
                }
            }

            let _scope_guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);

            for (i, param) in params.iter().enumerate() {
                self.memory.set_var_native(param.clone(), HelheimType::parse(&resolved_args[i]));
                if ctx.is_distributed { self.distributed.set_global(&param, resolved_args[i].clone()); }
            }

            // Robust return propagation:
            // propagate_return + the early `return Ok(Some(ret))` in If/Loop/ForEach/TryCatch
            // make a deep `retourneer` from inside geneste als/zolang immediately unwind
            // all the way out of this function's execute_ast call.
            let result = match self.propagate_return(&body, ctx.clone()).await {
                Ok(Some(ret)) => ret,
                Ok(None) => "".to_string(),
                Err(e) => return Err(e),
            };

            Ok(result)
        } else {
            tracing::error!("[ERR]: Functie '{}' bestaat niet in AST store of Native Library.", name);
            Ok("".to_string())
        }
    }

    /// Helper to turn a CodeTaal expr (Literal/VarGet) into a usable String for I/O paths/urls/content.
    /// (I/O performing cases are handled at statement level to avoid async recursion.)
    fn code_taal_to_string_sync(&self, expr: &CodeTaal) -> String {
        match expr {
            CodeTaal::Literal(lit) => lit.to_string().trim_matches('"').to_string(),
            CodeTaal::VarGet { name } => self.memory.resolve_value(name),
            // For complex exprs that are themselves I/O, we just use a placeholder here; the statement arm will have executed it
            _ => "".to_string(),
        }
    }


    pub async fn evaluate_condition(&self, condition: &str) -> bool {
        let condition = condition.trim();
        if condition == "waar" || condition == "true" {
            return true;
        }
        if condition == "onwaar" || condition == "false" {
            return false;
        }

        if let Some(path_str) = condition.strip_prefix("bestand_bestaat ")
            .or_else(|| condition.strip_prefix("file_exists ")) {
            let path = path_str.trim().trim_matches('"');
            return tokio::fs::try_exists(path).await.unwrap_or(false);
        }

        let result = self.evaluate_expression(condition);
        if result == "waar" {
            return true;
        }
        if result == "onwaar" {
            return false;
        }

        tracing::debug!(
            "[LOGIC]: Onbekende of ongeldige conditie: '{}' (Geëvalueerd tot '{}')",
            condition, result
        );
        false
    }
    async fn evaluate_ast_condition(&self, cond: &CodeTaal, ctx: crate::common::context::ExecutionContext) -> Result<bool> {
        let evaluated = self.evaluate_ast_expr(cond, ctx).await?;
        Ok(self.evaluate_condition(&evaluated).await)
    }

    pub fn evaluate_ast_expr<'a>(&'a self, expr: &'a CodeTaal, ctx: crate::common::context::ExecutionContext) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            // === Flight Recorder hook (Vraag 3) - zero-overhead when disabled ===
            if crate::orchestra::flight_recorder::is_enabled() {
                crate::orchestra::flight_recorder::record(
                    crate::orchestra::flight_recorder::TraceKind::ExprEvalStart,
                    crate::orchestra::flight_recorder::node_id_for(expr, Some(&self.memory)),
                    0,
                );
            }

            match expr {
                CodeTaal::Literal(l) => Ok(l.to_string().trim_matches('"').to_string()),
                CodeTaal::VarGet { name } => Ok(self.memory.resolve_value(name)),
                CodeTaal::Op { left, op, right } => {
                    let l = self.evaluate_ast_expr(left, ctx.clone()).await?;
                    let r = self.evaluate_ast_expr(right, ctx.clone()).await?;
                    
                    // --- Popcount / bit intrinsics CPU Fallback ---
                    if op == "popc" {
                        let mut count = 0;
                        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&l) {
                            for val in arr {
                                if let Some(s) = val.as_str() {
                                    if s == "waar" || s == "true" || s == "1" { count += 1; }
                                } else if let Some(b) = val.as_bool() {
                                    if b { count += 1; }
                                } else if let Some(n) = val.as_i64() {
                                    if n == 1 { count += 1; }
                                }
                            }
                        } else {
                            count = l.matches("waar").count() + l.matches("true").count();
                        }
                        return Ok(count.to_string());
                    }

                    if op == "&" {
                        if l.starts_with('[') && r.starts_with('[') {
                            if let (Ok(arr_l), Ok(arr_r)) = (
                                serde_json::from_str::<Vec<serde_json::Value>>(&l),
                                serde_json::from_str::<Vec<serde_json::Value>>(&r)
                            ) {
                                let mut res = Vec::new();
                                for (vl, vr) in arr_l.iter().zip(arr_r.iter()) {
                                    let l_is_true = vl.as_str().map(|s| s == "waar" || s == "true").unwrap_or(false) || vl.as_bool().unwrap_or(false) || vl.as_i64().unwrap_or(0) == 1;
                                    let r_is_true = vr.as_str().map(|s| s == "waar" || s == "true").unwrap_or(false) || vr.as_bool().unwrap_or(false) || vr.as_i64().unwrap_or(0) == 1;
                                    res.push(if l_is_true && r_is_true { "waar" } else { "onwaar" });
                                }
                                return Ok(serde_json::to_string(&res).unwrap_or_default());
                            }
                        }
                    }
                    
                    if op == "+" {
                        if l.parse::<f64>().is_err() && r.parse::<f64>().is_err() {
                            return Ok(format!("{}{}", l, r));
                        }
                    }
                    // -----------------------------------

                    let to_evalexpr_literal = |val: &str| -> String {
                        if val == "waar" || val == "true" { return "true".to_string(); }
                        if val == "onwaar" || val == "false" { return "false".to_string(); }
                        if val.parse::<f64>().is_ok() { return val.to_string(); }
                        format!("\"{}\"", val.replace("\"", "\\\""))
                    };
                    
                    let l_lit = to_evalexpr_literal(&l);
                    let r_lit = to_evalexpr_literal(&r);
                    let expr_str = format!("{} {} {}", l_lit, op, r_lit);
                    Ok(self.evaluate_expression(&expr_str))
                }
                CodeTaal::FunctionCall { name, args } => {
                    let mut resolved_args = Vec::new();
                    for a in args {
                        resolved_args.push(self.evaluate_ast_expr(a, ctx.clone()).await?);
                    }

                    // Native FFI Dispatch intercept
                    let mut ffi_result = None;
                    if let Some((mod_name, _func_name)) = name.rsplit_once("::") {
                        let module_opt = self.package_manager.get_native(mod_name).await;
                        
                        if let Some(module) = module_opt {
                                // Convert Helheim strings to internal types for Wasm
                                let mut ht_args = Vec::new();
                                for arg in &resolved_args {
                                    ht_args.push(crate::orchestra::memory::HelheimType::parse(arg));
                                }

                                // Phase 2: Wasm Sandboxing Call
                                // Wasm exports usually don't have ::, they use _ as separator (e.g. math_sin)
                                let export_name = name.replace("::", "_");
                                match module.call_function(&export_name, &ht_args) {
                                    Ok(ret_ht) => {
                                        ffi_result = Some(Ok(ret_ht.to_string()));
                                    }
                                    Err(e) => {
                                        ffi_result = Some(Err(anyhow::anyhow!("[WASM FFI ERROR] {}: {}", name, e)));
                                    }
                                }
                        }
                    }

                    if let Some(res) = ffi_result {
                        res
                    } else {
                        self.execute_function_call("", name, resolved_args, ctx).await
                    }
                }
                CodeTaal::QualifiedCall { ns, name, args } => {
                    let mut resolved_args = Vec::new();
                    for a in args {
                        resolved_args.push(self.evaluate_ast_expr(a, ctx.clone()).await?);
                    }

                    // Native FFI Dispatch intercept
                    let mut ffi_result = None;
                    let module_opt = self.package_manager.get_native(ns).await;
                    
                    if let Some(module) = module_opt {
                            let mut ht_args = Vec::new();
                            for arg in &resolved_args {
                                ht_args.push(crate::orchestra::memory::HelheimType::parse(arg));
                            }
                            let export_name = format!("{}_{}", ns, name);
                            match module.call_function(&export_name, &ht_args) {
                                Ok(ret_ht) => {
                                    ffi_result = Some(Ok(ret_ht.to_string()));
                                }
                                Err(e) => {
                                    ffi_result = Some(Err(anyhow::anyhow!("[WASM FFI ERROR] {}::{}: {}", ns, name, e)));
                                }
                            }
                    }

                    if let Some(res) = ffi_result {
                        res
                    } else {
                        self.execute_function_call(ns, name, resolved_args, ctx).await
                    }
                }
                CodeTaal::TcpConnect { addr } => {
                    let addr_str = self.evaluate_ast_expr(&addr, ctx.clone()).await?;
                    let addr_str = addr_str.trim_matches('"').to_string();
                    match tokio::net::TcpStream::connect(&addr_str).await {
                        Ok(stream) => {
                            let id = crate::orchestra::tcp_resources::next_handle_id();
                            crate::orchestra::tcp_resources::RESOURCE_TABLE.insert(
                                id,
                                crate::orchestra::tcp_resources::Resource::TcpStream(
                                    std::sync::Arc::new(tokio::sync::Mutex::new(stream))
                                )
                            );
                            tracing::debug!("[TCP CONNECT]: Verbonden met {} (handle: handle(tcp:{}))", addr_str, id);
                            Ok(format!("handle(tcp:{})", id))
                        }
                        Err(e) => Err(anyhow::anyhow!("[TCP ERROR]: Verbinden met {} mislukt: {}", addr_str, e)),
                    }
                }
                CodeTaal::TcpReceive { socket, max_bytes } => {
                    let handle_str = self.evaluate_ast_expr(&socket, ctx.clone()).await?;
                    let max = if let Some(mb) = max_bytes {
                        self.evaluate_ast_expr(&mb, ctx.clone()).await?
                            .parse::<usize>().unwrap_or(4096)
                    } else { 4096 };

                    let id: Option<u64> = handle_str.trim_matches('"')
                        .strip_prefix("handle(tcp:")
                        .and_then(|s| s.strip_suffix(')'))
                        .and_then(|s| s.parse().ok());

                    if let Some(id) = id {
                        if let Some(res) = crate::orchestra::tcp_resources::RESOURCE_TABLE.get(&id) {
                            match res.value() {
                                crate::orchestra::tcp_resources::Resource::TcpStream(stream) => {
                                    use tokio::io::AsyncReadExt;
                                    let mut guard = stream.lock().await;
                                    let mut buf = vec![0u8; max];
                                    match guard.read(&mut buf).await {
                                        Ok(n) => {
                                            let received = buf[..n].to_vec();
                                            Ok(String::from_utf8_lossy(&received).to_string())
                                        }
                                        Err(e) => Err(anyhow::anyhow!("[TCP ERROR]: Lezen mislukt: {}", e)),
                                    }
                                }
                                _ => Err(anyhow::anyhow!("[TCP ERROR]: Handle {} is geen TcpStream", id)),
                            }
                        } else {
                            Err(anyhow::anyhow!("[TCP ERROR]: Handle {} niet gevonden", id))
                        }
                    } else {
                        Err(anyhow::anyhow!("[TCP ERROR]: Ongeldig handle formaat: '{}'", handle_str))
                    }
                }
                CodeTaal::TcpClose { socket } => {
                    let handle_str = self.evaluate_ast_expr(&socket, ctx.clone()).await?;
                    let id: Option<u64> = handle_str.trim_matches('"')
                        .strip_prefix("handle(tcp:")
                        .and_then(|s| s.strip_suffix(')'))
                        .and_then(|s| s.parse().ok());

                    if let Some(id) = id {
                        if crate::orchestra::tcp_resources::RESOURCE_TABLE.remove(&id).is_some() {
                            Ok("".to_string())
                        } else {
                            Err(anyhow::anyhow!("[TCP ERROR]: Onbekend of al gesloten handle: tcp:{}", id))
                        }
                    } else {
                        Err(anyhow::anyhow!("[TCP ERROR]: Ongeldig handle formaat: '{}'", handle_str))
                    }
                }
                CodeTaal::ModelInit { model_name, args } => {
                    let fields_opt = self.memory.model_store.get(model_name).map(|v| v.value().clone());
                    if let Some(fields) = fields_opt {
                        if fields.len() != args.len() {
                            tracing::error!("[ERROR]: Model '{}' verwacht {} argumenten, kreeg er {}.", model_name, fields.len(), args.len());
                            Ok("null".to_string())
                        } else {
                            let mut resolved_args = Vec::new();
                            for arg in args {
                                // De parser geeft Strings die kunnen verwijzen naar vars of literals zijn
                                // We moeten self.evaluate_ast_expr niet direct aanroepen als het al Strings zijn, 
                                // want args is Vec<String>. 
                                // De oude manier was: parse comma string. 
                                // Als de nieuwe manier Vec<String> is, moeten we die resolven.
                                resolved_args.push(self.memory.resolve_value(&arg.trim().trim_matches('"')));
                            }
                            
                            let mut json_map = serde_json::Map::new();
                            for (i, field) in fields.iter().enumerate() {
                                let val_str: &str = &resolved_args[i];
                                let json_val = if let Ok(num) = val_str.parse::<i64>() {
                                    serde_json::json!(num)
                                } else if let Ok(num) = val_str.parse::<f64>() {
                                    serde_json::json!(num)
                                } else if val_str == "waar" || val_str == "true" {
                                    serde_json::json!(true)
                                } else if val_str == "onwaar" || val_str == "false" {
                                    serde_json::json!(false)
                                } else {
                                    serde_json::json!(val_str)
                                };
                                json_map.insert(field.clone(), json_val);
                            }
                            Ok(serde_json::to_string(&json_map).unwrap_or_else(|_| "null".to_string()))
                        }
                    } else {
                        tracing::error!("[ERROR]: Model '{}' is niet gedefinieerd.", model_name);
                        Ok("null".to_string())
                    }
                }
                _ => Ok("".to_string()),
            }
        })
    }


    fn evaluate_expression(&self, expr: &str) -> String {
        let expr_clean = expr.trim();

        // Pure string literal — return content without re-processing (prevents "en"→"&&" corruption)
        if expr_clean.starts_with('"') && expr_clean.ends_with('"') && expr_clean.len() >= 2
            && expr_clean.chars().filter(|&c| c == '"').count() == 2
        {
            return expr_clean[1..expr_clean.len() - 1].to_string();
        }

        // Native STD LIB: lengte(Lijst) / length(List)
        if (expr_clean.starts_with("lengte(") || expr_clean.starts_with("length(") || expr_clean.starts_with("len("))
            && expr_clean.ends_with(")") {
            let start = expr_clean.find('(').unwrap_or(0) + 1;
            let inner = expr_clean[start..expr_clean.len() - 1].trim();
            let inner_val = self.memory.resolve_value(inner);
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&inner_val) {
                return arr.len().to_string();
            } else {
                return inner_val.len().to_string();
            }
        }

        // Tensor Allocation Intercept
        if expr_clean.starts_with("tensor(")
            && expr_clean.ends_with(")")
            && !expr_clean.contains("id=")
        {
            let dim: Vec<&str> = expr_clean[7..expr_clean.len() - 1].split(',').collect();
            if dim.len() == 2 {
                let m = dim[0].trim().parse::<usize>().unwrap_or(0);
                let n = dim[1].trim().parse::<usize>().unwrap_or(0);
                if m > 0 && n > 0 {
                    tracing::debug!("[AST]: Nieuwe Tensor allocatie ({}x{})...", m, n);
                    match crate::gpu::gpu_alloc_tensor_random(m, n) {
                        Ok(id) => return format!("tensor({}, {}, id={})", m, n, id),
                        Err(e) => return format!("ERROR: VRAM Allocatie gefaald: {}", e),
                    }
                }
            }
        }

        // Tensor ReLU Intercept (Project Apex)
        if expr_clean.starts_with("relu(") && expr_clean.ends_with(")") {
            let inner = expr_clean[5..expr_clean.len() - 1].trim();
            let inner_val = self.memory.resolve_value(inner);
            if inner_val.starts_with("tensor(") && inner_val.contains("id=") {
                let parts: Vec<&str> = inner_val[7..inner_val.len() - 1].split(',').collect();
                if parts.len() == 3 {
                    let m = parts[0].trim().parse::<usize>().unwrap_or(0);
                    let n = parts[1].trim().parse::<usize>().unwrap_or(0);
                    let id_a = parts[2]
                        .trim()
                        .replace("id=", "")
                        .parse::<usize>()
                        .unwrap_or(0);
                    if m > 0 && n > 0 {
                        tracing::debug!(
                            "[AST]: Tensor Activering (ReLU) gedetecteerd op {}x{}...",
                            m, n
                        );
                        let out_id = match crate::gpu::gpu_alloc_tensor_empty(m, n) { Ok(id) => id, Err(e) => return format!("ERROR: GPU Allocatie gefaald: {}", e) };
                        let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                            CodeTaal::TensorRelu { m, n },
                        )
                        .unwrap_or_else(|_| String::new());
                        match crate::gpu::gpu_execute_tensor_relu(&ptx, id_a, out_id, m, n) {
                            Ok(gflops) => tracing::debug!(
                                "[GPU]: ✅ Tensor ReLU voltooid. Performance: {:.2} GFLOPS",
                                gflops
                            ),
                            Err(e) => tracing::error!("[ERROR]: GPU Tensor ReLU Fail: {}", e),
                        }
                        return format!("tensor({}, {}, id={})", m, n, out_id);
                    }
                }
            }
        }

        // --- TENSOR INTERCEPTS (Project Apex) ---
        // If the expression looks like a simple arithmetic operation, check if it's tensor math
        let parts: Vec<&str> = expr_clean.split_whitespace().collect();
        let mut left_val = String::new();
        let mut right_val = String::new();
        let mut op = "";
        if parts.len() == 3 {
            op = parts[1];
            left_val = self.memory.resolve_value(parts[0]);
            right_val = self.memory.resolve_value(parts[2]);
        }

        // Tensor Multiplication Intercept (Project Apex-WMMA)
        if left_val.starts_with("tensor(") && right_val.starts_with("tensor(") && op == "*" {
            let l_dim: Vec<&str> = left_val[7..left_val.len() - 1].split(',').collect();
            let r_dim: Vec<&str> = right_val[7..right_val.len() - 1].split(',').collect();
            if l_dim.len() == 3 && r_dim.len() == 3 {
                let m = l_dim[0].trim().parse::<usize>().unwrap_or(0);
                let k1 = l_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_a = l_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                let k2 = r_dim[0].trim().parse::<usize>().unwrap_or(0);
                let n = r_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_b = r_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                if k1 == k2 && k1 > 0 {
                    tracing::debug!(
                        "[AST]: Tensor vermenigvuldiging gedetecteerd. Matrix {}x{} * {}x{}...",
                        m, k1, k2, n
                    );
                    let out_id = match crate::gpu::gpu_alloc_tensor_empty(m, n) { Ok(id) => id, Err(e) => return format!("ERROR: GPU Allocatie gefaald: {}", e) };
                    let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                        CodeTaal::MatMul { m, n, k: k1 },
                    )
                    .unwrap_or_else(|_| String::new());
                    tracing::debug!("[GPU]: Activeren van WMMA Tensor Cores (Project Apex)...");
                    match crate::gpu::gpu_execute_raw_ptx_ids(&ptx, id_a, id_b, out_id, m, n, k1) {
                        Ok(gflops) => tracing::debug!(
                            "[GPU]: ✅ Tensor Executie voltooid. Performance: {:.2} GFLOPS",
                            gflops
                        ),
                        Err(e) => {
                            tracing::debug!("[GPU ERROR]: {} - Terugvallen op CPU (Rayon)...", e);
                            match crate::gpu::cpu_execute_matmul(id_a, id_b, out_id, m, n, k1) {
                                Ok(gflops) => tracing::debug!(
                                    "[CPU]: ✅ Tensor Executie voltooid (Fallback). Performance: {:.2} GFLOPS",
                                    gflops
                                ),
                                Err(e) => tracing::debug!("[CPU ERROR]: {}", e),
                            }
                        }
                    }
                    return format!("tensor({}, {}, id={})", m, n, out_id);
                } else {
                    tracing::debug!(
                        "[ERROR]: Tensor dimensies komen niet overeen ({}x{} * {}x{})",
                        m, k1, k2, n
                    );
                }
            }
        }

        // Tensor Addition Intercept (Project Apex-WMMA)
        if left_val.starts_with("tensor(") && right_val.starts_with("tensor(") && op == "+" {
            let l_dim: Vec<&str> = left_val[7..left_val.len() - 1].split(',').collect();
            let r_dim: Vec<&str> = right_val[7..right_val.len() - 1].split(',').collect();
            if l_dim.len() == 3 && r_dim.len() == 3 {
                let m1 = l_dim[0].trim().parse::<usize>().unwrap_or(0);
                let n1 = l_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_a = l_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                let m2 = r_dim[0].trim().parse::<usize>().unwrap_or(0);
                let n2 = r_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_b = r_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                if m1 == m2 && n1 == n2 && m1 > 0 {
                    tracing::debug!(
                        "[AST]: Tensor Optelling gedetecteerd. Matrix {}x{} + {}x{}...",
                        m1, n1, m2, n2
                    );
                    let out_id = match crate::gpu::gpu_alloc_tensor_empty(m1, n1) { Ok(id) => id, Err(e) => return format!("ERROR: GPU Allocatie gefaald: {}", e) };
                    let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                        CodeTaal::TensorAdd { m: m1, n: n1 },
                    )
                    .unwrap_or_else(|_| String::new());
                    match crate::gpu::gpu_execute_tensor_add(&ptx, id_a, id_b, out_id, m1, n1) {
                        Ok(gflops) => tracing::debug!(
                            "[GPU]: ✅ Tensor Optelling voltooid. Performance: {:.2} GFLOPS",
                            gflops
                        ),
                        Err(e) => {
                            tracing::debug!("[GPU ERROR]: {} - Terugvallen op CPU (Rayon)...", e);
                            match crate::gpu::cpu_execute_tensor_add(id_a, id_b, out_id, m1, n1) {
                                Ok(gflops) => tracing::debug!(
                                    "[CPU]: ✅ Tensor Optelling voltooid (Fallback). Performance: {:.2} GFLOPS",
                                    gflops
                                ),
                                Err(e) => tracing::debug!("[CPU ERROR]: {}", e),
                            }
                        }
                    }
                    return format!("tensor({}, {}, id={})", m1, n1, out_id);
                }
            }
        }

        // Robust expression evaluator (evalexpr)
        // If it's not a tensor operation, try to evaluate it as a complex math/logic expression
        if !expr_clean.starts_with("tensor(") && !expr_clean.contains("tensor(") {
            use evalexpr::ContextWithMutableVariables;
            use evalexpr::Context;
            let mut context: evalexpr::HashMapContext = evalexpr::HashMapContext::new();
            {
                let store = self.memory.local_stack.lock().unwrap_or_else(|e| e.into_inner());
                for scope in store.iter().rev() {
                    for (k, v) in scope.iter() {
                        if let HelheimType::Int(num_int) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Int(*num_int));
                        } else if let HelheimType::Float(num_float) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Float(*num_float));
                        } else if let HelheimType::Bool(b) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Boolean(*b));
                        } else {
                            let val_str = match v {
                                HelheimType::String(s) => s.clone(),
                                _ => v.to_string(),
                            };
                            let _ = context.set_value(k.clone(), val_str.into());
                        }
                    }
                }
                for entry in self.memory.globals.iter() {
                    let k = entry.key();
                    let v = entry.value();
                    if context.get_value(k).is_none() {
                        if let HelheimType::Int(num_int) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Int(*num_int));
                        } else if let HelheimType::Float(num_float) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Float(*num_float));
                        } else if let HelheimType::Bool(b) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Boolean(*b));
                        } else {
                            let val_str = match v {
                                HelheimType::String(s) => s.clone(),
                                _ => v.to_string(),
                            };
                            let _ = context.set_value(k.clone(), val_str.into());
                        }
                    }
                }
            }

            let _ = context.set_value("waar".to_string(), evalexpr::Value::Boolean(true));
            let _ = context.set_value("onwaar".to_string(), evalexpr::Value::Boolean(false));

            let eval_str = expr_clean
                .replace(" en ", " && ")
                .replace(" of ", " || ")
                .replace("niet ", "!");

            match evalexpr::eval_with_context(&eval_str, &context) {
                Ok(result) => {
                    match result {
                        evalexpr::Value::Int(i) => return format!("{}", i),
                        evalexpr::Value::Float(f) => {
                            if f.fract() == 0.0 {
                                return format!("{}.0", f);
                            } else {
                                return format!("{}", f);
                            }
                        },
                        evalexpr::Value::Boolean(b) => {
                            return (if b { "waar" } else { "onwaar" }).to_string();
                        }
                        evalexpr::Value::String(s) => return s.clone(),
                        evalexpr::Value::Tuple(t) => {
                            // Serialize Tuple to a JSON array string for Helheim's internal representation
                            let mut json_arr = "[".to_string();
                            for (i, v) in t.iter().enumerate() {
                                if i > 0 {
                                    json_arr.push_str(", ");
                                }
                                match v {
                                    evalexpr::Value::Int(ni) => json_arr.push_str(&ni.to_string()),
                                    evalexpr::Value::Float(nf) => {
                                        json_arr.push_str(&nf.to_string())
                                    }
                                    evalexpr::Value::String(ns) => {
                                        json_arr.push_str(&format!("\"{}\"", ns))
                                    }
                                    _ => json_arr.push_str("\"complex_type\""),
                                }
                            }
                            json_arr.push(']');
                            return json_arr;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if (err_str.contains("Expected") || err_str.contains("wrong combination of types")) && (err_str.contains("String") || err_str.contains("Int") || err_str.contains("Float")) {
                        // [W·AG·AF] Definitieve Monniken Review: 
                        // Deze println! statements blijven expliciet behouden. Dit is user-facing output 
                        // ontworpen om direct syntax-hulp te bieden in de CLI wanneer beginners fouten maken 
                        // met string-concatenatie. Het is geen debug-noise, maar een compiler/REPL help feature.
                        println!("{}", format!("\n[SYNTAX HULP]: Fout in de berekening: '{}'", expr_clean).yellow());
                        println!("{}", format!("  -> Je probeert tekst (String) en getallen (Int/Float) direct te combineren.").yellow());
                        println!("{}", format!("  -> In de nieuwe Native Type engine is dit niet toegestaan ter bescherming van de runtime.").yellow());
                        println!("{}", format!("  -> Oplossing: Houd berekeningen en tekst gescheiden, of bouw een tekst() formatter (komt in volgende update).").cyan());
                    } else if (!err_str.contains("Variable identifier is not bound")
                        && !err_str.contains("Tried to append a node"))
                        || !expr_clean.contains("[")
                    {
                        tracing::debug!(
                            "[DEBUG]: evalexpr gaf fout op '{}': {}",
                            expr_clean, err_str
                        );
                    }
                }
            }
        }

        // Fallback: return as is (maybe it's just a value or string)
        self.memory.resolve_value(expr)
    }

    fn build_eval_context(&self, node: &CodeTaal) -> std::collections::HashMap<String, helheim_lang::ast::LiteralValue> {
        let free_vars = helheim_lang::synthesis::collect_free_variables(node);
        let mut context = std::collections::HashMap::new();
        for name in free_vars {
            if let Some(typed) = self.memory.get_var_native(&name) {
                match typed {
                    crate::orchestra::memory::HelheimType::Bool(b) => {
                        context.insert(name, helheim_lang::ast::LiteralValue::Int(if b { 1 } else { 0 }));
                    }
                    crate::orchestra::memory::HelheimType::List(items) => {
                        let mut mask: u32 = 0;
                        for (i, item) in items.iter().take(32).enumerate() {
                            let is_true = match item {
                                serde_json::Value::Bool(b) => *b,
                                serde_json::Value::String(s) => s == "waar" || s == "true" || s == "1",
                                _ => false,
                            };
                            if is_true {
                                mask |= 1 << i;
                            }
                        }
                        context.insert(name, helheim_lang::ast::LiteralValue::Int(mask as i64));
                    }
                    crate::orchestra::memory::HelheimType::Int(i) => {
                        context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                    }
                    crate::orchestra::memory::HelheimType::Float(f) => {
                        context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                    }
                    crate::orchestra::memory::HelheimType::String(s) => {
                        context.insert(name, helheim_lang::ast::LiteralValue::String(s));
                    }
                    _ => {
                        let s = typed.to_string();
                        context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                    }
                }
            } else {
                let s = self.memory.resolve_value(&name);
                if let Ok(i) = s.parse::<i64>() {
                    context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                } else if let Ok(f) = s.parse::<f64>() {
                    context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                } else {
                    context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                }
            }
        }
        context
    }
}
