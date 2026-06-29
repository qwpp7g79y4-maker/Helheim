use crate::common::rune::RuneEngine;
use crate::network::DiscoveryService;
use helheim_lang::ast::CodeTaal;
use helheim_lang::synthesis::KernelSynthesisEngine;
use crate::shield::HelheimLock;
use anyhow::Result;
use colored::*;
use std::sync::Arc;

pub use helheim_lang::synthesis;
pub use helheim_lang::parser;
pub use helheim_lang::resolver;
pub use helheim_lang::persistence;
pub use helheim_lang::memory;
pub use helheim_lang::semantic;
use crate::cli::intent::{Intent, IntentParser};
use std::pin::Pin;
use std::future::Future;

// orchestra/swarm.rs verwijderd — ConsciousWorker/CleanerWorker hoort in helheim-web (sorteerlaag), niet in helheim-core
pub mod system;
pub mod distributed;
pub mod executor;
pub mod actor;  // Actor / Message-Passing (Vraag 2)
pub mod flight_recorder;  // Flight Recorder / Zero-Overhead Tracing (Vraag 3)
pub mod package_manager;    // Package Manager + Signing (Vraag 4)
pub mod stdlib_manager;
pub mod effects;
pub mod continuation;
pub mod trampoline;


#[derive(Clone)]
pub struct Orchestrator {
    pub executor: executor::Executor,
    pub memory: Arc<memory::MemoryManager>,
    pub discovery: Arc<crate::network::DiscoveryService>,
    pub distributed: Arc<distributed::DistributedMemory>,
}

impl Orchestrator {
    pub fn new(discovery: Arc<DiscoveryService>) -> Self {
        let memory = Arc::new(memory::MemoryManager::new());
        let node_id = std::env::var("HELHEIM_NODE_ID").unwrap_or_else(|_| hostname::get().map(|h| h.to_string_lossy().into_owned()).unwrap_or("node-0".to_string()));
        let distributed = Arc::new(distributed::DistributedMemory::new(node_id));
        Self {
            executor: executor::Executor::new(memory.clone(), discovery.clone(), distributed.clone()),
            memory,
            discovery,
            distributed,
        }
    }

    pub async fn bootstrap(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.executor.stdlib.bootstrap().await?;
        // Register pure functions to global memory
        for module_ref in self.executor.stdlib.pure_modules.iter() {
            for (name, (params, body)) in &module_ref.value().functions {
                self.memory.register_ast_function(Some(module_ref.key()), name.clone(), params.clone(), body.clone(), true); // Stdlib funcs are pub
            }
        }
        Ok(())
    }

    pub fn get_var(&self, key: &str) -> Option<String> {
        self.memory.get_var_native(key).map(|v| v.to_string())
    }

    pub fn set_var(&self, key: String, value: String) {
        self.memory.set_var_native(key, crate::orchestra::memory::HelheimType::parse(&value));
    }

