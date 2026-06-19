use std::time::Instant;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub is_privileged: bool,
    pub is_distributed: bool,
    pub start_time: Instant,
    pub current_module: Option<String>,
    pub gas_limit: Option<u64>,
    pub gas_consumed: Arc<AtomicU64>,
}

impl ExecutionContext {
    pub fn default_privileged() -> Self {
        Self {
            is_privileged: true,
            is_distributed: false,
            start_time: Instant::now(),
            current_module: None,
            gas_limit: None,
            gas_consumed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn sandbox() -> Self {
        Self {
            is_privileged: false,
            is_distributed: false,
            start_time: Instant::now(),
            current_module: None,
            gas_limit: Some(1_000_000), // Default sandbox gas limit
            gas_consumed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn check_timeout(&self) -> anyhow::Result<()> {
        if !self.is_privileged && self.start_time.elapsed().as_secs() > 5 {
            anyhow::bail!("TIMEOUT: Sandbox execution limit exceeded (5s)");
        }
        Ok(())
    }

    pub fn consume_gas(&self, amount: u64) -> anyhow::Result<()> {
        if let Some(limit) = self.gas_limit {
            let current = self.gas_consumed.fetch_add(amount, Ordering::Relaxed);
            if current + amount > limit {
                anyhow::bail!("OUT_OF_GAS: Execution exceeded the gas limit of {}", limit);
            }
        }
        Ok(())
    }
}
