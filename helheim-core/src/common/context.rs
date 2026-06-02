use std::time::Instant;

#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub is_privileged: bool,
    pub start_time: Instant,
}

impl ExecutionContext {
    pub fn default_privileged() -> Self {
        Self {
            is_privileged: true,
            start_time: Instant::now(),
        }
    }

    pub fn sandbox() -> Self {
        Self {
            is_privileged: false,
            start_time: Instant::now(),
        }
    }

    pub fn check_timeout(&self) -> anyhow::Result<()> {
        if !self.is_privileged && self.start_time.elapsed().as_secs() > 5 {
            anyhow::bail!("TIMEOUT: Sandbox execution limit exceeded (5s)");
        }
        Ok(())
    }
}
