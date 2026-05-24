use anyhow::{anyhow, Result};
use std::process::Command;

pub struct PackageManager;

impl PackageManager {
    /// Detects the package manager and installs the requested package.
    pub fn install(package: &str) -> Result<String> {
        let package = package.trim();
        if package.is_empty() {
            return Err(anyhow!("Geen pakketnaam opgegeven."));
        }

        // 1. Detect Package Manager (Simple heuristic for now)
        // In the future, we can add more robust detection or support for cargo/pip/npm/snap
        let (cmd, args) = if Self::has_command("apt-get") {
            ("sudo", vec!["apt-get", "install", "-y", package])
        } else if Self::has_command("pacman") {
            ("sudo", vec!["pacman", "-S", "--noconfirm", package])
        } else if Self::has_command("dnf") {
            ("sudo", vec!["dnf", "install", "-y", package])
        } else {
            return Err(anyhow!(
                "Geen ondersteunde package manager gevonden (apt/pacman/dnf)."
            ));
        };

        println!("[PKG]: Uitvoeren: {} {}", cmd, args.join(" "));

        let output = Command::new(cmd).args(&args).output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(format!(
                "✅ Installatie van '{}' geslaagd.\n{}",
                package, stdout
            ))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("❌ Installatie mislukt:\n{}", stderr))
        }
    }

    fn has_command(cmd: &str) -> bool {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
