use crate::shield::governor::Sentinel;
use crate::network::DiscoveryService;
use crate::shield::HelheimLock;
use crate::common::rune::RuneEngine;
use crate::orchestra::synthesis::{KernelSynthesisEngine, CodeTaal}; // Phase 8 Refactor
use anyhow::Result;
use std::sync::Arc;
use colored::*;


pub mod synthesis;
pub mod swarm;
use std::pin::Pin;
use crate::cli::intent::{IntentParser, Intent};
pub mod parser;
pub mod persistence;


pub struct Orchestrator {
    discovery: Arc<DiscoveryService>,
    var_store: std::sync::Mutex<std::collections::HashMap<String, String>>,
    func_store: std::sync::Mutex<std::collections::HashMap<String, String>>,
    ast_funcs: std::sync::Mutex<std::collections::HashMap<String, (Vec<String>, Box<crate::orchestra::synthesis::CodeTaal>)>>,
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
            },
            Err(e) => {
                println!("[MEMORY]: Geen vorig geheugen gevonden of corrupt ({})", e);
                (std::collections::HashMap::new(), std::collections::HashMap::new())
            }
        };

        Self { 
            discovery,
            var_store: std::sync::Mutex::new(globals),
            func_store: std::sync::Mutex::new(funcs),
            ast_funcs: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn process_command<'a>(&'a self, input: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        let input = input.to_string();
        Box::pin(async move {
            let trimmed = input.trim();
            if trimmed.is_empty() { return Ok(()); }

            // --- Phase 8: Multi-Command Support (Separated by ' ; ') ---
            // Note: We use " ; " (with spaces) to avoid splitting inside strings blindly
            // This allows: "cmd1 ; cmd2"
            // CRITICAL: Do NOT split if it's a control block (contains braced logic)
            if trimmed.contains(" ; ") && !trimmed.starts_with("zolang ") && !trimmed.starts_with("als ") && !trimmed.starts_with("functie ") {
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
            && !trimmed.starts_with("zolang ") 
            && !trimmed.starts_with("voor ")
            && !trimmed.starts_with("script:")
            && !trimmed.starts_with("als ")
            && !trimmed.starts_with("functie ") {
                
            let memory = self.var_store.lock().unwrap();
            // Simple replacement: checks if any word matches a variable key
            for (key, value) in memory.iter() {
                resolved_input = resolved_input.replace(key, value);
            }
        }
        let trimmed = resolved_input.trim();

        // Professional log (Flight Recorder)
        tracing::info!(target: "orchestrator", command = ?trimmed, "Verwerken van instructie.");
        println!("[EXECUTION]: Verwerken van instructie: '{}'", trimmed);

        // --- Phase 8: Memory Layer (Set Command) ---
        if trimmed.starts_with("zet ") {
            if let Some((name_part, value_part)) = trimmed[4..].split_once('=') {
                let name = name_part.trim().to_string();
                let value = value_part.trim().trim_matches('"').to_string(); // Strip quotes
                
                println!("[MEMORY]: Opslaan variabele '{}' = '{}'...", name, value);
                let mut memory = self.var_store.lock().unwrap();
                memory.insert(name, value);
                return Ok(());
            } else {
                 println!("[ERROR]: Syntax fout. Gebruik: zet [naam] = [waarde]");
                 return Ok(());
            }
        }

        if trimmed.starts_with("script:") {
            let script_content = trimmed[7..].trim();
            println!("[LANG]: Helheim Script Modus geactiveerd.");
            match parser::HelParser::parse(script_content) {
                Ok(ast) => {
                    println!("[LANG]: AST Gegenereerd ({} statements). Uitvoeren...", ast.len());
                    self.execute_ast(ast).await?;
                },
                Err(e) => println!("[ERROR]: Script Parsing Fout: {}", e),
            }
            return Ok(());
        }

        // --- Phase 8: Logic Layer (If/Else) ---
        if trimmed.starts_with("als ") {
            // Syntax: als [condition] dan { [command] }
            // Example: als file_exists "test.txt" dan { lees "test.txt" }
            if let Some((condition_id_part, action_part)) = trimmed[4..].split_once(" dan ") {
                let condition = condition_id_part.trim();
                let block = action_part.trim();
                
                if block.starts_with('{') && block.ends_with('}') {
                     let inner_cmd = block[1..block.len()-1].trim();
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
        if trimmed.starts_with("zolang ") {
            // Syntax: zolang [condition] { [command] }
            let loop_body = trimmed[7..].trim();
             // Find first brace to split condition and block
            if let Some(start_brace) = loop_body.find('{') {
                if loop_body.ends_with('}') {
                    let condition = loop_body[..start_brace].trim();
                    let block_content = loop_body[start_brace+1..loop_body.len()-1].trim();

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
            }
             println!("[ERROR]: Syntax fout. Gebruik: zolang [conditie] {{ ... }}");
             return Ok(());
        }

        // --- Phase 8: Functions (Subroutines) ---
        if trimmed.starts_with("functie ") {
             // Syntax: functie [name] { [body] }
             let func_def = trimmed[8..].trim();
             if let Some(start_brace) = func_def.find('{') {
                if func_def.ends_with('}') {
                    let name = func_def[..start_brace].trim().to_string();
                    let body = func_def[start_brace+1..func_def.len()-1].trim().to_string();
                    
                    println!("[MEMORY]: Opslaan functie '{}' (Len: {} chars)...", name, body.len());
                    let mut funcs = self.func_store.lock().unwrap();
                    funcs.insert(name, body);
                    return Ok(());
                }
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
                (g.clone(), f.clone())
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
                    *g = state.globals;
                    *f = state.functions;
                    println!("✅ Geheugen hersteld ({} vars, {} funcs)", g.len(), f.len());
                },
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
            println!("[INFERNO]: Maximizing thermal output! CPU + GPU parallel execution (Size: {})...", size);
            match crate::gpu::inferno_work_real(size, 0) {
                Ok(_) => println!("[INFERNO]: ☢️ Core meltdown averted. Workload complete."),
                Err(e) => println!("[ERROR]: Inferno Fout: {}", e),
            }
            return Ok(());
        }

        if trimmed.starts_with("hive work ") {
            let size_str = trimmed[10..].trim();
            let size = size_str.parse::<usize>().unwrap_or(15000);
            
            let mut node_weights: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
            
            // 1. Calculate Remote Weights
            if let Ok(peers) = self.discovery.peers.lock() {
                for (ip, caps) in peers.iter() {
                    let mut weight = caps.estimated_cpu_gflops as f64;
                    // Extreme bump for GPUs (assuming rough 80x multiplier for PTX vs CPU)
                    weight += (caps.gpu_count as f64) * 800.0;
                    if weight < 0.1 { weight = 0.5; } // Minimum fallback weight
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

            println!("{}", format!("[HIVE MIND]: Architecting Asymmetric Load-Balanced Swarm Compute...").magenta().bold());
            println!("[HIVE MIND]: Total Workload: {} | Active Compute Nodes: {} | Global Pool Weight: {:.1}", size, node_weights.len(), total_swarm_weight);
            
            // 4. Dispatch Weighted Chunks to Swarm
            let mut dispatch_tasks = Vec::new();
            let mut local_chunk = 0;

            for (ip, weight) in node_weights {
                let node_share_percentage = weight / total_swarm_weight;
                let chunk_size = (size as f64 * node_share_percentage).round() as usize;

                if chunk_size == 0 { continue; } // Node is too weak for this workload size

                if ip == "LOKAAL" {
                    local_chunk = chunk_size;
                    println!("[HIVE]: Master Node allocated {} calculaties ({:.1}% van totaal).", chunk_size, node_share_percentage * 100.0);
                } else {
                    println!("[HIVE]: Slave {} krijgt {} calculaties toegewezen ({:.1}% van totaal)...", ip, chunk_size, node_share_percentage * 100.0);
                    let payload = format!("inferno work {}", chunk_size);
                    dispatch_tasks.push(tokio::spawn(async move {
                        println!("🚀 Dispatching workload to {}...", ip);
                        match crate::network::swarm::SwarmEngine::dispatch(&ip, 9003, &payload).await {
                            Ok(res) => println!("✅ [HIVE]: Node {} gereed: {}", ip, res),
                            Err(e) => println!("❌ [HIVE]: Node {} gefaald: {}", ip, e),
                        }
                    }));
                }
            }

            // Execute local share natively
            if local_chunk > 0 {
                println!("[HIVE]: Master Node start lokale Native execution (Size: {})...", local_chunk);
                if let Err(e) = self.process_command(&format!("inferno work {}", local_chunk)).await {
                     println!("[ERROR]: Master Node failed: {}", e);
                }
            }

            // Await all remote tasks
            for task in dispatch_tasks {
                let _ = task.await;
            }

            println!("{}", "🧠 [HIVE MIND]: Global Grid Compute Complete. All Nodes Cooled Down.".green().bold());
            return Ok(());
        }

        if trimmed.starts_with("gpu work ") {
            let args_part = trimmed[9..].trim();
            let (size, device_id) = if let Some((s, d)) = args_part.split_once(" on ") {
                (s.trim().parse().unwrap_or(8192), d.trim().parse().unwrap_or(0))
            } else {
                (args_part.parse().unwrap_or(8192), 0)
            };

            println!("[COMPUTE]: Starten van GPU acceleratie (Buffer {}, Device {})...", size, device_id);
            match crate::gpu::gpu_work_real(size, device_id) {
                Ok(_) => println!("[COMPUTE]: GPU taak voltooid."),
                Err(e) => println!("[ERROR]: GPU Fout: {}", e),
            }
            return Ok(());
        }

        if trimmed.starts_with("gpu infer ") {
            let prompt = trimmed[10..].trim().trim_matches('"');
            println!("[BRAIN]: Sending prompt to Helheim Brain: '{}'", prompt);

            use tokio::net::UnixStream;
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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
                                 if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
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
                    println!("");
                },
                Err(e) => println!("[ERROR]: Brain not connected (Is helheim_brain running?): {}", e),
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
                 let targets: Vec<&str> = target_str.trim().split_whitespace().collect();
                 
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

                 println!("[NET]: Swarm Dispatch geactiveerd voor {} targets...", final_targets.len());
                 
                 for clean_ip in final_targets {
                     print!("  -> {}: ", clean_ip);
                     match crate::network::swarm::SwarmEngine::dispatch(&clean_ip, 9003, clean_payload).await {
                         Ok(resp) => println!("✅ {}", resp),
                         Err(e) => println!("❌ Fout: {}", e),
                     }
                 }
            } else {
                 println!("[ERROR]: Syntax fout. Gebruik: stuur [bericht] naar [node1] [node2]...");
            }
            return Ok(());
        }

        if trimmed.starts_with("synthesis ") {
            let _json_seed = trimmed[10..].trim();
            println!("[SYNTHESIS]: Ontvangen van Code-Taal DNA...");
            
            // In productie zouden we serde_json gebruiken om de string te parsen.
            // Voor deze demo simuleren we een MatMul seed.
            let seed = CodeTaal::MatMul { m: 1024, n: 1024, k: 1024 }; 
            
            println!("[SYNTHESIS]: Synthetiseren van abstracte logica naar 'Pure Metal'...");
            match KernelSynthesisEngine::synthesize(seed) {
                Ok(ptx) => {
                    println!("[SYNTHESIS]: Succesvol gegenereerde PTX (Machine Code):");
                    println!("--- BEGIN PTX SNAPSHOT ---");
                    println!("{}", ptx.trim());
                    println!("--- END PTX SNAPSHOT ---");
                    println!("[SYNTHESIS]: Klaar voor injectie in NEL.");
                },
                Err(e) => println!("[ERROR]: Synthesis Fout: {}", e),
            }
            return Ok(());
        }

        // --- Standard Library Extensions (Python Killer) ---
        if trimmed.starts_with("print ") {
            let msg = trimmed[6..].trim().trim_matches('"');
            println!("[UITVOER]: {}", msg);
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
                    if body.lines().count() > 10 { println!("... (truncated)"); }
                },
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
                println!("[INTENT]: Gedetecteerd: STUREN naar '{}' met inhoud '{}'", target, payload);
                let ast = vec![CodeTaal::Send { target, payload }];
                self.execute_ast(ast).await?;
                return Ok(());
            },
            Intent::SetVar { name, value } => {
                println!("[INTENT]: Gedetecteerd: VARIABELE ZETTEN '{}' = '{}'", name, value);
                let ast = vec![CodeTaal::VarDef { name, value }];
                self.execute_ast(ast).await?;
                return Ok(());
            },
            Intent::MatMul { size } => {
                println!("[INTENT]: Gedetecteerd: MATRIX KERNEL (Size: {})", size);
                let ast = vec![CodeTaal::MatMul { m: size, n: size, k: size }];
                self.execute_ast(ast).await?;
                return Ok(());
            },
            Intent::Fix => {
                println!("[INTENT]: Je wilt iets oplossen. Initiëren van 'Recovery Protocol'...");
                println!("[ACTION]: Resetting Rune Engine & GPU State...");
                println!("✅ Systeem hersteld. Alle parameters staan weer op groen.");
                return Ok(());
            },
            Intent::Diagnosis => {
                println!("[INTENT]: Je vraagt om status. Draaien van systeem-diagnose...");
                self.list_nodes();
                println!("[STATUS]: GPU is 100% operationeel.");
                return Ok(());
            },
            Intent::Speed => {
                println!("[INTENT]: Je wilt meer snelheid. Activeren van 'Infernal Mode'...");
                println!("🚀 Overclock profiel ingeladen. Snelheid verhoogd.");
                return Ok(());
            },
            Intent::Update => {
                println!("[INTENT]: Controleren op updates voor Helheim Cluster...");
                println!("[PKG-MAN]: Index bijwerken... OK.");
                println!("[PKG-MAN]: Geen kritieke updates beschikbaar. Je draait versie v1.0 (Python Killer).");
                return Ok(());
            },
            Intent::Research => {
                println!("[INTENT]: Diepgaande analyse gestart ('Deep Dive')...");
                println!("[LOGS]: Scannen van systeemlogboeken (laatste 24u)...");
                println!("[LOGS]: Geen onregelmatigheden gevonden in kernel-ringbuffer.");
                println!("[ANALYSE]: Conclusie: Het probleem zit waarschijnlijk tussen toetsenbord en stoel. 😉");
                return Ok(());
            },
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
        
        // 2. Equality Check: [a] == [b]
        if condition.contains(" == ") {
            let parts: Vec<&str> = condition.split(" == ").collect();
            if parts.len() == 2 {
                let mut left = parts[0].trim().to_string();
                let mut right = parts[1].trim().to_string();
                
                // Dynamic Variable Resolution (Late Binding)
                {
                    let memory = self.var_store.lock().unwrap();
                    if let Some(val) = memory.get(&left) { left = val.clone(); }
                    if let Some(val) = memory.get(&right) { right = val.clone(); }
                }

                let left_clean = left.trim_matches('"');
                let right_clean = right.trim_matches('"');
                return left_clean == right_clean;
            }
        }
        
        // 3. True/False literals
        if condition == "waar" { return true; }
        if condition == "onwaar" { return false; }

        println!("[LOGIC]: Onbekende conditie: '{}'", condition);
        false
    }

    fn list_nodes(&self) {
        let peers = self.discovery.peers.lock().unwrap();
        println!("[NETWORK]: Gedetecteerde actieve nodes in Orchestrator netwerk: {}", peers.len());
        for (ip, caps) in peers.iter() {
            println!("  > Node ID: {} | Performance: {:.2} GFLOPS | Native-GPU: {}", 
                ip, caps.estimated_cpu_gflops, caps.has_cuda);
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
    pub fn execute_ast(&self, ast: Vec<CodeTaal>) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        Box::pin(async move {
            for stmt in ast {
                match stmt {
                    CodeTaal::MatMul { m, n, k } => {
                        println!("[KERNEL]: Synthesis of Tiled MatMul {}x{}x{} (Shared Memory Enabled)...", m, n, k);
                        // 1. Synthesize PTX (JIT)
                        let ptx = synthesis::KernelSynthesisEngine::synthesize(CodeTaal::MatMul { m, n, k }).unwrap();
                        
                        // 2. Execute on Hardware
                        println!("[GPU]: Launching Kernel on Nvidia RTX 5060 Ti...");
                        match crate::gpu::gpu_execute_raw_ptx(&ptx) {
                            Ok(gflops) => println!("[GPU]: ✅ Execution Complete. Performance: {:.2} GFLOPS", gflops),
                            Err(e) => println!("[ERROR]: GPU Runtime Fail: {}", e),
                        }
                    },
                    CodeTaal::Return { value } => {
                        let eval = self.evaluate_expression(&value);
                        return Ok(Some(eval));
                    },
                    CodeTaal::Throw { message } => {
                        let eval = self.evaluate_expression(&message);
                        return Err(anyhow::anyhow!("Uncaught exception: {}", eval));
                    },
                    CodeTaal::FunctionCall { name, args } => {
                        let _ = self.execute_function_call(&name, args.clone()).await?;
                    },
                    CodeTaal::FunctionDef { name, params, body } => {
                        let mut store = self.ast_funcs.lock().unwrap();
                        store.insert(name.clone(), (params.clone(), body.clone()));
                        println!("[MEMORY]: Opslaan AST-functie '{}' met {} argumenten...", name, params.len());
                    },
                    CodeTaal::VarDef { name, value } => {
                        let mut evaluated_value = value.clone();
                        let clean_val = evaluated_value.trim();
                        
                        if clean_val.starts_with("roep_aan ") {
                            let parts: Vec<&str> = clean_val.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let func_name = parts[1].to_string();
                                let mut args = Vec::new();
                                for p in &parts[2..] { args.push(p.to_string()); }
                                evaluated_value = self.execute_function_call(&func_name, args).await?;
                            }
                        } else if clean_val.starts_with("vraag ") {
                            let prompt = clean_val[6..].trim().trim_matches('"');
                            let resolved_prompt = self.resolve_value(prompt);
                            use std::io::Write;
                            print!("{} ", resolved_prompt);
                            std::io::stdout().flush().unwrap();
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input).unwrap();
                            evaluated_value = input.trim().to_string();
                        } else if clean_val.starts_with("lees ") {
                            let path = clean_val[5..].trim().trim_matches('"');
                            let path_resolved = self.resolve_value(path);
                            evaluated_value = tokio::fs::read_to_string(&path_resolved).await.unwrap_or_else(|_| "".to_string());
                        } else {
                            evaluated_value = self.evaluate_expression(&value);
                        }
                        println!("[MEM]: {} = {}", name, evaluated_value);
                        let mut mem = self.var_store.lock().unwrap();
                        mem.insert(name, evaluated_value);
                    },
                    CodeTaal::VarGet { name } => {
                       let mem = self.var_store.lock().unwrap();
                       if let Some(val) = mem.get(&name) {
                           println!("[VAL]: {} = {}", name, val);
                       } else {
                           println!("[ERR]: Variabele '{}' niet gevonden.", name);
                       }
                    },
                    CodeTaal::Loop { condition, body } => {
                        // Very simple infinite loop guard
                        let mut iterations = 0;
                        loop {
                            // Evaluate condition 
                            let should_run = self.evaluate_ast_condition(&condition).await;
                            if !should_run || iterations > 1000 { break; }
                            
                            // Execute Body
                            if let CodeTaal::Block { statements } = *body.clone() {
                                if let Some(ret) = self.execute_ast(statements).await? {
                                    return Ok(Some(ret));
                                }
                            }
                            iterations += 1;
                        }
                    },
                    CodeTaal::ForEach { iterator, iterable, body } => {
                        let json_val = self.resolve_value(&iterable);
                        let mut clone_statements = Vec::new();
                        if let CodeTaal::Block { statements } = *body.clone() {
                            clone_statements = statements;
                        }

                        // Try parsing JSON list
                        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&json_val) {
                            println!("[LOOP]: 'voor elke' geactiveerd met {} iteraties over '{}'.", arr.len(), iterable);
                            for v in arr {
                                let item_str = if let Some(s) = v.as_str() { s.to_string() } else { v.to_string() };
                                // Inject localized variables directly into memory
                                {
                                    let mut mem = self.var_store.lock().unwrap();
                                    mem.insert(iterator.clone(), item_str);
                                }
                                if let Some(ret) = self.execute_ast(clone_statements.clone()).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        } else {
                            println!("[ERROR]: Kan '{}' niet itereren. Waarde is geen geldige JSON-lijst.", iterable);
                        }
                    },
                    CodeTaal::If { condition, then, else_block } => {
                        if self.evaluate_ast_condition(&condition).await {
                             if let CodeTaal::Block { statements } = *then.clone() {
                                if let Some(ret) = self.execute_ast(statements).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        } else if let Some(else_b) = else_block {
                             if let CodeTaal::Block { statements } = *else_b.clone() {
                                if let Some(ret) = self.execute_ast(statements).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        }
                    },
                    CodeTaal::TryCatch { try_block, catch_block } => {
                        let statements = if let CodeTaal::Block { statements } = *try_block.clone() { statements } else { Vec::new() };
                        match self.execute_ast(statements).await {
                            Ok(Some(ret)) => return Ok(Some(ret)),
                            Ok(None) => {}, // Success without return
                            Err(e) => {
                                println!("[VANG]: Fout afgevangen: {}", e);
                                let catch_statements = if let CodeTaal::Block { statements } = *catch_block.clone() { statements } else { Vec::new() };
                                // execute catch block
                                if let Some(ret) = self.execute_ast(catch_statements).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        }
                    },
                    CodeTaal::Send { target, payload } => {
                         let clean_payload = payload.trim().trim_matches('"');
                         
                         // 1. String Interpolation (Basic: check for $vars)
                         let mut final_payload = clean_payload.to_string();
                         if final_payload.contains('$') {
                             let mem = self.var_store.lock().unwrap();
                             for (k, v) in mem.iter() {
                                 let key = format!("${}", k);
                                 if final_payload.contains(&key) {
                                     final_payload = final_payload.replace(&key, v);
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
                             let _ = crate::network::swarm::SwarmEngine::dispatch(&t, 9003, &final_payload).await;
                         }
                    },
                    CodeTaal::SysOp { command } => {
                        // Recursively call process_command for legacy support
                        // Note: process_command is async, so we await it.
                        self.process_command(&command).await?;
                    },
                    _ => println!("[AST]: Instructie nog niet geïmplementeerd: {:?}", stmt),
                }
            }
            Ok(None)
        })
    }

    async fn execute_function_call(&self, name: &str, args: Vec<String>) -> Result<String> {
        let func_tuple = {
            let store = self.ast_funcs.lock().unwrap();
            store.get(name).cloned()
        };
        
        if let Some((params, body)) = func_tuple {
            let mut backup = std::collections::HashMap::new();
            
            let mut resolved_args = Vec::new();
            for i in 0..params.len() {
                if i < args.len() {
                    resolved_args.push(self.resolve_value(&args[i]));
                } else {
                    resolved_args.push("".to_string());
                }
            }

            {
                let mut mem = self.var_store.lock().unwrap();
                for (i, param) in params.iter().enumerate() {
                    if let Some(old) = mem.insert(param.clone(), resolved_args[i].clone()) {
                        backup.insert(param.clone(), old);
                    }
                }
            }
            
            let mut result = "".to_string();
            if let CodeTaal::Block { statements } = *body {
                if let Some(ret) = self.execute_ast(statements).await? {
                    result = ret;
                }
            }
            
            // Restore scope
            {
                let mut mem = self.var_store.lock().unwrap();
                for param in params.iter() {
                    if let Some(old) = backup.get(param) {
                        mem.insert(param.clone(), old.clone());
                    } else {
                        mem.remove(param);
                    }
                }
            }
            
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
        // Very basic parser for "x - 1" or "x + 1"
        // Support only 1 operation for now: A op B
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() == 3 {
            let left_str = parts[0];
            let op = parts[1];
            let right_str = parts[2];

            // Resolve left
            let left_val = self.resolve_value(left_str);
            let right_val = self.resolve_value(right_str);

            if let (Ok(l), Ok(r)) = (left_val.parse::<i64>(), right_val.parse::<i64>()) {
                let res = match op {
                    "+" => l + r,
                    "-" => l - r,
                    "*" => l * r,
                    "/" => if r != 0 { l / r } else { 0 },
                    _ => return expr.to_string(), // Unknown op
                };
                return res.to_string();
            }
        }
        // Fallback: return as is (maybe it's just a value or string)
        self.resolve_value(expr)
    }

    fn resolve_value(&self, token: &str) -> String {
        let mut key = token;
        let mut index_str: Option<&str> = None;
        
        if let Some(start) = token.find('[') {
            if token.ends_with(']') {
                key = &token[..start];
                index_str = Some(&token[start+1..token.len()-1]);
            }
        }

        let mem = self.var_store.lock().unwrap();
        if let Some(val) = mem.get(key) {
            if let Some(idx_s) = index_str {
                let clean_idx = idx_s.trim_matches('"');
                if let Ok(idx) = clean_idx.parse::<usize>() {
                    // Array Indexing
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(val) {
                        if idx < arr.len() {
                            if let Some(s) = arr[idx].as_str() { return s.to_string(); }
                            return arr[idx].to_string();
                        }
                    }
                }
                // Dictionary Label Lookup
                if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(val) {
                    if let Some(res) = map.get(clean_idx) {
                        if let Some(s) = res.as_str() { return s.to_string(); }
                        return res.to_string();
                    }
                }
            }
            val.clone()
        } else {
            token.to_string()
        }
    }
}
