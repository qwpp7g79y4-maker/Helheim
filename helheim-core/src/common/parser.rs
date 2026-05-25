use crate::orchestra::synthesis::CodeTaal;
use anyhow::Result;
use std::collections::HashMap;

/// De Hel-Lang Parser
/// Zet .hel source code om naar Code-Taal (Logic Seeds)
pub struct Parser;

#[derive(Debug)]
pub struct HelProgram {
    pub materie: HashMap<String, Vec<usize>>, // Variabelen & Dimensies
    pub stroom: Vec<CodeTaal>,                // Executie Logica
}

impl Parser {
    pub fn parse(source: &str) -> Result<HelProgram> {
        let mut program = HelProgram {
            materie: HashMap::new(),
            stroom: Vec::new(),
        };

        let mut in_stroom = false;

        for line in source.lines() {
            let clean = line.trim();
            if clean.is_empty() || clean.starts_with("//") {
                continue;
            }

            if clean.contains("MATERIE {") {
                continue;
            }
            if clean.contains("STROOM {") {
                in_stroom = true;
                continue;
            }
            if clean == "}" {
                continue;
            }

            if in_stroom {
                // Parse STROOM logica (e.g., $Out = MATMUL($A, $B))
                if clean.contains("MATMUL") {
                    // Voor nu hardcoded demo mapping:
                    program.stroom.push(CodeTaal::MatMul {
                        m: 1024,
                        n: 1024,
                        k: 1024,
                    });
                } else if clean.contains("CHAOS") {
                    program.stroom.push(CodeTaal::Chaos {
                        intensity: 100,
                        duration_ms: 1000,
                    });
                } else if clean.to_lowercase().starts_with("stuur ") {
                    // Syntax: stuur [payload] naar [target]
                    if let Some((payload, target)) = clean[6..].split_once(" naar ") {
                        program.stroom.push(CodeTaal::Send {
                            payload: payload.trim().to_string(),
                            target: target.trim().to_string(),
                        });
                    } else {
                        eprintln!("[PARSER WARNING]: 'stuur' mist doel. Gebruik: stuur X naar Y");
                    }
                } else if clean.to_lowercase().starts_with("shield ") {
                    // Syntax: shield [algo] [data]
                    let parts: Vec<&str> = clean.splitn(3, ' ').collect();
                    if parts.len() == 3 {
                        let algo = parts[1];
                        let data = parts[2];
                        program.stroom.push(CodeTaal::Encrypt {
                            algo: algo.to_string(),
                            data: data.to_string(),
                        });
                    }
                } else if clean.starts_with("lees ") {
                    program.stroom.push(CodeTaal::FileOp {
                        action: "read".to_string(),
                        path: clean[5..].trim().to_string(),
                        content: None,
                    });
                } else if clean.starts_with("schrijf ") {
                    if let Some((content, path)) = clean[8..].split_once(" naar ") {
                        program.stroom.push(CodeTaal::FileOp {
                            action: "write".to_string(),
                            path: path.trim().to_string(),
                            content: Some(content.trim().to_string()),
                        });
                    }
                } else if clean.starts_with("voer uit ") {
                    program.stroom.push(CodeTaal::SysOp {
                        command: clean[9..].trim().to_string(),
                    });
                } else if clean.starts_with("haal ") {
                    program.stroom.push(CodeTaal::HttpOp {
                        method: "GET".to_string(),
                        url: clean[5..].trim().to_string(),
                    });
                }
            } else {
                // Parse MATERIE (e.g., Invoer: [1024, 1024])
                if let Some((name, _dim_str)) = clean.split_once(':') {
                    // Simpele stub parsing
                    let dims = vec![1024, 1024];
                    program.materie.insert(name.trim().to_string(), dims);
                }
            }
        }

        Ok(program)
    }
}
