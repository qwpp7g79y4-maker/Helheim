use std::sync::Arc;
use anyhow::Result;
use clap::Parser;
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::{Editor, history::DefaultHistory};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use helheim_alchemie::cli::{Cli, Commands};
use helheim_alchemie::common::probe::HelProbe;
use helheim_alchemie::network::DiscoveryService;
use helheim_alchemie::orchestra::Orchestrator;
use helheim_alchemie::network::swarm::SwarmEngine;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize Telemetry (Flight Recorder)
    let _guard = helheim_alchemie::common::telemetry::init_telemetry();
    
    // 2. Parse Arguments
    let cli = Cli::parse();
    
    // 3. Ignite Core Components
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery.clone()));

    match cli.command {
        Commands::Service { port } => {
            println!("{}", format!("[SYSTEM]: Starting Helheim Service Daemon on port {}...", port).green().bold());
            println!("[SYSTEM]: Mode = HEADLESS SWARM NODE");
            
            // Start the Swarm Listener
            SwarmEngine::ignite(port, orchestrator.clone()).await?;
            
            // Broadcast Presence via Discovery Service
            let stats = HelProbe::probe();
            DiscoveryService::broadcast(port, stats)?;
            
            // Keep Alive Loop using Tokio Signal
            println!("[SYSTEM]: Daemon Operational. Press Ctrl+C to stop.");
            tokio::signal::ctrl_c().await?;
            println!("[SYSTEM]: Shutting down service.");
        }
        
        Commands::Listen { port } => {
            // Legacy Listener (Pre-Swarm)
            println!("[SYSTEM]: Starting Legacy Listener on port {}...", port);
            helheim_alchemie::network::NodeRelay::listen(port)?;
        }

        Commands::Repl => {
            // Ignite Swarm in Background for REPL node too (Active Node)
            SwarmEngine::ignite(9003, orchestrator.clone()).await?;

            // Initialize Rustyline Editor
            let mut rl = Editor::<(), DefaultHistory>::new()?;
            if rl.load_history("history.txt").is_err() {
                // No history found
            }

            println!("{}", "==========================================".cyan());
            println!("{}", "   HELHEIM: The Native Ascension (REPL)   ".cyan().bold());
            println!("{}", "   Type 'help' or 'exit' to begin.        ".cyan());
            println!("{}", "==========================================".cyan());

            let mut repl_buffer = String::new();
            let mut brace_depth = 0i32;

            loop {
                let prompt = if brace_depth > 0 {
                    format!("{}", "... ".yellow())
                } else {
                    format!("{}", "helheim> ".green())
                };

                let readline = rl.readline(&prompt);
                match readline {
                    Ok(line) => {
                        let input = line.trim();
                        if input.is_empty() { continue; }
                        
                        brace_depth += input.chars().filter(|&c| c == '{').count() as i32;
                        brace_depth -= input.chars().filter(|&c| c == '}').count() as i32;
                        
                        if !repl_buffer.is_empty() {
                            repl_buffer.push(' ');
                        }
                        repl_buffer.push_str(input);
                        
                        if brace_depth > 0 {
                            continue;
                        }

                        let final_input = repl_buffer.clone();
                        repl_buffer.clear();
                        brace_depth = 0; // Prevent negative depth breaking the prompt
                        
                        let _ = rl.add_history_entry(&final_input);

                        // Meta-commands
                        if final_input.eq_ignore_ascii_case("exit") || final_input.eq_ignore_ascii_case("quit") {
                            break;
                        }
                        if final_input.eq_ignore_ascii_case("clear") {
                            print!("\x1B[2J\x1B[1;1H");
                            continue;
                        }
                        if final_input.eq_ignore_ascii_case("help") {
                            print_help();
                            continue;
                        }

                        // We pass the string to the orchestrator. For proper AST in REPL, we could wrap it in script:
                        // but let's stick to process_command for now as it supports intent parsing.
                        // However, if the command contains braces, it's likely a native Helheim code block (als/functie)
                        let is_block = final_input.contains('{') && final_input.contains('}');
                        if is_block {
                             let script_cmd = format!("script: {}", final_input);
                             if let Err(e) = orchestrator.process_command(&script_cmd).await {
                                  println!("{} {}", "[ERROR]".red().bold(), e);
                             }
                        } else {
                             // Orchestrator Execution (Legacy / Single line / Intents)
                             if let Err(e) = orchestrator.process_command(&final_input).await {
                                  println!("{} {}", "[ERROR]".red().bold(), e);
                             }
                        }
                    }
                    Err(ReadlineError::Interrupted) => {
                        println!("{}", "^C".yellow());
                        repl_buffer.clear();
                        brace_depth = 0;
                        continue; // Don't break, just clear buffer on Ctrl+C
                    }
                    Err(ReadlineError::Eof) => {
                        println!("{}", "EOF".yellow());
                        break;
                    }
                    Err(err) => {
                        println!("Error: {:?}", err);
                        break;
                    }
                }
            }
            rl.save_history("history.txt")?;
        }

        Commands::Script { path } => {
            println!("[SCRIPT]: Executing '{}' via Native AST Engine...", path);
            let content = tokio::fs::read_to_string(path).await?;
            let script_cmd = format!("script: {}", content);
            orchestrator.process_command(&script_cmd).await?;
        }

        Commands::Run { input } => {
             // Direct command execution
             orchestrator.process_command(&input).await?;
        }

        Commands::Upgrade { url } => {
            println!("{}", "[UPDATE]: Initiating Signed Upgrade Sequence...".yellow().bold());
            
            // 1. Download Binary
            println!("[UPDATE]: Downloading binary from '{}'...", url);
            let bin_data = match ureq::get(&url).call() {
                Ok(res) => res.into_body().read_to_vec().map_err(|e| anyhow::anyhow!("Read Error: {}", e))?,
                Err(e) => return Err(anyhow::anyhow!("Download Failed: {}", e)),
            };

            // 2. Download Signature (.sig)
            let sig_url = format!("{}.sig", url);
            println!("[UPDATE]: Downloading signature from '{}'...", sig_url);
            let sig_data = match ureq::get(&sig_url).call() {
                Ok(res) => res.into_body().read_to_vec().map_err(|e| anyhow::anyhow!("Read Error: {}", e))?,
                Err(e) => return Err(anyhow::anyhow!("Signature Download Failed: {}", e)),
            };

            // 3. Verify Signature
            println!("[UPDATE]: Verifying cryptographic signature...");
            use helheim_alchemie::shield::crypto::HelSigner;
            HelSigner::verify_update(&bin_data, &sig_data)?;
            println!("{}", "✅ SIGNATURE VERIFIED. TRUST ESTABLISHED.".green().bold());

            // 4. Install Update (Self-Replace)
            println!("[UPDATE]: Installing new binary...");
            let current_exe = std::env::current_exe()?;
            
            // Create temp file
            let tmp_path = current_exe.with_extension("tmp");
            tokio::fs::write(&tmp_path, &bin_data).await?;
            
            // Rename is atomic on POSIX
            tokio::fs::rename(&tmp_path, &current_exe).await?;
            
            // chmod +x (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&current_exe)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&current_exe, perms)?;
            }

            println!("{}", "🚀 UPDATE COMPLETE. RESTARTING HIVE NODE.".green().bold());
            std::process::exit(0);
        }

        Commands::Trap { type_ } => {
            use helheim_alchemie::shield::trap::DesireEngine;
            
            println!("{}", format!("[ARTISJOK]: Generating Cursed Artifact (Type: {})...", type_).magenta().bold());
            
            let (filename, content) = match type_.as_str() {
                "env" => (".env", DesireEngine::generate_env()),
                "rsa" => ("id_rsa", DesireEngine::generate_rsa()),
                "sql" => ("database.sql", DesireEngine::generate_sql()),
                _ => {
                     println!("❌ Unknown trap type. Use: env, rsa, sql");
                     return Ok(());
                }
            };
            
            tokio::fs::write(filename, &content).await?;
            println!("🪤 TRAP DEPLOYED: ./{} (Do not open!)", filename);
        }

        Commands::Cage { ban, log } => {
            use helheim_alchemie::shield::cage::Cage;
            
            if let Some(ip) = ban {
                let report = Cage::drop_ip(&ip);
                println!("{}", report);
            } else if let Some(ip) = log {
                 let report = Cage::log_ip(&ip);
                 println!("{}", report);
            } else {
                println!("⚠️ Usage: helheim cage --ban [IP] OR --log [IP]");
            }
        }

        Commands::Brain { prompt } => {
            println!("{}", "[BRAIN]: Connecting to Neural Interface (/tmp/helheim_brain.sock)...".cyan());
            let socket_path = "/tmp/helheim_brain.sock";
            
            // Connect to Brain Service
            let mut stream = tokio::net::UnixStream::connect(socket_path).await
                .map_err(|e| anyhow::anyhow!("Brain Offline (Is ./run_brain.sh active?): {}", e))?;
            
            // Send Request
            let req = serde_json::json!({
                "prompt": prompt,
                "max_tokens": 1024
            });
            stream.write_all(req.to_string().as_bytes()).await?;

            // Read Response Stream
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            
            println!("{}", "--- MISTRAL LINK ESTABLISHED ---".magenta().bold());
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await?;
                if n == 0 { break; }
                
                // Parse { "token": "...", "done": false }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                     if let Some(token) = json["token"].as_str() {
                         print!("{}", token);
                         io::stdout().flush().await?;
                     }
                     if json["done"].as_bool().unwrap_or(false) {
                         break;
                     }
                }
            }
            println!("\n{}", "--- END OF TRANSMISSION ---".magenta().bold());
        }
    }

    Ok(())
}

fn print_help() {
    println!("\n{}", "--- Available Commands ---".yellow().bold());
    println!("{} {}", "Standard Lib:".bold(), "FS, SYS, HTTP");
    println!("  lees [bestand]                    - Read file");
    println!("  schrijf [tekst] naar [bestand]    - Write file");
    println!("  voer uit [commando]               - Execute shell cmd");
    println!("  haal [url]                        - HTTP GET request");
    println!("  installeer [naam]                 - Auto-install package (NEW)");
    println!("");
    println!("{} {}", "Core Engine:".bold(), "Compute & Shield");
    println!("  gpu work [size]                   - Run CUDA MatMul");
    println!("  shield encrypt [text]             - Encrypt data");
    println!("  stuur [msg] naar [node]           - Network dispatch");
    println!("");
    println!("{} {}", "Social Mode:", "Pieter-Direct Intent Parser");
    println!("  \"zoek dit uit\", \"analyseer\"       - Deep Dive Analysis");
    println!("  \"welke updates\", \"nieuws\"         - Check Package Status");
    println!("  \"wat is er mis\", \"foutcodes\"      - System Diagnosis");
    println!("\n");
}
