use anyhow::{Result, anyhow};
// use std::io::Read;

/// De Helheim HTTP Module
/// Lichtgewicht web-requests (zonder de zwaarte van reqwest).
pub struct HttpManager;

impl HttpManager {
    /// Haalt content van een URL (GET request).
    /// Voorbeeld: get("https://api.ipify.org")
    pub fn get(url: &str) -> Result<String> {
        // Ureq is synchroon en blokkeert, wat prima is voor CLI scripts.
        let mut resp = ureq::get(url)
            .call()
            .map_err(|e| anyhow!("HTTP Fout bij verbinden met '{}': {}", url, e))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| anyhow!("HTTP Fout bij lezen van response: {}", e))?;

        Ok(body)
    }
}
