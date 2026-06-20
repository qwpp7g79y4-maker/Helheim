use std::path::Path;
use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;

use helheim_lang::ast::CodeTaal;
use helheim_lang::parser::HelParser;

pub struct PureModule {
    pub module: String,
    pub functions: HashMap<String, (Vec<String>, Box<CodeTaal>)>,
}

pub struct StdLibManager {
    pub pure_modules: DashMap<String, PureModule>,
    pub package_manager: Arc<crate::orchestra::package_manager::PackageManager>,
}

impl StdLibManager {
    pub fn new(package_manager: Arc<crate::orchestra::package_manager::PackageManager>) -> Self {
        Self {
            pure_modules: DashMap::new(),
            package_manager,
        }
    }

    pub async fn bootstrap(&self) -> Result<(), Box<dyn std::error::Error>> {
        let pure_dir = Path::new("stdlib/pure");
        if pure_dir.exists() && pure_dir.is_dir() {
            for entry in std::fs::read_dir(pure_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("hel") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let ns = stem.to_string();
                        let content = std::fs::read_to_string(&path)?;
                        
                        match HelParser::parse(&content) {
                            Ok(ast) => {
                                let mut module = PureModule {
                                    module: ns.clone(),
                                    functions: HashMap::new(),
                                };
                                
                                for node in ast {
                                    if let CodeTaal::FunctionDef { name, is_pub: _, params, body } = node {
                                        // Auto-prefix pure functions if they aren't already namespaced
                                        let final_name = if name.contains("::") {
                                            name.clone()
                                        } else {
                                            format!("{}::{}", ns, name)
                                        };
                                        module.functions.insert(final_name, (params, body));
                                    }
                                }
                                
                                self.pure_modules.insert(ns.clone(), module);
                                tracing::debug!("[STDLIB]: Pure module '{}' geladen (.hel)", ns);
                            }
                            Err(e) => {
                                tracing::error!("[STDLIB]: Fout bij parsen van '{}': {}", path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        // Native modules
        let lib_dir = Path::new("stdlib/lib");
        if lib_dir.exists() && lib_dir.is_dir() {
            for entry in std::fs::read_dir(lib_dir)? {
                let entry = entry?;
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if ext == "wasm" {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let lib_name = stem
                            .strip_prefix("libhelheim_").unwrap_or_else(||
                                stem.strip_prefix("lib").unwrap_or(stem)
                            );
                        
                        // Pass loading to PackageManager as a trusted local module
                        match self.package_manager.import_local_trusted(&lib_name, &path).await {
                            Ok(_) => {
                                // Handled via tracing info internally
                            }
                            Err(e) => {
                                tracing::error!("[STDLIB]: Fout bij laden native plugin '{}': {}", path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
