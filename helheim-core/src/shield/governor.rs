use lazy_static::lazy_static;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

lazy_static! {
    static ref IS_BLACKLISTED: AtomicBool = AtomicBool::new(false);
    static ref COMMAND_HISTORY: Mutex<Vec<(Instant, String)>> = Mutex::new(Vec::new());
}

pub struct Sentinel;

impl Sentinel {
    pub fn check_abuse(cmd: &str) -> bool {
        if IS_BLACKLISTED.load(Ordering::SeqCst) {
            return true;
        }

        let mut history = COMMAND_HISTORY.lock().unwrap();
        let now = Instant::now();

        // Cleanup old history (older than 60s)
        history.retain(|(time, _)| now.duration_since(*time) < Duration::from_secs(60));

        // Add current command
        history.push((now, cmd.to_string()));

        // Abuse Pattern 1: Rapid Stress-Testing (Spamming KETS/GALLOP/BURN)
        let stress_count = history
            .iter()
            .filter(|(_, c)| c.contains("KETS") || c.contains("GALLOP") || c.contains("BURN"))
            .count();

        if stress_count > 10 {
            Self::trigger_revocation("OVERMATIGE_HARDWARE_BELASTING");
            return true;
        }

        // Abuse Pattern 2: Suspicious DEEP Injections (Low-level probe)
        if cmd.starts_with("rune DEEP") && cmd.len() > 100000 {
            Self::trigger_revocation("EXTREEM_GROTE_DEEP_PAYLOAD");
            return true;
        }

        false
    }

    pub fn is_revoked() -> bool {
        IS_BLACKLISTED.load(Ordering::SeqCst)
    }

    fn trigger_revocation(reason: &str) {
        if !IS_BLACKLISTED.load(Ordering::SeqCst) {
            IS_BLACKLISTED.store(true, Ordering::SeqCst);
            println!("\n[🚨 SENTINEL]: MISBRUIK GEDETECTEERD: {}", reason);
            println!("[🚨 SENTINEL]: Toegang tot de Silicon Hive is onmiddellijk ingetrokken.");
            println!("[🚨 SENTINEL]: Ondersteuning en updates zijn geblokkeerd voor deze node.");

            // In een echte productie-omgeving zou dit "terugleidend lijntje" hier een
            // UDP/TCP pulse sturen naar de Master Node (Pieter) om de HW-ID te blacklisten.
            println!("[NATIVE]: Verbiinding met Master Orchestrator verbroken.");
        }
    }
}
