use clap::{Parser, Subcommand};
pub mod intent;

#[derive(Parser, Debug)]
#[command(
    name = "helheim",
    about = "Helheim CLI - High-Performance Motor Cortex"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Versnel je actie: voer een raw file of command uit
    Run { input: String },
    /// Start de Helheim Interactive REPL
    Repl,
    /// Voer een Helheim script (.hel) uit
    Script { path: String },
    /// Compileer een Helheim script naar native PTX machinecode
    Build { path: String },
    /// Start de Helheim Node listener (gedistribueerd netwerk)
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
    /// Hervat een opgeslagen continuation (JSON)
    ResumeContinuation {
        #[arg(short, long)]
        file: String,
        #[arg(short, long)]
        value: String,
    },
    /// Visualiseer de Swarm Flight Recorder data als een tijdlijn
    Audit {
        #[arg(short, long, default_value = "audit_trail.jsonl")]
        file: String,
    },
}

// Parse "stuur [what] naar [target]" / "send [what] to [target]"
pub fn parse_simple_command(input: &str) -> Option<(String, String)> {
    let input_lc = input.to_lowercase();
    let send_words = ["stuur", "send"];
    let to_words = ["naar", "to"];

    // Fuzzy/Natural language improvements
    let has_send = send_words.iter().any(|&s| input_lc.contains(s));
    let has_to = to_words.iter().any(|&s| input_lc.contains(s));

    if has_send && has_to {
        let words: Vec<&str> = input.split_whitespace().collect();
        let send_pos = words.iter().position(|&w| send_words.contains(&w.to_lowercase().as_str()));
        let to_pos = words.iter().position(|&w| to_words.contains(&w.to_lowercase().as_str()));
        if let (Some(sp), Some(tp)) = (send_pos, to_pos) {
            if tp > sp + 1 && words.len() > tp + 1 {
                let what = words[sp + 1..tp].join(" ");
                let target = words[tp + 1..].join(" ");
                return Some((what, target));
            }
        }
    }

    // Fallback to legacy exact match
    let trimmed = input.trim();
    for send_kw in &["stuur ", "send "] {
        if let Some(after) = trimmed.strip_prefix(send_kw) {
            for to_kw in &[" naar ", " to "] {
                if let Some(pos) = after.rfind(to_kw) {
                    let what = after[..pos].trim().to_string();
                    let target = after[pos + to_kw.len()..].trim().to_string();
                    if !target.is_empty() {
                        return Some((what, target));
                    }
                }
            }
        }
    }
    None
}
