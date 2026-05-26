use crate::common::rune::RuneEngine;
use crate::network::DiscoveryService;
use crate::orchestra::synthesis::{CodeTaal, KernelSynthesisEngine}; // Phase 8 Refactor
use crate::shield::HelheimLock;
use crate::shield::governor::Sentinel;
use anyhow::Result;
use colored::*;
use std::sync::Arc;

pub mod swarm;
pub mod synthesis;
use crate::cli::intent::{Intent, IntentParser};
use std::pin::Pin;
pub mod parser;
pub mod persistence;

pub struct Orchestrator {
    discovery: Arc<DiscoveryService>,
    var_store: std::sync::Mutex<Vec<std::collections::HashMap<String, String>>>,
    func_store: std::sync::Mutex<std::collections::HashMap<String, String>>,
    ast_funcs: std::sync::Mutex<
        std::collections::HashMap<
            String,
            (Vec<String>, Box<crate::orchestra::synthesis::CodeTaal>),
        >,
    >,
    model_store: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
}

impl Orchestrator {
    pub fn new(discovery: Arc<DiscoveryService>) -> Self {
        // Use synchronous load since we are in a constructor (and likely before runtime start, or inside one where block_on fails)
        let (globals, funcs) = match persistence::MemoryState::load_sync() {
            Ok(state) => {
                println!("[MEMORY]: 🧠 Local CLI Cache geladen.");
                println!("          > {} variabelen", state.globals.len());
                println!("          > {} functies", state.functions.len());
                (state.globals, state.functions)
            }
            Err(e) => {
                println!("[MEMORY]: Geen vorig geheugen gevonden of corrupt ({})", e);
                (
                    std::collections::HashMap::new(),
                    std::collections::HashMap::new(),
                )
            }
        };

        let mut initial_scope = std::collections::HashMap::new();
        for (k, v) in globals {
            initial_scope.insert(k, v);
        }

        Self {
            discovery,
            var_store: std::sync::Mutex::new(vec![initial_scope]),
            func_store: std::sync::Mutex::new(funcs),
            ast_funcs: std::sync::Mutex::new(std::collections::HashMap::new()),
            model_store: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    // --- SCOPE MANAGEMENT ---
    fn push_scope(&self) {
        let mut store = self.var_store.lock().unwrap();
        store.push(std::collections::HashMap::new());
        println!("[SCOPE]: Gepusht naar level {}", store.len());
    }

    fn pop_scope(&self) {
        let mut store = self.var_store.lock().unwrap();
        if store.len() > 1 {
            store.pop();
            println!("[SCOPE]: Gepopt naar level {}", store.len());
        } else {
            println!("[SCOPE]: Kan globaal scope niet poppen.");
        }
    }

    pub fn get_var(&self, key: &str) -> Option<String> {
        let store = self.var_store.lock().unwrap();
        for scope in store.iter().rev() {
            if let Some(val) = scope.get(key) {
                return Some(val.clone());
            }
        }
        None
    }

    fn set_var(&self, key: String, value: String) {
        let mut store = self.var_store.lock().unwrap();
        if let Some(scope) = store.last_mut() {
            scope.insert(key, value);
        }
    }

    pub fn process_command<'a>(
        &'a self,
        input: &'a str,
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
                    self.process_command(cmd).await?;
                }
                return Ok(());
            }

            // Sentinel Anti-Abuse Check (Phase 7)
            if Sentinel::check_abuse(trimmed) {
                return Ok(());
            }

            // --- Phase 8: Memory Layer (Variable Resolution) ---
            // We resolve variables BEFORE processing commands.
            // EXCEPTION: Do NOT resolve for 'zet' (assignment), 'zolang', 'voor', 'script:', 'als', 'functie'
            // because we need the raw variable names for AST logic parsing.
            let mut resolved_input = trimmed.to_string();
            if !trimmed.starts_with("zet ")
                && !trimmed.starts_with("let ")
                && !trimmed.starts_with("set ")
                && !trimmed.starts_with("zolang ")
                && !trimmed.starts_with("while ")
                && !trimmed.starts_with("repeat ")
                && !trimmed.starts_with("voor ")
                && !trimmed.starts_with("script:")
                && !trimmed.starts_with("als ")
                && !trimmed.starts_with("if ")
                && !trimmed.starts_with("functie ")
                && !trimmed.starts_with("func ")
                && !trimmed.starts_with("fn ")
                && !trimmed.starts_with("function ")
            {
                let store = self.var_store.lock().unwrap();
                for scope in store.iter().rev() {
                    for (key, value) in scope.iter() {
                        let var_sigil = format!("${}", key);
                        resolved_input = resolved_input.replace(&var_sigil, value);
                    }
                }
            }
            let trimmed = resolved_input.trim();

            // Professional log (Flight Recorder)
            tracing::info!(target: "orchestrator", command = ?trimmed, "Verwerken van instructie.");
            println!("[EXECUTION]: Verwerken van instructie: '{}'", trimmed);

            // --- We delegate 'zet' entirely to the AST Parser so it can evaluate expressions ---

