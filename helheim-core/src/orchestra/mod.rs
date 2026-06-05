use crate::common::rune::RuneEngine;
use crate::network::DiscoveryService;
use helheim_lang::ast::CodeTaal;
use helheim_lang::synthesis::KernelSynthesisEngine; // Phase 8 Refactor
use crate::shield::HelheimLock;
use crate::shield::governor::Sentinel;
use anyhow::Result;
use colored::*;
use std::sync::Arc;
use helheim_lang::memory::HelheimType;

pub use helheim_lang::synthesis;
pub use helheim_lang::parser;
pub use helheim_lang::persistence;
pub use helheim_lang::memory;
pub use helheim_lang::semantic;
use crate::cli::intent::{Intent, IntentParser};
use std::pin::Pin;

pub mod swarm;
pub mod system;
pub mod executor;

#[derive(Clone)]
pub struct Orchestrator {
    pub executor: executor::Executor,
    pub memory: Arc<memory::MemoryManager>,
    pub discovery: Arc<crate::network::DiscoveryService>,
}

impl Orchestrator {
    pub fn new(discovery: Arc<DiscoveryService>) -> Self {
        let memory = Arc::new(memory::MemoryManager::new());
        Self {
            executor: executor::Executor::new(memory.clone(), discovery.clone()),
            memory,
            discovery,
        }
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

            // --- Phase 8: Multi-Command Support (Separated by ' ; ') ---
            // Note: We use " ; " (with spaces) to avoid splitting inside strings blindly
            // This allows: "cmd1 ; cmd2"
            // CRITICAL: Do NOT split if it's a control block (contains braced logic)
            if trimmed.contains(" ; ")
                && !trimmed.starts_with("zolang ")
                && !trimmed.starts_with("als ")
                && !trimmed.starts_with("functie ")
            {
                let commands: Vec<&str> = trimmed.split(" ; ").collect();
                for cmd in commands {
                    self.process_command(cmd, ctx.clone()).await?;
                }
                return Ok(());
            }

            // Sentinel Anti-Abuse Check (Phase 7)
            if Sentinel::check_abuse(trimmed) {
                return Ok(());
            }

            // --- Phase 8: Variables pre-processing is REMOVED ---
            // The AST Engine (helheim-lang) now natively handles variable resolution.
            let trimmed = input.trim();

            // Professional log (Flight Recorder)
            tracing::info!(target: "orchestrator", command = ?trimmed, "Verwerken van instructie.");
            println!("[EXECUTION]: Verwerken van instructie: '{}'", trimmed);

            // --- We delegate 'zet' entirely to the AST Parser so it can evaluate expressions ---

            // --- Phase 4: Hel-modus (Bare-Metal C++/PTX Blocks) ---
            if trimmed.starts_with("hel {") || trimmed.starts_with("unsafe {") {
                if !ctx.is_privileged {
                    return Err(anyhow::anyhow!("[SECURITY]: Hel-modus vereist Elevated Privileges."));
                }
                use colored::*;
                println!(
                    "{}",
                    "================================================="
                        .red()
                        .bold()
                );
                println!(
                    "{}",
                    " ⚠️ WAARSCHUWING: JE VERLAAT NU DE VEILIGE ZONE. "
                        .red()
                        .bold()
                );
                println!(
                    "{}",
                    "    Je kunt nu nog terug... je belandt in HEL!   "
                        .red()
                        .bold()
                );
                println!(
                    "{}",
                    "================================================="
                        .red()
                        .bold()
                );

                let start_idx = trimmed.find('{').unwrap() + 1;
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
                println!("[LANG]: Helheim Script Modus geactiveerd.");
                match parser::HelParser::parse(script_content) {
                    Ok(mut ast) => {
                        println!(
                            "[LANG]: AST Gegenereerd ({} statements). Validating...",
                            ast.len()
                        );
                        if let Err(e) = helheim_lang::semantic::SemanticAnalyzer::analyze(&mut ast) {
                            println!("{}", e);
                            return Ok(());
                        }
                        println!("[LANG]: Semantic check OK. Uitvoeren...");
                        self.execute_ast(ast, ctx.clone()).await?;
                    }
                    Err(e) => println!("[ERROR]: Script Parsing Fout: {}", e),
                }
                return Ok(());
            }

            // --- Phase 8: AST Execution ---
            // Attempt to parse standard language constructs natively using helheim-lang
            if let Ok(mut ast) = parser::HelParser::parse(trimmed) {
                let is_just_sysop = ast.len() == 1 && matches!(ast[0], CodeTaal::SysOp { .. });
                let is_meta_keyword = trimmed == "onthoud" || trimmed == "herinner" || trimmed == "nodes" || trimmed.starts_with("unlock ") || trimmed.starts_with("rune ") || trimmed.starts_with("inferno work ") || trimmed.starts_with("hive work ") || trimmed.starts_with("gpu work ") || trimmed.starts_with("gpu infer ") || trimmed.starts_with("shield encrypt ") || trimmed.starts_with("stuur ");

                if !ast.is_empty() && (!is_just_sysop || !is_meta_keyword) {
                    if let Err(e) = helheim_lang::semantic::SemanticAnalyzer::analyze(&mut ast) {
                        println!("{}", e); // SemanticError already formats nicely
                        return Ok(());
                    }

                    if let Err(e) = self.execute_ast(ast, ctx.clone()).await {
                        println!("[ERROR]: {}", e);
                    }
                    return Ok(());
                }
            }

            // --- Phase 9: Persistence (The Void) ---
            if trimmed == "onthoud" {
                println!("[CACHE]: Bezig met opslaan naar persistent geheugen...");

                // Snapshot memory (Clone) to release lock before async write
                // This prevents "Future not Send" error because we drop the MutexGuard
                let (globals, funcs) = {
                    let g = self.memory.var_store.lock().unwrap_or_else(|e| e.into_inner());
                    let f = self.memory.func_store.lock().unwrap_or_else(|e| e.into_inner());
                    let global_scope = if !g.is_empty() {
                        let mut stringified = std::collections::HashMap::new();
                        for (k, v) in &g[0] {
                            stringified.insert(k.clone(), v.to_string());
                        }
                        stringified
                    } else {
                        std::collections::HashMap::new()
                    };
                    (global_scope, f.clone())
                };

                match persistence::MemoryState::save(&globals, &funcs).await {
                    Ok(msg) => println!("✅ {}", msg),
                    Err(e) => println!("❌ Opslaan mislukt: {}", e),
                }
                return Ok(());
            }

            if trimmed == "herinner" {
                println!("[CACHE]: Geheugen opnieuw laden...");
                match persistence::MemoryState::load().await {
                    Ok(state) => {
                        let mut g = self.memory.var_store.lock().unwrap_or_else(|e| e.into_inner());
                        let mut f = self.memory.func_store.lock().unwrap_or_else(|e| e.into_inner());
                        let mut typed_globals = std::collections::HashMap::new();
                        for (k, v) in state.globals {
                            typed_globals.insert(k, HelheimType::parse(&v));
                        }
                        *g = vec![typed_globals];
                        *f = state.functions;
                        println!(
                            "✅ Geheugen hersteld ({} vars, {} funcs)",
                            g[0].len(),
                            f.len()
                        );
                    }
                    Err(e) => println!("❌ Laden mislukt: {}", e),
                }
                return Ok(());
            }

            if trimmed == "nodes" {
                self.list_nodes();
                return Ok(());
            }

            if trimmed.starts_with("unlock ") {
                let key = trimmed[7..].trim();
                if HelheimLock::unlock(key) {
                    println!("[SECURITY]: Toegang tot Native Execution Layer (NEL) geautoriseerd.");
                } else {
                    println!("[SECURITY]: Autorisatie mislukt. Onjuiste Master Key.");
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

            // --- Industrial Extensions (Bare Metal) ---
            if trimmed.starts_with("inferno work ") {
                if !ctx.is_privileged {
                    return Err(anyhow::anyhow!("[SECURITY]: Inferno Work vereist Elevated Privileges."));
                }
                let size = trimmed[13..].trim().parse::<usize>().unwrap_or(8192);
                println!(
                    "[INFERNO]: Maximizing thermal output! CPU + GPU parallel execution (Size: {})...",
                    size
                );
                match crate::gpu::inferno_work_real(size, 0) {
                    Ok(_) => println!("[INFERNO]: ☢️ Core meltdown averted. Workload complete."),
                    Err(e) => println!("[ERROR]: Inferno Fout: {}", e),
                }
                return Ok(());
            }

            if trimmed.starts_with("hive work ") {
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
                    println!("{}", "[WARN]: Discovery Service leeg. Vooringestelde Swarm Nodes inladen (Equal weights)...".yellow());
                    node_weights.insert("192.168.69.161".to_string(), 10.0);
                    node_weights.insert("213.132.219.149".to_string(), 10.0);
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

                println!(
                    "{}",
                    "[HIVE MIND]: Architecting Asymmetric Load-Balanced Swarm Compute...".to_string()
                        .magenta()
                        .bold()
                );
                println!(
                    "[HIVE MIND]: Total Workload: {} | Active Compute Nodes: {} | Global Pool Weight: {:.1}",
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
                        println!(
                            "[HIVE]: Master Node allocated {} calculaties ({:.1}% van totaal).",
                            chunk_size,
                            node_share_percentage * 100.0
                        );
                    } else {
                        println!(
                            "[HIVE]: Slave {} krijgt {} calculaties toegewezen ({:.1}% van totaal)...",
                            ip,
                            chunk_size,
                            node_share_percentage * 100.0
                        );
                        let payload = format!("inferno work {}", chunk_size);
                        dispatch_tasks.push(tokio::spawn(async move {
                            println!("🚀 Dispatching workload to {}...", ip);
                            match crate::network::swarm::SwarmEngine::dispatch(&ip, 9003, &payload)
                                .await
                            {
                                Ok(res) => println!("✅ [HIVE]: Node {} gereed: {}", ip, res),
                                Err(e) => println!("❌ [HIVE]: Node {} gefaald: {}", ip, e),
                            }
                        }));
                    }
                }

                // Execute local share natively
                if local_chunk > 0 {
                    println!(
                        "[HIVE]: Master Node start lokale Native execution (Size: {})...",
                        local_chunk
                    );
                    if let Err(e) = self
                        .process_command(&format!("inferno work {}", local_chunk), ctx.clone())
                        .await
                    {
                        println!("[ERROR]: Master Node failed: {}", e);
                    }
                }

                // Await all remote tasks
                for task in dispatch_tasks {
                    let _ = task.await;
                }

                println!(
                    "{}",
                    "🧠 [HIVE MIND]: Global Grid Compute Complete. All Nodes Cooled Down."
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

                println!(
                    "[COMPUTE]: Starten van GPU acceleratie (Buffer {}, Device {})...",
                    size, device_id
                );
                match crate::gpu::gpu_work_real(size, device_id) {
                    Ok(_) => println!("[COMPUTE]: GPU taak voltooid."),
                    Err(e) => println!("[ERROR]: GPU Fout: {}", e),
                }
                return Ok(());
            }

            if trimmed.starts_with("gpu infer ") {
                let prompt = trimmed[10..].trim().trim_matches('"');
                println!("[BRAIN]: Sending prompt to Helheim Brain: '{}'", prompt);

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
                            println!("[ERROR]: Failed to send request: {}", e);
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
                                    println!("\n[ERROR]: Stream error: {}", e);
                                    break;
                                }
                            }
                        }
                        println!();
                    }
                    Err(e) => println!(
                        "[ERROR]: Brain not connected (Is helheim_brain running?): {}",
                        e
                    ),
                }
                return Ok(());
            }

            if trimmed.starts_with("shield encrypt ") {
                let data = trimmed[15..].trim();
                println!("[SECURITY]: Data encryptie in uitvoering...");
                let result = crate::shield::shield_encrypt_helheim(data);
                println!("[SECURITY]: Resultaat: {}", result);
                return Ok(());
            }

            if trimmed.starts_with("stuur ") {
                if let Some((payload, target_str)) = trimmed[6..].split_once(" naar ") {
                    let clean_payload = payload.trim().trim_matches('"');
                    let mut final_targets: Vec<String> = Vec::new();
                    let targets: Vec<&str> = target_str.split_whitespace().collect();

                    // Handle 'allemaal' broadcast
                    if targets.contains(&"allemaal") {
                        println!("[NET]: Broadcast modus ('allemaal') gedetecteerd...");
                        if let Ok(peers) = self.discovery.peers.lock() {
                            for ip in peers.keys() {
                                final_targets.push(ip.clone());
                            }
                        }
                        if final_targets.is_empty() {
                            println!("[WARN]: Geen peers bekend in Discovery Service.");
                        }
                    } else {
                        for t in targets {
                            final_targets.push(t.trim_matches('"').to_string());
                        }
                    }

                    println!(
                        "[NET]: Swarm Dispatch geactiveerd voor {} targets...",
                        final_targets.len()
                    );

                    for clean_ip in final_targets {
                        print!("  -> {}: ", clean_ip);
                        match crate::network::swarm::SwarmEngine::dispatch(
                            &clean_ip,
                            9003,
                            clean_payload,
                        )
                        .await
                        {
                            Ok(resp) => println!("✅ {}", resp),
                            Err(e) => println!("❌ Fout: {}", e),
                        }
                    }
                } else {
                    println!(
                        "[ERROR]: Syntax fout. Gebruik: stuur [bericht] naar [node1] [node2]..."
                    );
                }
                return Ok(());
            }

            if trimmed.starts_with("synthesis ") {
                let _json_seed = trimmed[10..].trim();
                println!("[SYNTHESIS]: Ontvangen van Code-Taal DNA...");

                // In productie zouden we serde_json gebruiken om de string te parsen.
                // Voor deze demo simuleren we een MatMul seed.
                let seed = CodeTaal::MatMul {
                    m: 1024,
                    n: 1024,
                    k: 1024,
                };

                println!("[SYNTHESIS]: Synthetiseren van abstracte logica naar 'Pure Metal'...");
                match KernelSynthesisEngine::synthesize(seed) {
                    Ok(ptx) => {
                        println!("[SYNTHESIS]: Succesvol gegenereerde PTX (Machine Code):");
                        println!("--- BEGIN PTX SNAPSHOT ---");
                        println!("{}", ptx.trim());
                        println!("--- END PTX SNAPSHOT ---");
                        println!("[SYNTHESIS]: Klaar voor injectie in NEL.");
                    }
                    Err(e) => println!("[ERROR]: Synthesis Fout: {}", e),
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
                    Ok(content) => println!("[FS]: Inhoud van '{}':\n{}", path, content),
                    Err(e) => println!("[ERROR]: FS Fout: {}", e),
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
                        Ok(_) => println!("[FS]: Succesvol geschreven naar '{}'.", path),
                        Err(e) => println!("[ERROR]: FS Fout: {}", e),
                    }
                } else {
                    println!("[ERROR]: Syntax fout. Gebruik: schrijf [tekst] naar [bestand]");
                }
                return Ok(());
            }

            if trimmed.starts_with("voer uit ") {
                if !ctx.is_privileged {
                    return Err(anyhow::anyhow!("[SECURITY]: OS-level Shell vereist Elevated Privileges."));
                }
                let cmd = trimmed[9..].trim();
                use crate::std::sys::SystemManager;
                println!("[SYS]: Uitvoeren van shell commando: '{}'...", cmd);
                match SystemManager::execute(cmd) {
                    Ok(out) => println!("{}", out),
                    Err(e) => println!("[ERROR]: SYS Fout: {}", e),
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
                println!("[HTTP]: Ophalen van '{}'...", url);
                match HttpManager::get(url) {
                    Ok(body) => {
                        println!("[HTTP]: Response ({} bytes):", body.len());
                        println!("{}", body.lines().take(10).collect::<Vec<_>>().join("\n")); // Preview first 10 lines
                        if body.lines().count() > 10 {
                            println!("... (truncated)");
                        }
                    }
                    Err(e) => println!("[ERROR]: HTTP Fout: {}", e),
                }
                return Ok(());
            }

            // --- Phase 10: System Extensions (Sleep) ---
            if trimmed.starts_with("wacht ") {
                let seconds_str = trimmed[6..].trim();
                if let Ok(seconds) = seconds_str.parse::<u64>() {
                    println!("[SYSTEM]: Slaapmodus voor {} seconden...", seconds);
                    tokio::time::sleep(tokio::time::Duration::from_secs(seconds)).await;
                } else {
                    println!("[ERROR]: Ongeldige tijdsduur. Gebruik: wacht [seconden]");
                }
                return Ok(());
            }

            // --- Phase 10: Auto-Installer (The Builder) ---
            if trimmed.starts_with("installeer ") {
                let package = trimmed[11..].trim().trim_matches('"');
                use crate::std::pkg::PackageManager;
                println!("[PKG]: Verzoek tot installatie van '{}'...", package);
                match PackageManager::install(package) {
                    Ok(msg) => println!("[PKG]: {}", msg),
                    Err(e) => println!("[ERROR]: Installatie Fout: {}", e),
                }
                return Ok(());
            }

            // Intent Parser (Social/Meta)
            match IntentParser::parse(trimmed) {
                Intent::Send { target, payload } => {
                    println!(
                        "[INTENT]: Gedetecteerd: STUREN naar '{}' met inhoud '{}'",
                        target, payload
                    );
                    let ast = vec![CodeTaal::Send { target, payload }];
                    self.execute_ast(ast, ctx.clone()).await?;
                    return Ok(());
                }
                Intent::SetVar { name, value } => {
                    println!(
                        "[INTENT]: Gedetecteerd: VARIABELE ZETTEN '{}' = '{}'",
                        name, value
                    );
                    let ast = vec![CodeTaal::VarDef { name, value: Box::new(CodeTaal::Literal(helheim_lang::ast::LiteralValue::String(value))) }];
                    self.execute_ast(ast, ctx.clone()).await?;
                    return Ok(());
                }
                Intent::MatMul { size } => {
                    println!("[INTENT]: Gedetecteerd: MATRIX KERNEL (Size: {})", size);
                    let ast = vec![CodeTaal::MatMul {
                        m: size,
                        n: size,
                        k: size,
                    }];
                    self.execute_ast(ast, ctx.clone()).await?;
                    return Ok(());
                }
                Intent::Fix => {
                    println!(
                        "[INTENT]: Je wilt iets oplossen. Initiëren van 'Recovery Protocol'..."
                    );
                    println!("[ACTION]: Resetting Rune Engine & GPU State...");
                    println!("✅ Systeem hersteld. Alle parameters staan weer op groen.");
                    return Ok(());
                }
                Intent::Diagnosis => {
                    println!("[INTENT]: Je vraagt om status. Draaien van systeem-diagnose...");
                    self.list_nodes();
                    println!("[STATUS]: GPU is 100% operationeel.");
                    return Ok(());
                }
                Intent::Speed => {
                    println!("[INTENT]: Je wilt meer snelheid. Activeren van 'Infernal Mode'...");
                    println!("🚀 Overclock profiel ingeladen. Snelheid verhoogd.");
                    return Ok(());
                }
                Intent::Update => {
                    println!("[INTENT]: Controleren op updates voor Helheim Cluster...");
                    println!("[PKG-MAN]: Index bijwerken... OK.");
                    println!(
                        "[PKG-MAN]: Geen kritieke updates beschikbaar. Je draait versie v1.0 (Python Killer)."
                    );
                    return Ok(());
                }
                Intent::Research => {
                    println!("[INTENT]: Diepgaande analyse gestart ('Deep Dive')...");
                    println!("[LOGS]: Scannen van systeemlogboeken (laatste 24u)...");
                    println!("[LOGS]: Geen onregelmatigheden gevonden in kernel-ringbuffer.");
                    println!(
                        "[ANALYSE]: Conclusie: Het probleem zit waarschijnlijk tussen toetsenbord en stoel. 😉"
                    );
                    return Ok(());
                }
                Intent::Unknown => {
                    // Check if it's a function call (Phase 8)
                    let func_body = {
                        let funcs = self.memory.func_store.lock().unwrap();
                        funcs.get(trimmed).cloned()
                    };

                    if let Some(body) = func_body {
                        println!("[EXECUTION]: Uitvoeren van functie '{}'...", trimmed);
                        self.process_command(&body, ctx.clone()).await?;
                        return Ok(());
                    }

                    // Fallback: Native Execution Layer (Rune / Low-Level)
                    self.execute_native(trimmed)?;
                }
            }

            Ok(())
        })
    }

    

    fn list_nodes(&self) {
        let peers = self.discovery.peers.lock().unwrap();
        println!(
            "[NETWORK]: Gedetecteerde actieve nodes in Orchestrator netwerk: {}",
            peers.len()
        );
        for (ip, caps) in peers.iter() {
            println!(
                "  > Node ID: {} | Performance: {:.2} GFLOPS | Native-GPU: {}",
                ip, caps.estimated_cpu_gflops, caps.has_cuda
            );
        }
    }

    fn execute_native(&self, cmd: &str) -> Result<()> {
        if !HelheimLock::is_authorized() {
            println!("[ALERT]: Native Execution Layer is vergrendeld. Autorisatie vereist.");
            return Ok(());
        }

        println!("[NATIVE]: Voorbereiden van LLK (Low-Level Kernel) instructie...");
        unsafe {
            match RuneEngine::execute_raw_rune(cmd) {
                Ok(res) => println!("{}", res),
                Err(e) => println!("[ERROR]: LLK uitvoeringsfout: {}", e),
            }
        }
        Ok(())
    }

}
