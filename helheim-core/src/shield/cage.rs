use colored::*;
use std::process::Command;

pub struct Cage;

impl Cage {
    /// BANS an IP address using iptables (Requires Root/Sudo)
    /// This is the "Jail" function.
    pub fn drop_ip(ip: &str) -> String {
        // [W·AG·AF] C1 Review: Zero-Noise Compliance
        tracing::warn!(
            "{}",
            format!("[SHIELD] 🛡️ ENGAGING CAGE PROTOCOL FOR: {}", ip)
                .red()
                .bold()
        );

        // 1. Drop Input
        let _ = Command::new("sudo")
            .args(["iptables", "-A", "INPUT", "-s", ip, "-j", "DROP"])
            .output();

        // 2. Kill Connections
        let _ = Command::new("sudo")
            .args(["conntrack", ("-D"), "-s", ip])
            .output();

        format!("🚫 TARGET '{}' HAS BEEN LOCKED IN THE CAGE.", ip)
    }

    /// Logs an IP without banning (Warning Shot)
    pub fn log_ip(ip: &str) -> String {
        // [W·AG·AF] C1 Review: Zero-Noise Compliance
        tracing::info!(
            "{}",
            format!("[SHIELD] 👁️ MONITORING SUSPICIOUS ACTIVITY: {}", ip)
                .yellow()
                .bold()
        );
        // In real V2.4, this would write to evidence_locker
        format!("👁️ TARGET '{}' IS BEING WATCHED.", ip)
    }
}
