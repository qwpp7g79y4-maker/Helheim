use clap::{Parser, Subcommand};
pub mod intent;

#[derive(Parser, Debug)]
#[command(name = "helheim", about = "Helheim CLI - de taal die Python obsolete maakt")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Versnel je actie: voer een raw file of command uit
    Run {
        input: String,
    },
    /// Start de Helheim Interactive REPL
    Repl,
    /// Voer een Helheim script (.hel) uit
    Script {
        path: String,
    },
    /// Start de Helheim Node listener (Antigravity Cluster)
    Listen {
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },
    /// Start in Silent Swarm Mode (Service Daemon)
    Service {
        #[arg(short, long, default_value_t = 9001)]
        port: u16,
    },
    /// Upgrade Helheim from a signed source via HTTP
    Upgrade {
        #[arg(short, long)]
        url: String,
    },
}

// Parse "stuur [what] naar [target]"
pub fn parse_simple_command(input: &str) -> Option<(String, String)> {
    let input_lc = input.to_lowercase();
    // Fuzzy/Natural language improvements
    if input_lc.contains("stuur") && input_lc.contains("naar") {
        // Simple regex-like capture
        let words: Vec<&str> = input.split_whitespace().collect();
        if let Some(stuur_pos) = words.iter().position(|&w| w.to_lowercase() == "stuur") {
            if let Some(naar_pos) = words.iter().position(|&w| w.to_lowercase() == "naar") {
                if naar_pos > stuur_pos + 1 && words.len() > naar_pos + 1 {
                    let what = words[stuur_pos + 1..naar_pos].join(" ");
                    let target = words[naar_pos + 1..].join(" ");
                    return Some((what, target));
                }
            }
        }
    }
    
    // Fallback to legacy exact match
    let trimmed = input.trim();
    if !trimmed.starts_with("stuur ") {
        return None;
    }
    let after_stuur = &trimmed[6..];
    if let Some(pos) = after_stuur.rfind(" naar ") {
        let what = after_stuur[..pos].trim().to_string();
        let target = after_stuur[pos + 6..].trim().to_string();
        if !target.is_empty() {
            return Some((what, target));
        }
    }
    None
}
