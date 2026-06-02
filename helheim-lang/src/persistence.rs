use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

// The complete state of the Helheim Engine
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MemoryState {
    pub globals: HashMap<String, String>,
    pub functions: HashMap<String, String>,
}

impl MemoryState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Determines the standard path for memory.json
    pub fn get_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".helheim").join("memory.json")
    }

    /// Atomic Write: Saves state to disk safely.
    /// 1. Writes to memory.json.tmp
    /// 2. Renames memory.json.tmp -> memory.json
    pub async fn save(
        globals: &HashMap<String, String>,
        functions: &HashMap<String, String>,
    ) -> Result<String> {
        let state = Self {
            globals: globals.clone(),
            functions: functions.clone(),
        };

        let path = Self::get_path();
        let tmp_path = path.with_extension("json.tmp");

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let json = serde_json::to_string_pretty(&state)?;

        // Write to TMP
        fs::write(&tmp_path, json)
            .await
            .context("Failed to write temp memory")?;

        // Rename TMP to FINAL (Atomic on POSIX)
        fs::rename(&tmp_path, &path)
            .await
            .context("Failed to commit memory")?;

        Ok(format!("Memory saved to {:?}", path))
    }

    /// Load state from disk (Synchronous for startup)
    pub fn load_sync() -> Result<Self> {
        let path = Self::get_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path).context("Failed to read memory")?;
        let state: MemoryState = serde_json::from_str(&content).context("Corrupt memory file")?;

        Ok(state)
    }

    /// Load state from disk (Async for runtime)
    pub async fn load() -> Result<Self> {
        let path = Self::get_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .await
            .context("Failed to read memory")?;
        let state: MemoryState = serde_json::from_str(&content).context("Corrupt memory file")?;

        Ok(state)
    }
}
