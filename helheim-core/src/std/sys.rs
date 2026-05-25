use anyhow::{Result, anyhow};
use std::process::Command;

/// De Helheim System Module
/// Directe interface voor OS-level commando's (Bash/Shell).
pub struct SystemManager;

impl SystemManager {
    /// Voert een commando uit in de shell en geeft de output terug.
    /// Voorbeeld: execute("ls -la")
    pub fn execute(cmd: &str) -> Result<String> {
        // We gebruiken 'sh -c' om pipes en argumenten correct te parsen op Linux/macOS.
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| anyhow!("SYS Fout bij starten van commando '{}': {}", cmd, e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(anyhow!(
                "SYS Commando faalde (Exit Code {}): {}",
                output.status.code().unwrap_or(-1),
                stderr
            ))
        }
    }
}
