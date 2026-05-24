use std::fs;
use std::io::Write;
use anyhow::{Result, anyhow};

/// De Helheim File System Module
/// Robuuste I/O voor datalogging en configuratie.
pub struct FileManager;

impl FileManager {
    pub fn read(path: &str) -> Result<String> {
        match fs::read_to_string(path) {
            Ok(content) => Ok(content),
            Err(e) => Err(anyhow!("FS Fout bij lezen van '{}': {}", path, e))
        }
    }

    pub fn write(path: &str, content: &str) -> Result<()> {
        let mut file = fs::File::create(path)
            .map_err(|e| anyhow!("FS Fout bij maken van '{}': {}", path, e))?;
            
        file.write_all(content.as_bytes())
            .map_err(|e| anyhow!("FS Fout bij schrijven: {}", e))?;
            
        Ok(())
    }
}
