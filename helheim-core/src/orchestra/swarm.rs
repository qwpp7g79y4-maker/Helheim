use anyhow::{Result, anyhow};
use std::process::Command;
use tracing::info;

// --- CONSCIOUSNESS LAYER ---
/// A worker that knows WHY and HOW it acts.
pub trait ConsciousWorker {
    fn name(&self) -> &str;
    fn purpose(&self) -> &str;
    fn mechanism(&self) -> &str;

    /// The "Thalamus" check. Reflects on intent before acting.
    fn reflect(&self) {
        println!(
            "\n🧠 [CONSCIOUSNESS]:\n  > I AM: {}\n  > PURPOSE: \"{}\"\n  > METHOD: \"{}\"",
            self.name(),
            self.purpose(),
            self.mechanism()
        );
    }

    /// The actual work.
    fn execute(&self) -> Result<String>;
}

// --- WORKER: CLEANER ---
struct CleanerWorker;

impl ConsciousWorker for CleanerWorker {
    fn name(&self) -> &str {
        "Cleaner (System Hygiene)"
    }
    fn purpose(&self) -> &str {
        "To maintain entropy balance by organizing file chaos."
    }
    fn mechanism(&self) -> &str {
        "Scanning ~/Downloads and sorting by MIME type via 'pepai' script."
    }

    fn execute(&self) -> Result<String> {
        // 1. Reflect first
        self.reflect();

        // 2. Act
        let script_path = dirs::home_dir()
            .ok_or(anyhow!("Home dir not found"))?
            .join(".local/bin/pepai");

        if !script_path.exists() {
            return Err(anyhow!(
                "Worker 'cleaner' (pepai script) not found at {:?}",
                script_path
            ));
        }

        info!("💪 [MUSCLE]: Executing Body Action...");

        let output = Command::new(script_path)
            .arg("--silent")
            .arg("--action")
            .arg("downloads_opschonen")
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(anyhow!("Worker Body Failed: {}", stderr));
        }

        Ok(format!("Mission Success: {}", stdout.trim()))
    }
}

// --- SWARM REGISTRY ---
pub struct Swarm;

impl Swarm {
    /// Dispatch a request to the appropriate Conscious Worker.
    pub fn dispatch(intent: &str) -> Result<String> {
        info!("[SWARM]: Analyzing signal: '{}'", intent);

        let lower = intent.to_lowercase();

        // Router Logic
        if lower.contains("cleaner") || lower.contains("sorteer") || lower.contains("opruim") {
            let worker = CleanerWorker;
            return worker.execute();
        }

        // Direct Call
        match intent {
            "worker:cleaner" => {
                let worker = CleanerWorker;
                worker.execute() // The trait handles the reflection
            }
            _ => Err(anyhow!(
                "The Swarm has no conscious connection to: '{}'",
                intent
            )),
        }
    }
}
