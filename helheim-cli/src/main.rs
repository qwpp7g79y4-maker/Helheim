use std::sync::Arc;
use anyhow::Result;
use clap::Parser;
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::{Editor, history::DefaultHistory};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use helheim_core::cli::{Cli, Commands};
use helheim_core::common::probe::HelProbe;
use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::network::swarm::SwarmEngine;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize Telemetry (Flight Recorder)
    let _guard = helheim_core::common::telemetry::init_telemetry();
    
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
            helheim_core::network::NodeRelay::listen(port)?;
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
                            repl_buffer.push('\n');
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

                        // Orchestrator Execution
                        if let Err(e) = orchestrator.process_command(&final_input).await {
                             println!("{} {}", "[ERROR]".red().bold(), e);
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
            let mut script_buffer = String::new();
            let mut brace_depth = 0i32;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") { continue; }
                
                brace_depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
                brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;
                
                if !script_buffer.is_empty() {
                    script_buffer.push('\n');
                }
                script_buffer.push_str(trimmed);
                
                if brace_depth > 0 {
                    continue;
                }
                
                let final_input = script_buffer.clone();
                script_buffer.clear();
                brace_depth = 0;

                if let Err(e) = orchestrator.process_command(&final_input).await {
                    println!("[ERROR]: {}", e);
                    break;
                }
            }
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
            use helheim_core::shield::crypto::HelSigner;
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