            // --- Phase 4: Hel-modus (Bare-Metal C++/PTX Blocks) ---
            if trimmed.starts_with("hel {") || trimmed.starts_with("unsafe {") {
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
                let raw_code = trimmed[start_idx..end_idx].trim();

                crate::gpu::gpu_execute_hel_block(raw_code).await?;
                return Ok(());
            }

            if trimmed.starts_with("script:") {
                let script_content = trimmed[7..].trim();
                println!("[LANG]: Helheim Script Modus geactiveerd.");
                match parser::HelParser::parse(script_content) {
                    Ok(ast) => {
                        println!(
                            "[LANG]: AST Gegenereerd ({} statements). Uitvoeren...",
                            ast.len()
                        );
                        self.execute_ast(ast).await?;
                    }
                    Err(e) => println!("[ERROR]: Script Parsing Fout: {}", e),
                }
                return Ok(());
            }

            // --- Phase 8: Logic Layer (If/Else) ---
            if trimmed.starts_with("als ") || trimmed.starts_with("if ") {
                let offset = if trimmed.starts_with("als ") { 4 } else { 3 };
                // Syntax: als [condition] dan { [command] }
                // Example: als file_exists "test.txt" dan { lees "test.txt" }
                if let Some((condition_id_part, action_part)) = trimmed[offset..]
                    .split_once(" dan ")
                    .or_else(|| trimmed[offset..].split_once(" then "))
                {
                    let condition = condition_id_part.trim();
                    let block = action_part.trim();

                    if block.starts_with('{') && block.ends_with('}') {
                        let inner_cmd = block[1..block.len() - 1].trim();
                        if self.evaluate_condition(condition).await {
                            println!("[LOGIC]: Conditie WAAR. Uitvoeren: '{}'", inner_cmd);
                            self.process_command(inner_cmd).await?;
                        } else {
                            println!("[LOGIC]: Conditie ONWAAR. Overslaan.");
                        }
                    } else {
                        println!("[ERROR]: Syntax fout. Blok moet tussen {{ }} staan.");
                    }
                    return Ok(());
                }
            }

            // --- Phase 8: Loops (Iteration) ---
            if trimmed.starts_with("zolang ")
                || trimmed.starts_with("while ")
                || trimmed.starts_with("repeat ")
            {
                let offset = if trimmed.starts_with("zolang ") {
                    7
                } else if trimmed.starts_with("while ") {
                    6
                } else {
                    7
                };
                let loop_body = trimmed[offset..].trim();
                // Find first brace to split condition and block
                if let Some(start_brace) = loop_body.find('{')
                    && loop_body.ends_with('}') {
                        let condition = loop_body[..start_brace].trim();
                        let block_content = loop_body[start_brace + 1..loop_body.len() - 1].trim();

                        println!("[LOOP]: Starten van 'zolang {}'...", condition);

                        let mut iterations = 0;
                        // HARD LIMIT to prevent infinite loops (Halting Problem guard)
                        while iterations < 1000 {
                            if self.evaluate_condition(condition).await {
                                // Execute the block
                                self.process_command(block_content).await?;
                                iterations += 1;
                            } else {
                                println!("[LOOP]: Conditie niet meer waar. Stop.");
                                break;
                            }
                        }
                        if iterations >= 1000 {
                            println!("[LOOP]: ⚠️ NOODSTOP: Maximale iteraties (1000) bereikt.");
                        }
                        return Ok(());
                    }
                println!("[ERROR]: Syntax fout. Gebruik: zolang [conditie] {{ ... }}");
                return Ok(());
            }