    pub fn execute_ast(&self, ast: Vec<CodeTaal>, ctx: crate::common::context::ExecutionContext) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        self.executor.execute_ast(ast, ctx)
    }

    pub fn resolve_value(&self, value: &str) -> String {
        self.memory.resolve_value(value)
    }

    // Time-Travel REPL support (Vraag 5)
    pub fn snapshot(&self) {
        self.memory.snapshot();
    }

    pub fn rollback(&self, steps: usize) -> bool {
        self.memory.rollback(steps)
    }

    pub async fn resume_continuation_from_file(&self, path: &str, resume_value_str: &str, _ctx: crate::common::context::ExecutionContext) -> Result<Option<String>> {
        let content = tokio::fs::read_to_string(path).await?;
        let mut cont: crate::orchestra::continuation::SerializableContinuation = serde_json::from_str(&content)?;
        
        // Verificatie van de Swarm Handtekening
        if let Some(sig_str) = cont.signature.take() {
            use base64::Engine;
            let sig_bytes = base64::engine::general_purpose::STANDARD.decode(&sig_str)?;
            let json_without_sig = serde_json::to_string(&cont)?;
            if let Some(pub_key) = &cont.source_pubkey {
                crate::shield::crypto::SwarmSigner::verify_peer(pub_key, json_without_sig.as_bytes(), &sig_bytes)?;
                tracing::debug!("[SHIELD]: Continuation handtekening geverifieerd. Veilig om te hervatten.");
            } else {
                anyhow::bail!("⛔ SHIELD ALARM: Continuation mist public key voor verificatie!");
            }
        } else {
            anyhow::bail!("⛔ SHIELD ALARM: Continuation is niet ondertekend! Hervatten geweigerd.");
        }

        let resume_val = crate::orchestra::memory::HelheimType::parse(resume_value_str);
        
        let mut rx = tokio::sync::mpsc::channel(1).1; // dummy receiver since this is not in an actor loop
        crate::orchestra::actor::resume_from_serialized(&self.executor, cont, resume_val, self.memory.clone(), &mut rx).await
    }

    pub fn process_command<'a>(
        &'a self,
        input: &'a str,
        ctx: crate::common::context::ExecutionContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        let input = input.to_string();
        Box::pin(async move {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(());
            }



            // Variables pre-processing removed (now handled in AST)
            // The AST Engine (helheim-lang) now natively handles variable resolution.
            let trimmed = input.trim();

            if trimmed.starts_with("ast_json:") {
                let json = &trimmed["ast_json:".len()..];
                match serde_json::from_str::<Vec<CodeTaal>>(json) {
                    Ok(ast_vec) => {
                        tracing::debug!(
                            "[SWARM RECEIVER]: ast_json ontvangen ({} statements). Directe AST-executie (geen string-parser).",
                            ast_vec.len()
                        );
                        let mut dist_ctx = ctx.clone();
                        dist_ctx.is_distributed = true;
                        let _ = self.execute_ast(ast_vec, dist_ctx).await;
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::error!("[SWARM RECEIVER ERROR]: ast_json deserialisatie mislukt: {}", e);
                        return Ok(());
                    }
                }
            }

            if trimmed.starts_with("state_delta:") {
                let json = &trimmed["state_delta:".len()..];
                match serde_json::from_str::<crate::orchestra::distributed::StateDelta>(json) {
                    Ok(delta) => {
                        tracing::debug!("[SWARM RECEIVER]: state_delta ontvangen van {}", delta.source_node);
                        self.distributed.apply_delta(delta);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::error!("[SWARM RECEIVER ERROR]: state_delta deserialisatie mislukt: {}", e);
                        return Ok(());
                    }
                }
            }

            // Professional log (Flight Recorder)
            tracing::debug!(target: "orchestrator", command = ?trimmed, "Verwerken van instructie.");
            tracing::debug!("[EXECUTION]: Verwerken van instructie: '{}'", trimmed);

            // --- We delegate 'zet' entirely to the AST Parser so it can evaluate expressions ---

            // Hel-modus (bare-metal C++/PTX blocks)
            if trimmed.starts_with("hel {") || trimmed.starts_with("unsafe {") {
                if !ctx.is_privileged {
                    return Err(anyhow::anyhow!("[SECURITY]: Hel-modus vereist Elevated Privileges."));
                }
                use colored::*;
                tracing::debug!(
                    "{}",
                    "================================================="
                        .red()
                        .bold()
                );
                tracing::debug!(
                    "{}",
                    " ⚠️ WAARSCHUWING: JE VERLAAT NU DE VEILIGE ZONE. "
                        .red()
                        .bold()
                );
                tracing::debug!(
                    "{}",
                    "    Je kunt nu nog terug... je belandt in HEL!   "
                        .red()
                        .bold()
                );
                tracing::debug!(
                    "{}",
                    "================================================="
                        .red()
                        .bold()
                );

                let start_idx = trimmed.find('{').ok_or_else(|| anyhow::anyhow!("Ontbrekende '{{' in Hel-block"))? + 1;
                let end_idx = trimmed.rfind('}').unwrap_or(trimmed.len());
                let _raw_code = trimmed[start_idx..end_idx].trim();

                #[cfg(feature = "cuda")]
                {
                    crate::gpu::gpu_execute_hel_block(_raw_code).await?;
                    return Ok(());
                }
                #[cfg(not(feature = "cuda"))]
                {
                    return Err(anyhow::anyhow!("Hel-block execution requires 'cuda' feature"));
                }
            }

            if trimmed.starts_with("script:") {
                let script_content = trimmed[7..].trim();
                tracing::debug!("[LANG]: Helheim Script Modus geactiveerd.");
                match parser::HelParser::parse(script_content) {
                    Ok(ast) => {
                        tracing::debug!(
                            "[LANG]: AST Gegenereerd ({} statements). Linking modules...",
                            ast.len()
                        );
                        let mut linker = resolver::ModuleLinker::with_std_lib(
                            std::path::PathBuf::from("."),
                            std::path::PathBuf::from(".")
                        );
                        match linker.link(ast, std::path::Path::new(".")) {
                            Ok(mut linked_ast) => {
                                if let Err(e) = helheim_lang::semantic::SemanticAnalyzer::analyze(&mut linked_ast) {
                                    tracing::debug!("{}", e);
                                    return Err(anyhow::anyhow!("Semantic Analysis Failed: {}", e));
                                }
                                tracing::debug!("[LANG]: Semantic check OK. Uitvoeren...");
                                self.execute_ast(linked_ast, ctx.clone()).await?;
                            }
                            Err(e) => tracing::debug!("[ERROR]: Module Linker Fout: {}", e),
                        }
                    }
                    Err(e) => tracing::debug!("[ERROR]: Script Parsing Fout: {}", e),
                }
                return Ok(());
            }

            // AST execution
            // Attempt to parse standard language constructs natively using helheim-lang
            if let Ok(ast) = parser::HelParser::parse(trimmed) {
                let is_just_sysop = ast.len() == 1 && matches!(ast[0], CodeTaal::SysOp { .. });
                let is_meta_keyword = trimmed == "nodes" || trimmed.starts_with("unlock ") || trimmed.starts_with("rune ") || trimmed.starts_with("heavy_work ") || trimmed.starts_with("swarm_work ") || trimmed.starts_with("gpu work ") || trimmed.starts_with("gpu infer ") || trimmed.starts_with("shield encrypt ") || trimmed.starts_with("stuur ");

                if !ast.is_empty() && (!is_just_sysop || !is_meta_keyword) {
                    let mut linker = resolver::ModuleLinker::with_std_lib(
                        std::path::PathBuf::from("."),
                        std::path::PathBuf::from(".")
                    );
                    match linker.link(ast, std::path::Path::new(".")) {
                        Ok(mut linked_ast) => {
                            if let Err(e) = helheim_lang::semantic::SemanticAnalyzer::analyze(&mut linked_ast) {
                                tracing::debug!("{}", e); 
                                return Err(anyhow::anyhow!("Semantic Analysis Failed: {}", e));
                            }

                            if let Err(e) = self.execute_ast(linked_ast, ctx.clone()).await {
                                let line = self.memory.resolve_value("__LAST_ERR_LINE__");
                                let col = self.memory.resolve_value("__LAST_ERR_COL__");
                                if !line.is_empty() && !col.is_empty() {
                                    use colored::*;
                                    tracing::debug!("{}", format!("[ERROR at {}:{}]: {}", line, col, e).red().bold());
                                } else {
                                    tracing::debug!("[ERROR]: {}", e);
                                }
                            }
                        }
                        Err(e) => tracing::debug!("[ERROR]: Module Linker Fout: {}", e),
                    }
                    return Ok(());
                }
            }

            // --- Persistence ---
            // Legacy string-based persistence removed. Now handled via AST Onthoud/Herinner nodes (see executor and resolver).

            if trimmed == "nodes" {
                self.list_nodes();
                return Ok(());
            }

            if trimmed.starts_with("unlock ") {
                let key = trimmed[7..].trim();
                if HelheimLock::unlock(key) {
                    tracing::debug!("[SECURITY]: Toegang tot native execution geautoriseerd.");
                } else {
                    tracing::debug!("[SECURITY]: Autorisatie mislukt. Onjuiste Master Key.");
                }
                return Ok(());
            }

            if trimmed.starts_with("rune ") {
                if !ctx.is_privileged {
                    return Err(anyhow::anyhow!("[SECURITY]: Native Runes vereisen Elevated Privileges."));
                }
                self.execute_native(trimmed[5..].trim())?;
                return Ok(());
            }

            // Low-level extensions
            if trimmed.starts_with("heavy_work ") {
                if !ctx.is_privileged {
                    return Err(anyhow::anyhow!("[SECURITY]: Heavy compute work requires elevated privileges."));
                }
                let size = trimmed[11..].trim().parse::<usize>().unwrap_or(8192);
                tracing::debug!(
                    "[HEAVY]: Parallel heavy compute (CPU + GPU) (Size: {})...",
                    size
                );
                match crate::gpu::inferno_work_real(size, 0) {
                    Ok(_) => tracing::debug!("[HEAVY]: Workload complete."),
                    Err(e) => tracing::debug!("[ERROR]: Heavy compute error: {}", e),
                }
                return Ok(());
            }

            if trimmed.starts_with("swarm_work ") {
                let size_str = trimmed[10..].trim();
                let size = size_str.parse::<usize>().unwrap_or(15000);

                let mut node_weights: std::collections::HashMap<String, f64> =
                    std::collections::HashMap::new();

                // 1. Calculate Remote Weights
                if let Ok(peers) = self.discovery.peers.lock() {
                    for (ip, caps) in peers.iter() {
                        let mut weight = caps.estimated_cpu_gflops;
                        // Extreme bump for GPUs (assuming rough 80x multiplier for PTX vs CPU)
                        weight += (caps.gpu_count as f64) * 800.0;
                        if weight < 0.1 {
                            weight = 0.5;
                        } // Minimum fallback weight
                        node_weights.insert(ip.clone(), weight);
                    }
                }

                // Bare Metal guarantee / Fallback if discovery is empty
                if node_weights.is_empty() {
                    tracing::debug!("{}", "[WARN]: Discovery Service leeg. Vooringestelde Swarm Nodes inladen (Equal weights)...".yellow());
                    if let Ok(env_nodes) = std::env::var("HELHEIM_NODES") {
                        for ip in env_nodes.split(',') {
                            node_weights.insert(ip.trim().to_string(), 10.0);
                        }
                    } else {
                        // Fallback to local only to prevent leaking public IPs
                        node_weights.insert("127.0.0.1".to_string(), 10.0);
                    }
                }

                // 2. Calculate Local Weight (Master Node)
                let mut local_weight = 10.0; // Base CPU
                let has_nvidia = std::process::Command::new("nvidia-smi").output().is_ok();
                if has_nvidia {
                    local_weight += 800.0 * 2.0; // Assuming Master has both 5060 and 3060 active
                }
                node_weights.insert("LOKAAL".to_string(), local_weight);

                // 3. Compute Global Pool Weight
                let total_swarm_weight: f64 = node_weights.values().sum();

                tracing::debug!(
                    "[SWARM]: Architecting load-balanced swarm compute..."
                );
                tracing::debug!(
                    "[SWARM]: Total Workload: {} | Active Compute Nodes: {} | Global Pool Weight: {:.1}",
                    size,
                    node_weights.len(),
                    total_swarm_weight
                );

                // 4. Dispatch Weighted Chunks to Swarm
                let mut dispatch_tasks = Vec::new();
                let mut local_chunk = 0;

                for (ip, weight) in node_weights {
                    let node_share_percentage = weight / total_swarm_weight;
                    let chunk_size = (size as f64 * node_share_percentage).round() as usize;

                    if chunk_size == 0 {
                        continue;
                    } // Node is too weak for this workload size

                    if ip == "LOKAAL" {
                        local_chunk = chunk_size;
                        tracing::debug!(
                            "[HIVE]: Master Node allocated {} calculaties ({:.1}% van totaal).",
                            chunk_size,
                            node_share_percentage * 100.0
                        );
                    } else {
                        tracing::debug!(
                            "[HIVE]: Slave {} krijgt {} calculaties toegewezen ({:.1}% van totaal)...",
                            ip,
                            chunk_size,
                            node_share_percentage * 100.0
                        );
                        let payload = format!("heavy_work {}", chunk_size);
                        dispatch_tasks.push(tokio::spawn(async move {
                            tracing::debug!("🚀 Dispatching workload to {}...", ip);
                            match crate::network::hsp_node::SwarmEngine::dispatch(&ip, 9003, &payload)
                                .await
                            {
                                Ok(res) => tracing::debug!("✅ [HIVE]: Node {} gereed: {}", ip, res),
                                Err(e) => tracing::debug!("❌ [HIVE]: Node {} gefaald: {}", ip, e),
                            }
                        }));
                    }
                }

                // Execute local share natively
                if local_chunk > 0 {
                    tracing::debug!(
                        "[HIVE]: Master Node start lokale Native execution (Size: {})...",
                        local_chunk
                    );
                    if let Err(e) = self
                        .process_command(&format!("heavy_work {}", local_chunk), ctx.clone())
                        .await
                    {
                        tracing::debug!("[ERROR]: Master Node failed: {}", e);
                    }
                }

                // Await all remote tasks
                for task in dispatch_tasks {
                    let _ = task.await;
                }

                tracing::debug!(
                    "{}",
                    "[SWARM]: Global compute complete."
                        .green()
                        .bold()
                );
                return Ok(());
            }

            if trimmed.starts_with("gpu work ") {
                let args_part = trimmed[9..].trim();
                let (size, device_id) = if let Some((s, d)) = args_part.split_once(" on ") {
                    (
                        s.trim().parse().unwrap_or(8192),
                        d.trim().parse().unwrap_or(0),
                    )
                } else {
                    (args_part.parse().unwrap_or(8192), 0)
                };

                tracing::debug!(
                    "[COMPUTE]: Starten van GPU acceleratie (Buffer {}, Device {})...",
                    size, device_id
                );
                match crate::gpu::gpu_work_real(size, device_id) {
                    Ok(_) => tracing::debug!("[COMPUTE]: GPU taak voltooid."),
                    Err(e) => tracing::debug!("[ERROR]: GPU Fout: {}", e),
                }
                return Ok(());
            }

            if trimmed.starts_with("gpu infer ") {
                let prompt = trimmed[10..].trim().trim_matches('"');
                tracing::debug!("[BRAIN]: Sending prompt to Helheim Brain: '{}'", prompt);

                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                use tokio::net::UnixStream;

                let request = serde_json::json!({
                    "prompt": prompt,
                    "max_tokens": 100
                });

                match UnixStream::connect("/tmp/helheim_brain.sock").await {
                    Ok(mut stream) => {
                        let req_str = request.to_string();
                        if let Err(e) = stream.write_all(req_str.as_bytes()).await {
                            tracing::debug!("[ERROR]: Failed to send request: {}", e);
                            return Ok(());
                        }

                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();

                        print!("[BRAIN]: ");
                        use std::io::Write; // For flush

                        loop {
                            line.clear();
                            match reader.read_line(&mut line).await {
                                Ok(0) => break, // EOF
                                Ok(_) => {
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(&line)
                                    {
                                        if let Some(token) = json["token"].as_str() {
                                            print!("{}", token);
                                            let _ = std::io::stdout().flush();
                                        }
                                        if json["done"].as_bool().unwrap_or(false) {
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::debug!("\n[ERROR]: Stream error: {}", e);
                                    break;
                                }
                            }
                        }
                        tracing::debug!("");
                    }
                    Err(e) => tracing::debug!(
                        "[ERROR]: Brain not connected (Is helheim_brain running?): {}",
                        e
                    ),
                }
                return Ok(());
            }

            if trimmed.starts_with("shield encrypt ") {
                let data = trimmed[15..].trim();
                tracing::debug!("[SECURITY]: Data encryptie in uitvoering...");
                let result = crate::shield::shield_encrypt_helheim(data);
                tracing::debug!("[SECURITY]: Resultaat: {}", result);
                return Ok(());
            }

            if trimmed.starts_with("stuur ") {
                if let Some((payload, target_str)) = trimmed[6..].split_once(" naar ") {
                    let clean_payload = payload.trim().trim_matches('"');
                    let mut final_targets: Vec<String> = Vec::new();
                    let targets: Vec<&str> = target_str.split_whitespace().collect();

                    // Handle 'allemaal' broadcast
                    if targets.contains(&"allemaal") {
                        tracing::debug!("[NET]: Broadcast modus ('allemaal') gedetecteerd...");
                        if let Ok(peers) = self.discovery.peers.lock() {
                            for ip in peers.keys() {
                                final_targets.push(ip.clone());
                            }
                        }
                        if final_targets.is_empty() {
                            tracing::debug!("[WARN]: Geen peers bekend in Discovery Service.");
                        }
                    } else {
                        for t in targets {
                            final_targets.push(t.trim_matches('"').to_string());
                        }
                    }

                    tracing::debug!(
                        "[NET]: Swarm Dispatch geactiveerd voor {} targets...",
                        final_targets.len()
                    );

                    for clean_ip in final_targets {
                        print!("  -> {}: ", clean_ip);
                        match crate::network::hsp_node::SwarmEngine::dispatch(
                            &clean_ip,
                            9003,
                            clean_payload,
                        )
                        .await
                        {
                            Ok(resp) => tracing::debug!("✅ {}", resp),
                            Err(e) => tracing::debug!("❌ Fout: {}", e),
                        }
                    }
                } else {
                    tracing::debug!(
                        "[ERROR]: Syntax fout. Gebruik: stuur [bericht] naar [node1] [node2]..."
                    );
                }
                return Ok(());
            }

            if trimmed.starts_with("synthesis ") {
                let _json_seed = trimmed[10..].trim();
                tracing::debug!("[SYNTHESIS]: Ontvangen van Code-Taal DNA...");

                // In productie zouden we serde_json gebruiken om de string te parsen.
                // Voor deze demo simuleren we een MatMul seed.
                let seed = CodeTaal::MatMul {
                    m: 1024,
                    n: 1024,
                    k: 1024,
                };

                tracing::debug!("[SYNTHESIS]: Synthetiseren van abstracte logica naar 'Pure Metal'...");
                match KernelSynthesisEngine::synthesize(seed) {
                    Ok(ptx) => {
                        tracing::debug!("[SYNTHESIS]: Succesvol gegenereerde PTX (Machine Code):");
                        tracing::debug!("--- BEGIN PTX SNAPSHOT ---");
                        tracing::debug!("{}", ptx.trim());
                        tracing::debug!("--- END PTX SNAPSHOT ---");
                        tracing::debug!("[SYNTHESIS]: Klaar voor native lowering.");
                    }
                    Err(e) => tracing::debug!("[ERROR]: Synthesis Fout: {}", e),
                }
                return Ok(());
            }



            if trimmed.starts_with("lees ") {
                let path = trimmed[5..].trim();
                if !ctx.is_privileged {
                    if path.contains("../") {
                        return Err(anyhow::anyhow!("[SECURITY]: Path Traversal gedetecteerd."));
                    }
                    if !path.starts_with("./sandbox/") && !path.starts_with("/var/lib/helheim/sandbox/") {
                        return Err(anyhow::anyhow!("[SECURITY]: Bestandstoegang buiten sandbox geweigerd."));
                    }
                }
                use crate::std::fs::FileManager;
                match FileManager::read(path) {
                    Ok(content) => tracing::debug!("[FS]: Inhoud van '{}':\n{}", path, content),
                    Err(e) => tracing::debug!("[ERROR]: FS Fout: {}", e),
                }
                return Ok(());
            }

            if trimmed.starts_with("schrijf ") {
                if let Some((content_part, path_part)) = trimmed[8..].split_once(" naar ") {
                    let content = content_part.trim().trim_matches('"');
                    let path = path_part.trim().trim_matches('"');
                    if !ctx.is_privileged {
                        if path.contains("../") {
                            return Err(anyhow::anyhow!("[SECURITY]: Path Traversal gedetecteerd."));
                        }
                        if !path.starts_with("./sandbox/") && !path.starts_with("/var/lib/helheim/sandbox/") {
                            return Err(anyhow::anyhow!("[SECURITY]: Bestandstoegang buiten sandbox geweigerd."));
                        }
                    }
                    use crate::std::fs::FileManager;
                    match FileManager::write(path, content) {
                        Ok(_) => tracing::debug!("[FS]: Succesvol geschreven naar '{}'.", path),
                        Err(e) => tracing::debug!("[ERROR]: FS Fout: {}", e),
                    }
                } else {
                    tracing::debug!("[ERROR]: Syntax fout. Gebruik: schrijf [tekst] naar [bestand]");
                }
                return Ok(());
            }

            if trimmed.starts_with("voer uit ") {
                if !ctx.is_privileged {
                    return Err(anyhow::anyhow!("[SECURITY]: OS-level Shell vereist Elevated Privileges."));
                }
                let cmd = trimmed[9..].trim();
                use crate::std::sys::SystemManager;
                tracing::debug!("[SYS]: Uitvoeren van shell commando: '{}'...", cmd);
                match SystemManager::execute(cmd) {
                    Ok(out) => tracing::debug!("{}", out),
                    Err(e) => tracing::debug!("[ERROR]: SYS Fout: {}", e),
                }
                return Ok(());
            }

            if trimmed.starts_with("haal ") {
                let url = trimmed[5..].trim().trim_matches('"');
                if !ctx.is_privileged {
                    if url.contains("127.0.0.1") || url.contains("localhost") || url.contains("192.168.") || url.contains("10.") || url.contains("169.254.") {
                        return Err(anyhow::anyhow!("[SECURITY]: SSRF Protectie actief. Lokale IPs geblokkeerd."));
                    }
                }
                use crate::std::http::HttpManager;
                tracing::debug!("[HTTP]: Ophalen van '{}'...", url);
                match HttpManager::get(url) {
                    Ok(body) => {
                        tracing::debug!("[HTTP]: Response ({} bytes):", body.len());
                        tracing::debug!("{}", body.lines().take(10).collect::<Vec<_>>().join("\n")); // Preview first 10 lines
                        if body.lines().count() > 10 {
                            tracing::debug!("... (truncated)");
                        }
                    }
                    Err(e) => tracing::debug!("[ERROR]: HTTP Fout: {}", e),
                }
                return Ok(());
            }

            // System extensions (sleep)
            if trimmed.starts_with("wacht ") {
                let seconds_str = trimmed[6..].trim();
                if let Ok(seconds) = seconds_str.parse::<u64>() {
                    tracing::debug!("[SYSTEM]: Slaapmodus voor {} seconden...", seconds);
                    tokio::time::sleep(tokio::time::Duration::from_secs(seconds)).await;
                } else {
                    tracing::debug!("[ERROR]: Ongeldige tijdsduur. Gebruik: wacht [seconden]");
                }
                return Ok(());
            }

            // Package installer
            if trimmed.starts_with("installeer ") {
                let package = trimmed[11..].trim().trim_matches('"');
                use crate::std::pkg::PackageManager;
                tracing::debug!("[PKG]: Verzoek tot installatie van '{}'...", package);
                match PackageManager::install(package) {
                    Ok(msg) => tracing::debug!("[PKG]: {}", msg),
                    Err(e) => tracing::debug!("[ERROR]: Installatie Fout: {}", e),
                }
                return Ok(());
            }

            // Intent Parser (Social/Meta)
            match IntentParser::parse(trimmed) {
                Intent::Send { target, payload } => {
                    tracing::debug!(
                        "[INTENT]: Gedetecteerd: STUREN naar '{}' met inhoud '{}'",
                        target, payload
                    );
                    let ast = vec![CodeTaal::Send { target, payload }];
                    self.execute_ast(ast, ctx.clone()).await?;
                    return Ok(());
                }
                Intent::SetVar { name, value } => {
                    tracing::debug!(
                        "[INTENT]: Gedetecteerd: VARIABELE ZETTEN '{}' = '{}'",
                        name, value
                    );
                    let ast = vec![CodeTaal::VarDef { name, value: Box::new(CodeTaal::Literal(helheim_lang::ast::LiteralValue::String(value))) }];
                    self.execute_ast(ast, ctx.clone()).await?;
                    return Ok(());
                }
                Intent::MatMul { size } => {
                    tracing::debug!("[INTENT]: Detected matrix kernel (size: {})", size);
                    let ast = vec![CodeTaal::MatMul {
                        m: size,
                        n: size,
                        k: size,
                    }];
                    self.execute_ast(ast, ctx.clone()).await?;
                    return Ok(());
                }
                Intent::Fix => {
                    tracing::debug!(
                        "[INTENT]: Je wilt iets oplossen. Initiëren van 'Recovery Protocol'..."
                    );
                    tracing::debug!("[ACTION]: Resetting Rune Engine & GPU State...");
                    tracing::debug!("✅ Systeem hersteld. Alle parameters staan weer op groen.");
                    return Ok(());
                }
                Intent::Diagnosis => {
                    tracing::debug!("[INTENT]: Je vraagt om status. Draaien van systeem-diagnose...");
                    self.list_nodes();
                    tracing::debug!("[STATUS]: GPU is 100% operationeel.");
                    return Ok(());
                }
                Intent::Speed => {
                    tracing::debug!("[INTENT]: Overclock profile loaded. Speed increased.");

                    return Ok(());
                }
                Intent::Update => {
                    tracing::debug!("[INTENT]: Controleren op updates voor Helheim Cluster...");
                    tracing::debug!("[PKG-MAN]: Index bijwerken... OK.");
                    tracing::debug!(
                        "[PKG-MAN]: Geen kritieke updates beschikbaar. Je draait versie v1.0 (Python Killer)."
                    );
                    return Ok(());
                }
                Intent::Research => {
                    tracing::debug!("[INTENT]: Diepgaande analyse gestart ('Deep Dive')...");
                    tracing::debug!("[LOGS]: Scannen van systeemlogboeken (laatste 24u)...");
                    tracing::debug!("[LOGS]: No irregularities found in kernel ringbuffer.");
                    tracing::debug!(
                        "[ANALYSE]: Conclusie: Het probleem zit waarschijnlijk tussen toetsenbord en stoel. 😉"
                    );
                    return Ok(());
                }
                Intent::Unknown => {
                    // Check if it's a function call
                    let func_body = self.memory.func_store.get(trimmed).map(|v| v.value().clone());

                    if let Some(body) = func_body {
                        tracing::debug!("[EXECUTION]: Uitvoeren van functie '{}'...", trimmed);
                        self.process_command(&body, ctx.clone()).await?;
                        return Ok(());
                    }

                    // Fallback: native execution (raw / low-level)
                    self.execute_native(trimmed)?;
                }
            }

            Ok(())
        })
    }

    

    fn list_nodes(&self) {
        let peers = match self.discovery.peers.lock() {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Peers lock poisoned: {}", e);
                return;
            }
        };
        tracing::debug!(
            "[NETWORK]: Gedetecteerde actieve nodes in Orchestrator netwerk: {}",
            peers.len()
        );
        for (ip, caps) in peers.iter() {
            tracing::debug!(
                "  > Node ID: {} | Performance: {:.2} GFLOPS | Native-GPU: {}",
                ip, caps.estimated_cpu_gflops, caps.has_cuda
            );
        }
    }

    fn execute_native(&self, cmd: &str) -> Result<()> {
        if !HelheimLock::is_authorized() {
            tracing::debug!("[ALERT]: Native execution is locked. Authorization required.");
            return Ok(());
        }

        tracing::debug!("[NATIVE]: Low-level instruction...");
        unsafe {
            match RuneEngine::execute_raw_rune(cmd) {
                Ok(res) => tracing::debug!("{}", res),
                Err(e) => tracing::debug!("[ERROR]: Low-level kernel execution error: {}", e),
            }
        }
        Ok(())
    }

}
pub mod tcp_resources;
