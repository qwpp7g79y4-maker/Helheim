use anyhow::{Result, anyhow};
// use std::io::Read;

/// De Helheim HTTP Module
/// Lichtgewicht web-requests (zonder de zwaarte van reqwest).
pub struct HttpManager;

impl HttpManager {
    /// Haalt content van een URL (GET request).
    /// Voorbeeld: get("https://api.ipify.org")
    pub fn get(url: &str) -> Result<String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build()
            .into();
        let mut resp = agent.get(url)
            .call()
            .map_err(|e| anyhow!("HTTP Fout bij verbinden met '{}': {}", url, e))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| anyhow!("HTTP Fout bij lezen van response: {}", e))?;

        Ok(body)
    }
}