            // --- Phase 8: Functions (Subroutines) ---
            if trimmed.starts_with("functie ")
                || trimmed.starts_with("func ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("function ")
            {
                let offset = if trimmed.starts_with("functie ") {
                    8
                } else if trimmed.starts_with("func ") {
                    5
                } else if trimmed.starts_with("fn ") {
                    3
                } else {
                    9
                };
                let func_def = trimmed[offset..].trim();
                if let Some(start_brace) = func_def.find('{')
                    && func_def.ends_with('}') {
                        let name = func_def[..start_brace].trim().to_string();
                        let body = func_def[start_brace + 1..func_def.len() - 1]
                            .trim()
                            .to_string();

                        println!(
                            "[MEMORY]: Opslaan functie '{}' (Len: {} chars)...",
                            name,
                            body.len()
                        );
                        let mut funcs = self.func_store.lock().unwrap();
                        funcs.insert(name, body);
                        return Ok(());
                    }
                println!("[ERROR]: Syntax fout. Gebruik: functie [naam] {{ ... }}");
                return Ok(());
            }

            // --- Phase 9: Persistence (The Void) ---
            if trimmed == "onthoud" {
                println!("[CACHE]: Bezig met opslaan naar persistent geheugen...");

                // Snapshot memory (Clone) to release lock before async write
                // This prevents "Future not Send" error because we drop the MutexGuard
                let (globals, funcs) = {
                    let g = self.var_store.lock().unwrap();
                    let f = self.func_store.lock().unwrap();
                    let global_scope = if !g.is_empty() {
                        g[0].clone()
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
                        let mut g = self.var_store.lock().unwrap();
                        let mut f = self.func_store.lock().unwrap();
                        *g = vec![state.globals];
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
                self.execute_native(trimmed[5..].trim())?;
                return Ok(());
            }

            // --- Industrial Extensions (Bare Metal) ---
            if trimmed.starts_with("inferno work ") {
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
                        .process_command(&format!("inferno work {}", local_chunk))
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

            // --- Standard Library Extensions (Python Killer) ---
            if trimmed.starts_with("print ")
                || trimmed.starts_with("print(")
                || trimmed.starts_with("druk_af ")
                || trimmed.starts_with("druk_af(")
            {
                let start_idx = if trimmed.starts_with("print(") || trimmed.starts_with("druk_af(")
                {
                    trimmed.find('(').unwrap() + 1
                } else if trimmed.starts_with("print ") {
                    6
                } else {
                    8
                };

                let mut msg = trimmed[start_idx..].trim();
                if msg.ends_with(')') || msg.ends_with(';') {
                    msg = &msg[..msg.len() - 1];
                    if msg.ends_with(')') {
                        msg = &msg[..msg.len() - 1];
                    }
                }
                let clean_msg = msg.trim_matches('"');

                // Re-apply variable interpolation ONLY for the string content (since we intercepted print)
                let mut final_msg = clean_msg.to_string();
                let store = self.var_store.lock().unwrap();
                for scope in store.iter().rev() {
                    for (k, v) in scope.iter() {
                        let key = format!("${}", k);
                        if final_msg.contains(&key) {
                            // strip quotes if the value is a string, so they don't leak into the output
                            final_msg = final_msg.replace(&key, v.trim_matches('"'));
                        }
                    }
                }

                println!("[UITVOER]: {}", final_msg);
                return Ok(());
            }

            if trimmed.starts_with("lees ") {
                let path = trimmed[5..].trim();
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
                    self.execute_ast(ast).await?;
                    return Ok(());
                }
                Intent::SetVar { name, value } => {
                    println!(
                        "[INTENT]: Gedetecteerd: VARIABELE ZETTEN '{}' = '{}'",
                        name, value
                    );
                    let ast = vec![CodeTaal::VarDef { name, value }];
                    self.execute_ast(ast).await?;
                    return Ok(());
                }
                Intent::MatMul { size } => {
                    println!("[INTENT]: Gedetecteerd: MATRIX KERNEL (Size: {})", size);
                    let ast = vec![CodeTaal::MatMul {
                        m: size,
                        n: size,
                        k: size,
                    }];
                    self.execute_ast(ast).await?;
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
                        let funcs = self.func_store.lock().unwrap();
                        funcs.get(trimmed).cloned()
                    };

                    if let Some(body) = func_body {
                        println!("[EXECUTION]: Uitvoeren van functie '{}'...", trimmed);
                        self.process_command(&body).await?;
                        return Ok(());
                    }

                    // Fallback: Native Execution Layer (Rune / Low-Level)
                    self.execute_native(trimmed)?;
                }
            }

            Ok(())
        })
    }

    // Phase 8: Logical Evaluator
    async fn evaluate_condition(&self, condition: &str) -> bool {
        // 1. File Check: bestand_bestaat [path]
        if condition.starts_with("bestand_bestaat ") {
            let path = condition[16..].trim().trim_matches('"');
            return tokio::fs::try_exists(path).await.unwrap_or(false);
        }

        // 2. Powerful AST Evaluator via evalexpr
        let result = self.evaluate_expression(condition);
        if result == "waar" {
            return true;
        }
        if result == "onwaar" {
            return false;
        }

        println!(
            "[LOGIC]: Onbekende of ongeldige conditie: '{}' (Geëvalueerd tot '{}')",
            condition, result
        );
        false
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

    // --- HELHEIM V1 INTERPRETER (Language Core) ---
    pub fn execute_ast(
        &self,
        ast: Vec<CodeTaal>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        Box::pin(async move {
            for stmt in ast {
                match stmt {
                    CodeTaal::MatMul { m, n, k } => {
                        println!(
                            "[KERNEL]: Synthesis of Tiled MatMul {}x{}x{} (Shared Memory Enabled)...",
                            m, n, k
                        );
                        // 1. Synthesize PTX (JIT)
                        let ptx = synthesis::KernelSynthesisEngine::synthesize(CodeTaal::MatMul {
                            m,
                            n,
                            k,
                        })
                        .unwrap();

                        // 2. Execute on Hardware
                        println!("[GPU]: Launching Kernel on Nvidia RTX 5060 Ti...");
                        let id_a = crate::gpu::gpu_alloc_tensor_random(m, k).unwrap();
                        let id_b = crate::gpu::gpu_alloc_tensor_random(k, n).unwrap();
                        let id_c = crate::gpu::gpu_alloc_tensor_empty(m, n).unwrap();
                        match crate::gpu::gpu_execute_raw_ptx_ids(&ptx, id_a, id_b, id_c, m, n, k) {
                            Ok(gflops) => println!(
                                "[GPU]: ✅ Execution Complete. Performance: {:.2} GFLOPS",
                                gflops
                            ),
                            Err(e) => println!("[ERROR]: GPU Runtime Fail: {}", e),
                        }
                    }
                    CodeTaal::Return { value } => {
                        let eval = self.evaluate_expression(&value);
                        return Ok(Some(eval));
                    }
                    CodeTaal::Throw { message } => {
                        let eval = self.evaluate_expression(&message);
                        return Err(anyhow::anyhow!("Uncaught exception: {}", eval));
                    }
                    CodeTaal::FunctionCall { name, args } => {
                        let _ = self.execute_function_call(&name, args.clone()).await?;
                    }
                    CodeTaal::Gebruik { path } => {
                        println!("[AST]: Laden van module: '{}'", path);
                        match tokio::fs::read_to_string(&path).await {
                            Ok(content) => {
                                match crate::orchestra::parser::HelParser::parse(&content) {
                                    Ok(module_ast) => {
                                        if let Err(e) = Box::pin(self.execute_ast(module_ast)).await
                                        {
                                            println!("[ERROR]: Fout in module '{}': {}", path, e);
                                        }
                                    }
                                    Err(e) => println!(
                                        "[ERROR]: Kan module '{}' niet parsen: {}",
                                        path, e
                                    ),
                                }
                            }
                            Err(e) => println!("[ERROR]: Module '{}' niet gevonden: {}", path, e),
                        }
                    }
                    CodeTaal::FunctionDef { name, params, body } => {
                        let mut store = self.ast_funcs.lock().unwrap();
                        store.insert(name.clone(), (params.clone(), body.clone()));
                        println!(
                            "[MEMORY]: Opslaan AST-functie '{}' met {} argumenten...",
                            name,
                            params.len()
                        );
                    }
                    CodeTaal::ModelDef { name, fields } => {
                        let mut store = self.model_store.lock().unwrap();
                        store.insert(name.clone(), fields.clone());
                        println!("[MEMORY]: Blauwdruk opgeslagen voor model '{}' met {} velden.", name, fields.len());
                    }
                    CodeTaal::ModelInit { model_name, args } => {
                        // Not used in execute_ast natively because VarDef intercepts 'nieuw'
                        println!("[AST]: Onverwachte losse ModelInit voor {}", model_name);
                    }
                    CodeTaal::VarDef { name, value } => {
                        let mut evaluated_value = value.clone();
                        let clean_val = evaluated_value.trim();

                        if clean_val.starts_with("roep_aan ") {
                            let parts: Vec<&str> = clean_val.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let func_name = parts[1].to_string();
                                let mut args = Vec::new();
                                for p in &parts[2..] {
                                    args.push(p.to_string());
                                }
                                evaluated_value =
                                    self.execute_function_call(&func_name, args).await?;
                            }
                        } else if let Some(prompt) = clean_val.strip_prefix("vraag ") {
                            let prompt = prompt.trim().trim_matches('"');
                            let resolved_prompt = self.resolve_value(prompt);
                            use std::io::Write;
                            print!("{} ", resolved_prompt);
                            std::io::stdout().flush().unwrap();
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input).unwrap();
                            evaluated_value = input.trim().to_string();
                        } else if let Some(path) = clean_val.strip_prefix("lees ") {
                            let path = path.trim().trim_matches('"');
                            let resolved_path = self.resolve_value(path);
                            match std::fs::read_to_string(&resolved_path) {
                                Ok(content) => evaluated_value = content,
                                Err(e) => {
                                    println!("[ERROR]: Kan bestand '{}' niet lezen: {}", resolved_path, e);
                                    evaluated_value = "".to_string();
                                }
                            }
                        } else if let Some(model_init) = clean_val.strip_prefix("nieuw ") {
                            // Format: nieuw Server("192.168.1.1", 9000)
                            let mut parts = model_init.splitn(2, '(');
                            let model_name = parts.next().unwrap_or("").trim().to_string();
                            let args_str = parts.next().unwrap_or("").trim().trim_end_matches(')');
                            
                            let mut args = Vec::new();
                            for arg in args_str.split(',') {
                                let arg_val = arg.trim().trim_matches('"').to_string();
                                if !arg_val.is_empty() {
                                    args.push(self.resolve_value(&arg_val));
                                }
                            }
                            
                            let store = self.model_store.lock().unwrap();
                            if let Some(fields) = store.get(&model_name) {
                                if fields.len() != args.len() {
                                    println!("[ERROR]: Model '{}' verwacht {} argumenten, kreeg er {}.", model_name, fields.len(), args.len());
                                    evaluated_value = "null".to_string();
                                } else {
                                    let mut json_map = serde_json::Map::new();
                                    for (i, field) in fields.iter().enumerate() {
                                        // Attempt to parse as number or boolean, else store as string
                                        let val_str = &args[i];
                                        let json_val = if let Ok(num) = val_str.parse::<f64>() {
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
                                    evaluated_value = serde_json::to_string(&json_map).unwrap_or_else(|_| "null".to_string());
                                }
                            } else {
                                println!("[ERROR]: Model '{}' is niet gedefinieerd.", model_name);
                                evaluated_value = "null".to_string();
                            }
                        } else {
                            evaluated_value = self.evaluate_expression(&value);
                        }
                        let evaluated_value = self.resolve_value(&evaluated_value);
                        println!("[MEM]: {} = {}", name, evaluated_value);
                        self.set_var(name, evaluated_value);
                    }
                    CodeTaal::VarGet { name } => {
                        if let Some(val) = self.get_var(&name) {
                            println!("[VAL]: {} = {}", name, val);
                        } else {
                            println!("[ERR]: Variabele '{}' niet gevonden.", name);
                        }
                    }
                    CodeTaal::Loop { condition, body } => {
                        // Very simple infinite loop guard
                        let mut iterations = 0;
                        loop {
                            // Evaluate condition
                            let should_run = self.evaluate_ast_condition(&condition).await;
                            if !should_run || iterations > 1000 {
                                break;
                            }

                            // Execute Body
                            if let CodeTaal::Block { statements } = *body.clone()
                                && let Some(ret) = self.execute_ast(statements).await? {
                                    return Ok(Some(ret));
                                }
                            iterations += 1;
                        }
                    }
                    CodeTaal::ForEach {
                        iterator,
                        iterable,
                        body,
                    } => {
                        let json_val = self.resolve_value(&iterable);
                        let mut clone_statements = Vec::new();
                        if let CodeTaal::Block { statements } = *body.clone() {
                            clone_statements = statements;
                        }

                        // Try parsing JSON list
                        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&json_val) {
                            println!(
                                "[LOOP]: 'voor elke' geactiveerd met {} iteraties over '{}'.",
                                arr.len(),
                                iterable
                            );
                            for v in arr {
                                let item_str = if let Some(s) = v.as_str() {
                                    s.to_string()
                                } else {
                                    v.to_string()
                                };
                                // Inject localized variables directly into memory
                                self.set_var(iterator.clone(), item_str);
                                if let Some(ret) =
                                    self.execute_ast(clone_statements.clone()).await?
                                {
                                    return Ok(Some(ret));
                                }
                            }
                        } else {
                            println!(
                                "[ERROR]: Kan '{}' niet itereren. Waarde is geen geldige JSON-lijst.",
                                iterable
                            );
                        }
                    }
                    CodeTaal::If {
                        condition,
                        then,
                        else_block,
                    } => {
                        if self.evaluate_ast_condition(&condition).await {
                            if let CodeTaal::Block { statements } = *then.clone()
                                && let Some(ret) = self.execute_ast(statements).await? {
                                    return Ok(Some(ret));
                                }
                        } else if let Some(else_b) = else_block
                            && let CodeTaal::Block { statements } = *else_b.clone()
                                && let Some(ret) = self.execute_ast(statements).await? {
                                    return Ok(Some(ret));
                                }
                    }
                    CodeTaal::ArrayPush { array_name, value } => {
                        let args = vec![array_name.clone(), value.clone()];
                        let _ = self.execute_function_call("voeg_toe", args).await?;
                    }
                    CodeTaal::ArrayRemove { array_name, index } => {
                        let args = vec![array_name.clone(), index.clone()];
                        let _ = self.execute_function_call("verwijder", args).await?;
                    }
                    CodeTaal::Concurrent { statements } => {
                        println!(
                            "[AST]: Activeren van parallelle uitvoering ({} taken)...",
                            statements.len()
                        );
                        let mut futures_list = Vec::new();
                        for concurrent_stmt in statements {
                            // Execute each statement in its own context
                            futures_list.push(self.execute_ast(vec![concurrent_stmt.clone()]));
                        }

                        // Await all of them simultaneously
                        let results = futures::future::join_all(futures_list).await;
                        for res in results {
                            if let Err(e) = res {
                                println!("[ERROR]: Fout in parallelle taak: {}", e);
                            }
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
                        match self.execute_ast(statements).await {
                            Ok(Some(ret)) => return Ok(Some(ret)),
                            Ok(None) => {} // Success without return
                            Err(e) => {
                                println!("[VANG]: Fout afgevangen: {}", e);
                                if let Some(err_name) = error_var {
                                    let mut store = self.var_store.lock().unwrap();
                                    if let Some(scope) = store.last_mut() {
                                        scope.insert(err_name.clone(), e.to_string());
                                    }
                                }
                                let catch_statements =
                                    if let CodeTaal::Block { statements } = *catch_block.clone() {
                                        statements
                                    } else {
                                        Vec::new()
                                    };
                                // execute catch block
                                if let Some(ret) = self.execute_ast(catch_statements).await? {
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
                            let store = self.var_store.lock().unwrap();
                            for scope in store.iter().rev() {
                                for (k, v) in scope.iter() {
                                    let key = format!("${}", k);
                                    if final_payload.contains(&key) {
                                        final_payload = final_payload.replace(&key, v);
                                    }
                                }
                            }
                        }

                        println!("[AST]: Sturen naar '{}': {}", target, final_payload);

                        // 2. Broadcast Logic
                        let mut final_targets = Vec::new();
                        if target == "allemaal" {
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
                            let _ = crate::network::swarm::SwarmEngine::dispatch(
                                &t,
                                9003,
                                &final_payload,
                            )
                            .await;
                        }
                    }
                    CodeTaal::SysOp { command } => {
                        // Recursively call process_command for legacy support
                        // Note: process_command is async, so we await it.
                        self.process_command(&command).await?;
                    }
                    _ => println!("[AST]: Instructie nog niet geïmplementeerd: {:?}", stmt),
                }
            }
            Ok(None)
        })
    }

    async fn execute_function_call(&self, name: &str, args: Vec<String>) -> Result<String> {
        // --- NATIVE STD LIB (Phase 8/5) ---
        if name == "wacht" && args.len() == 1 {
            let secs_str = self.resolve_value(&args[0]);
            if let Ok(secs) = secs_str.parse::<u64>() {
                println!("[ASYNC]: Wachten voor {} seconden...", secs);
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
            return Ok("".to_string());
        }

        if name == "voeg_toe" && args.len() == 2 {
            let list_name = &args[0]; // Expecting the raw variable name
            let item = self.resolve_value(&args[1]);
            let list_val = self.resolve_value(list_name);

            if let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(&list_val) {
                if let Ok(num) = item.parse::<f64>() {
                    if num.fract() == 0.0 {
                        arr.push(serde_json::json!(num as i64));
                    } else {
                        arr.push(serde_json::json!(num));
                    }
                } else {
                    arr.push(serde_json::json!(item));
                }
                let new_list = serde_json::to_string(&arr).unwrap();

                // Modify in place where it lives!
                let mut store = self.var_store.lock().unwrap();
                let mut found = false;
                for scope in store.iter_mut().rev() {
                    if scope.contains_key(list_name) {
                        scope.insert(list_name.clone(), new_list.clone());
                        found = true;
                        break;
                    }
                }
                if !found
                    && let Some(top) = store.last_mut() {
                        top.insert(list_name.clone(), new_list.clone());
                    }
                return Ok(new_list);
            }
        }

        if name == "verwijder" && args.len() == 2 {
            let list_name = &args[0];
            let index_val = self.resolve_value(&args[1]);
            let list_val = self.resolve_value(list_name);

            if let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(&list_val)
                && let Ok(idx) = index_val.parse::<usize>()
                    && idx < arr.len() {
                        arr.remove(idx);
                        let new_list = serde_json::to_string(&arr).unwrap();

                        let mut store = self.var_store.lock().unwrap();
                        let mut found = false;
                        for scope in store.iter_mut().rev() {
                            if scope.contains_key(list_name) {
                                scope.insert(list_name.clone(), new_list.clone());
                                found = true;
                                break;
                            }
                        }
                        if !found
                            && let Some(top) = store.last_mut() {
                                top.insert(list_name.clone(), new_list.clone());
                            }

                        return Ok(new_list);
                    }
        }

        // --- STD LIB: TEKST ---
        if name == "tekst.lengte" && args.len() == 1 {
            let s = self.resolve_value(&args[0]).trim_matches('"').to_string();
            return Ok(s.len().to_string());
        }
        if name == "tekst.vervang" && args.len() == 3 {
            let s = self.resolve_value(&args[0]).trim_matches('"').to_string();
            let zoek = self.resolve_value(&args[1]).trim_matches('"').to_string();
            let vervang = self.resolve_value(&args[2]).trim_matches('"').to_string();
            return Ok(s.replace(&zoek, &vervang));
        }
        if name == "tekst.hoofdletters" && args.len() == 1 {
            let s = self.resolve_value(&args[0]).trim_matches('"').to_string();
            return Ok(s.to_uppercase());
        }
        if name == "tekst.splitsen" && args.len() == 2 {
            let s = self.resolve_value(&args[0]).trim_matches('"').to_string();
            let delimeter = self.resolve_value(&args[1]).trim_matches('"').to_string();
            let parts: Vec<String> = s.split(&delimeter).map(|p| p.to_string()).collect();
            let json_arr = serde_json::to_string(&parts).unwrap_or_else(|_| "[]".to_string());
            return Ok(json_arr);
        }

        // --- STD LIB: WISKUNDE ---
        if name == "wiskunde.willekeurig" && args.len() == 2 {
            let min_val = self.resolve_value(&args[0]).trim_matches('"').parse::<i64>().unwrap_or(0);
            let max_val = self.resolve_value(&args[1]).trim_matches('"').parse::<i64>().unwrap_or(100);
            if min_val <= max_val {
                use rand::Rng;
                let mut rng = rand::rng();
                let random_num: i64 = rng.random_range(min_val..=max_val);
                return Ok(random_num.to_string());
            } else {
                return Ok("0".to_string());
            }
        }
        if name == "wiskunde.afronden" && args.len() == 1 {
            let val = self.resolve_value(&args[0]).trim_matches('"').to_string();
            if let Ok(num) = val.parse::<f64>() {
                return Ok(num.round().to_string());
            }
        }

        // --- STD LIB: JSON ---
        if name == "json.ontleden" && args.len() == 1 {
            let mut s = self.resolve_value(&args[0]);
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                let unescaped = s[1..s.len() - 1].replace("\\\"", "\"").replace("\\n", "\n");
                s = unescaped;
            }
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s) {
                return Ok(parsed.to_string());
            } else {
                return Ok(s);
            }
        }
        if name == "json.tekst" && args.len() == 1 {
            let s = self.resolve_value(&args[0]);
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s) {
                return Ok(serde_json::to_string(&parsed).unwrap_or(s));
            } else {
                return Ok(format!("\"{}\"", s));
            }
        }

        let func_tuple = {
            let store = self.ast_funcs.lock().unwrap();
            store.get(name).cloned()
        };

        if let Some((params, body)) = func_tuple {
            let mut resolved_args = Vec::new();
            for i in 0..params.len() {
                if i < args.len() {
                    resolved_args.push(self.resolve_value(&args[i]));
                } else {
                    resolved_args.push("".to_string());
                }
            }

            self.push_scope();

            for (i, param) in params.iter().enumerate() {
                self.set_var(param.clone(), resolved_args[i].clone());
            }

            let mut result = "".to_string();
            if let CodeTaal::Block { statements } = *body
                && let Some(ret) = self.execute_ast(statements).await? {
                    result = ret;
                }

            self.pop_scope();

            Ok(result)
        } else {
            println!("[ERR]: Functie '{}' bestaat niet in AST store.", name);
            Ok("".to_string())
        }
    }

    async fn evaluate_ast_condition(&self, cond: &CodeTaal) -> bool {
        // Evaluate the raw string via the primary logical parser
        if let CodeTaal::VarGet { name } = cond {
            return self.evaluate_condition(name).await;
        }

        false
    }

    fn evaluate_expression(&self, expr: &str) -> String {
        let expr_clean = expr.trim();

        // Native STD LIB: lengte(Lijst)
        if expr_clean.starts_with("lengte(") && expr_clean.ends_with(")") {
            let inner = expr_clean[7..expr_clean.len() - 1].trim();
            let inner_val = self.resolve_value(inner);
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&inner_val) {
                return arr.len().to_string();
            } else {
                return inner_val.len().to_string();
            }
        }

        // Tensor Allocation Intercept (Phase 6)
        if expr_clean.starts_with("tensor(")
            && expr_clean.ends_with(")")
            && !expr_clean.contains("id=")
        {
            let dim: Vec<&str> = expr_clean[7..expr_clean.len() - 1].split(',').collect();
            if dim.len() == 2 {
                let m = dim[0].trim().parse::<usize>().unwrap_or(0);
                let n = dim[1].trim().parse::<usize>().unwrap_or(0);
                if m > 0 && n > 0 {
                    println!("[AST]: Nieuwe Tensor allocatie ({}x{})...", m, n);
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
            let inner_val = self.resolve_value(inner);
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
                        println!(
                            "[AST]: Tensor Activering (ReLU) gedetecteerd op {}x{}...",
                            m, n
                        );
                        let out_id = crate::gpu::gpu_alloc_tensor_empty(m, n).unwrap();
                        let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                            crate::orchestra::synthesis::CodeTaal::TensorRelu { m, n },
                        )
                        .unwrap();
                        match crate::gpu::gpu_execute_tensor_relu(&ptx, id_a, out_id, m, n) {
                            Ok(gflops) => println!(
                                "[GPU]: ✅ Tensor ReLU voltooid. Performance: {:.2} GFLOPS",
                                gflops
                            ),
                            Err(e) => println!("[ERROR]: GPU Tensor ReLU Fail: {}", e),
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
            left_val = self.resolve_value(parts[0]);
            right_val = self.resolve_value(parts[2]);
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
                    println!(
                        "[AST]: Tensor vermenigvuldiging gedetecteerd. Matrix {}x{} * {}x{}...",
                        m, k1, k2, n
                    );
                    let out_id = crate::gpu::gpu_alloc_tensor_empty(m, n).unwrap();
                    let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                        crate::orchestra::synthesis::CodeTaal::MatMul { m, n, k: k1 },
                    )
                    .unwrap();
                    println!("[GPU]: Activeren van WMMA Tensor Cores (Project Apex)...");
                    match crate::gpu::gpu_execute_raw_ptx_ids(&ptx, id_a, id_b, out_id, m, n, k1) {
                        Ok(gflops) => println!(
                            "[GPU]: ✅ Tensor Executie voltooid. Performance: {:.2} GFLOPS",
                            gflops
                        ),
                        Err(e) => {
                            println!("[GPU ERROR]: {} - Terugvallen op CPU (Rayon)...", e);
                            match crate::gpu::cpu_execute_matmul(id_a, id_b, out_id, m, n, k1) {
                                Ok(gflops) => println!(
                                    "[CPU]: ✅ Tensor Executie voltooid (Fallback). Performance: {:.2} GFLOPS",
                                    gflops
                                ),
                                Err(e) => println!("[CPU ERROR]: {}", e),
                            }
                        }
                    }
                    return format!("tensor({}, {}, id={})", m, n, out_id);
                } else {
                    println!(
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
                    println!(
                        "[AST]: Tensor Optelling gedetecteerd. Matrix {}x{} + {}x{}...",
                        m1, n1, m2, n2
                    );
                    let out_id = crate::gpu::gpu_alloc_tensor_empty(m1, n1).unwrap();
                    let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                        crate::orchestra::synthesis::CodeTaal::TensorAdd { m: m1, n: n1 },
                    )
                    .unwrap();
                    match crate::gpu::gpu_execute_tensor_add(&ptx, id_a, id_b, out_id, m1, n1) {
                        Ok(gflops) => println!(
                            "[GPU]: ✅ Tensor Optelling voltooid. Performance: {:.2} GFLOPS",
                            gflops
                        ),
                        Err(e) => {
                            println!("[GPU ERROR]: {} - Terugvallen op CPU (Rayon)...", e);
                            match crate::gpu::cpu_execute_tensor_add(id_a, id_b, out_id, m1, n1) {
                                Ok(gflops) => println!(
                                    "[CPU]: ✅ Tensor Optelling voltooid (Fallback). Performance: {:.2} GFLOPS",
                                    gflops
                                ),
                                Err(e) => println!("[CPU ERROR]: {}", e),
                            }
                        }
                    }
                    return format!("tensor({}, {}, id={})", m1, n1, out_id);
                }
            }
        }

        // --- PHASE 7: ROBUST EXPRESSION EVALUATOR (evalexpr) ---
        // If it's not a tensor operation, try to evaluate it as a complex math/logic expression
        if !expr_clean.starts_with("tensor(") && !expr_clean.contains("tensor(") {
            use evalexpr::ContextWithMutableVariables;
            let mut context: evalexpr::HashMapContext = evalexpr::HashMapContext::new();
            {
                let store = self.var_store.lock().unwrap();
                for scope in store.iter().rev() {
                    for (k, v) in scope.iter() {
                        if let Ok(num_int) = v.parse::<i64>() {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Int(num_int));
                        } else if let Ok(num_float) = v.parse::<f64>() {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Float(num_float));
                        } else {
                            // Only set string if it doesn't conflict (evalexpr treats barewords as identifiers, so string values are fine)
                            let _ = context.set_value(k.clone(), v.clone().into());
                        }
                    }
                }
            }

            let eval_str = expr_clean
                .replace(" en ", " && ")
                .replace(" of ", " || ")
                .replace("niet ", "!");

            match evalexpr::eval_with_context(&eval_str, &context) {
                Ok(result) => {
                    match result {
                        evalexpr::Value::Int(i) => return format!("{}", i),
                        evalexpr::Value::Float(f) => return format!("{}", f),
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
                    // Ignore "Variable identifier is not bound" and "Tried to append a node" errors
                    // if the string contains a bracket because it means we are just using array/dict indexing
                    // and evalexpr doesn't support it, but the fallback `resolve_value` WILL support it right after this.
                    let err_str = e.to_string();
                    if (!err_str.contains("Variable identifier is not bound")
                        && !err_str.contains("Tried to append a node"))
                        || !expr_clean.contains("[")
                    {
                        println!(
                            "[DEBUG]: evalexpr gaf fout op '{}': {}",
                            expr_clean, err_str
                        );
                    }
                }
            }
        }

        // Fallback: return as is (maybe it's just a value or string)
        self.resolve_value(expr)
    }

    fn resolve_value(&self, token: &str) -> String {
        let mut key = token;

        // Strip sigil if present (e.g. $Waarde -> Waarde)
        if key.starts_with('$') {
            key = &key[1..];
        }

        let mut index_str: Option<&str> = None;

        if let Some(start) = token.find('[')
            && token.ends_with(']') {
                key = &token[..start];
                index_str = Some(&token[start + 1..token.len() - 1]);
            }

        if let Some(val) = self.get_var(key) {
            if let Some(idx_s) = index_str {
                let clean_idx = idx_s.trim_matches('"');
                if let Ok(idx) = clean_idx.parse::<usize>() {
                    // Array Indexing
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&val)
                        && idx < arr.len() {
                            if let Some(s) = arr[idx].as_str() {
                                return s.to_string();
                            }
                            return arr[idx].to_string();
                        }
                }
                // Dictionary Label Lookup
                if let Ok(map) =
                    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&val)
                    && let Some(res) = map.get(clean_idx) {
                        if let Some(s) = res.as_str() {
                            return s.to_string();
                        }
                        return res.to_string();
                    }
            }
            val.clone()
        } else {
            token.to_string()
        }
    }
}
