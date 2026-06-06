use std::sync::Arc;
use anyhow::Result;
use crate::orchestra::memory::{MemoryManager, HelheimType};

pub struct SystemManager;

impl SystemManager {
    pub async fn try_execute_native(memory: &Arc<MemoryManager>, name: &str, args: &[String], ctx: &crate::common::context::ExecutionContext) -> Result<Option<String>> {
        // --- NATIVE STD LIB ---
        if name == "wacht" && args.len() == 1 {
            let secs_str = memory.resolve_value(&args[0]);
            if let Ok(secs) = secs_str.parse::<u64>() {
                println!("[ASYNC]: Wachten voor {} seconden...", secs);
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
            return Ok(Some("".to_string()));
        }

        if name == "voeg_toe" && args.len() == 2 {
            let list_name = &args[0]; // Expecting the raw variable name
            let item = memory.resolve_value(&args[1]);
            let list_val = memory.resolve_value(list_name);

            if let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(&list_val) {
                if let Ok(num) = item.parse::<f64>() {
                    if num.fract() == 0.0 {
                        arr.push(serde_json::json!(num as i64));
                    } else {
                        arr.push(serde_json::json!(num));
                    }
                } else {
                    arr.push(serde_json::json!(item));
                }
                let new_list = serde_json::to_string(&arr).unwrap();

                memory.set_var_native(list_name.clone(), HelheimType::parse(&new_list));
                return Ok(Some(new_list));
            }
        }

        if name == "verwijder" && args.len() == 2 {
            let list_name = &args[0];
            let index_val = memory.resolve_value(&args[1]);
            let list_val = memory.resolve_value(list_name);

            if let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(&list_val)
                && let Ok(idx) = index_val.parse::<usize>()
                    && idx < arr.len() {
                        arr.remove(idx);
                        let new_list = serde_json::to_string(&arr).unwrap();
                        memory.set_var_native(list_name.clone(), HelheimType::parse(&new_list));
                        return Ok(Some(new_list));
                    }
        }

        // --- STD LIB: TEKST EN LIJST ---
        if name == "lengte" && args.len() == 1 {
            let val = memory.resolve_value(&args[0]);
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&val) {
                return Ok(Some(arr.len().to_string()));
            } else {
                return Ok(Some(val.trim_matches('"').len().to_string()));
            }
        }
        if name == "tekst.lengte" && args.len() == 1 {
            let s = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            return Ok(Some(s.len().to_string()));
        }
        if name == "tekst.vervang" && args.len() == 3 {
            let s = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            let zoek = memory.resolve_value(&args[1]).trim_matches('"').to_string();
            let vervang = memory.resolve_value(&args[2]).trim_matches('"').to_string();
            return Ok(Some(s.replace(&zoek, &vervang)));
        }
        if name == "tekst.hoofdletters" && args.len() == 1 {
            let s = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            return Ok(Some(s.to_uppercase()));
        }
        if name == "tekst.splitsen" && args.len() == 2 {
            let s = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            let delimeter = memory.resolve_value(&args[1]).trim_matches('"').to_string();
            let parts: Vec<String> = s.split(&delimeter).map(|p| p.to_string()).collect();
            let json_arr = serde_json::to_string(&parts).unwrap_or_else(|_| "[]".to_string());
            return Ok(Some(json_arr));
        }

        // --- STD LIB: WISKUNDE ---
        if name == "wiskunde.willekeurig" && args.len() == 2 {
            let min_val = memory.resolve_value(&args[0]).trim_matches('"').parse::<i64>().unwrap_or(0);
            let max_val = memory.resolve_value(&args[1]).trim_matches('"').parse::<i64>().unwrap_or(100);
            if min_val <= max_val {
                use rand::Rng;
                let mut rng = rand::rng();
                let random_num: i64 = rng.random_range(min_val..=max_val);
                return Ok(Some(random_num.to_string()));
            } else {
                return Ok(Some("0".to_string()));
            }
        }
        if name == "wiskunde.afronden" && args.len() == 1 {
            let val = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            if let Ok(num) = val.parse::<f64>() {
                return Ok(Some(num.round().to_string()));
            }
        }

        // --- STD LIB: JSON ---
        if name == "json.ontleden" && args.len() == 1 {
            let mut s = memory.resolve_value(&args[0]);
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                s = s[1..s.len() - 1].to_string();
            }
            s = s.replace("\\\"", "\"").replace("\\n", "\n");
            
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s) {
                return Ok(Some(parsed.to_string()));
            } else {
                return Ok(Some(s));
            }
        }
        if name == "json.tekst" && args.len() == 1 {
            let s = memory.resolve_value(&args[0]);
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s) {
                return Ok(Some(serde_json::to_string(&parsed).unwrap_or(s)));
            } else {
                return Ok(Some(format!("\"{}\"", s)));
            }
        }
        // --- STD LIB: DICTIONARY ---
        if name == "dic.lees" && args.len() == 2 {
            let json_str = memory.resolve_value(&args[0]);
            let key = memory.resolve_value(&args[1]).trim_matches('"').to_string();
            
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(val) = parsed.get(&key) {
                    if val.is_string() {
                        return Ok(Some(val.as_str().unwrap().to_string()));
                    } else {
                        return Ok(Some(val.to_string()));
                    }
                }
            }
            return Ok(Some("null".to_string()));
        }
        if name == "dic.schrijf" && args.len() == 3 {
            let json_str = memory.resolve_value(&args[0]);
            let key = memory.resolve_value(&args[1]).trim_matches('"').to_string();
            let value_str = memory.resolve_value(&args[2]);
            
            let value_json: serde_json::Value = if let Ok(v) = serde_json::from_str(&value_str) {
                v
            } else {
                if value_str.starts_with('"') && value_str.ends_with('"') {
                    serde_json::Value::String(value_str[1..value_str.len() - 1].to_string())
                } else {
                    serde_json::Value::String(value_str)
                }
            };

            let mut parsed = if let Ok(p) = serde_json::from_str::<serde_json::Value>(&json_str) {
                p
            } else {
                serde_json::json!({})
            };

            if let Some(obj) = parsed.as_object_mut() {
                obj.insert(key, value_json);
            }
            
            return Ok(Some(serde_json::to_string(&parsed).unwrap_or(json_str)));
        }
        // --- STD LIB: BESTANDSBEHEER ---
        if name == "bestand.lees" && args.len() == 1 {
            let path = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            if !ctx.is_privileged {
                if path.contains("../") {
                    return Err(anyhow::anyhow!("[SECURITY]: Path Traversal gedetecteerd."));
                }
                if !path.starts_with("./sandbox/") && !path.starts_with("/var/lib/helheim/sandbox/") {
                    return Err(anyhow::anyhow!("[SECURITY]: Bestandstoegang buiten sandbox geweigerd."));
                }
            }
            match std::fs::read_to_string(&path) {
                Ok(content) => return Ok(Some(content)),
                Err(e) => {
                    println!("[ERR]: bestand.lees - Kan '{}' niet lezen: {}", path, e);
                    return Ok(Some("null".to_string()));
                }
            }
        }
        if name == "bestand.schrijf" && args.len() == 2 {
            let path = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            let content = memory.resolve_value(&args[1]);
            
            if !ctx.is_privileged {
                if path.contains("../") {
                    return Err(anyhow::anyhow!("[SECURITY]: Path Traversal gedetecteerd."));
                }
                if !path.starts_with("./sandbox/") && !path.starts_with("/var/lib/helheim/sandbox/") {
                    return Err(anyhow::anyhow!("[SECURITY]: Bestandstoegang buiten sandbox geweigerd."));
                }
            }

            let mut clean_content = content;
            if clean_content.starts_with('"') && clean_content.ends_with('"') && clean_content.len() >= 2 {
                clean_content = clean_content[1..clean_content.len() - 1].replace("\\\"", "\"").replace("\\n", "\n");
            }

            match std::fs::write(&path, clean_content) {
                Ok(_) => return Ok(Some("waar".to_string())),
                Err(e) => {
                    println!("[ERR]: bestand.schrijf - Kan '{}' niet schrijven: {}", path, e);
                    return Ok(Some("onwaar".to_string()));
                }
            }
        }
        if name == "bestand.lees_binair" && args.len() == 1 {
            let path = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            if !ctx.is_privileged {
                if path.contains("../") {
                    return Err(anyhow::anyhow!("[SECURITY]: Path Traversal gedetecteerd."));
                }
                if !path.starts_with("./sandbox/") && !path.starts_with("/var/lib/helheim/sandbox/") {
                    return Err(anyhow::anyhow!("[SECURITY]: Bestandstoegang buiten sandbox geweigerd."));
                }
            }
            match std::fs::read(&path) {
                Ok(bytes) => {
                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                    let b64 = STANDARD.encode(&bytes);
                    return Ok(Some(b64));
                }
                Err(e) => {
                    println!("[ERR]: bestand.lees_binair - Kan '{}' niet lezen: {}", path, e);
                    return Ok(Some("null".to_string()));
                }
            }
        }
        if name == "bestand.schrijf_binair" && args.len() == 2 {
            let path = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            let content = memory.resolve_value(&args[1]);
            
            if !ctx.is_privileged {
                if path.contains("../") {
                    return Err(anyhow::anyhow!("[SECURITY]: Path Traversal gedetecteerd."));
                }
                if !path.starts_with("./sandbox/") && !path.starts_with("/var/lib/helheim/sandbox/") {
                    return Err(anyhow::anyhow!("[SECURITY]: Bestandstoegang buiten sandbox geweigerd."));
                }
            }

            let mut clean_content = content.trim().to_string();
            if clean_content.starts_with('"') && clean_content.ends_with('"') && clean_content.len() >= 2 {
                clean_content = clean_content[1..clean_content.len() - 1].to_string();
            }

            use base64::{Engine as _, engine::general_purpose::STANDARD};
            match STANDARD.decode(&clean_content) {
                Ok(bytes) => {
                    match std::fs::write(&path, bytes) {
                        Ok(_) => return Ok(Some("waar".to_string())),
                        Err(e) => {
                            println!("[ERR]: bestand.schrijf_binair - Kan '{}' niet schrijven: {}", path, e);
                            return Ok(Some("onwaar".to_string()));
                        }
                    }
                }
                Err(e) => {
                    println!("[ERR]: bestand.schrijf_binair - Ongeldige Base64 data: {}", e);
                    return Ok(Some("onwaar".to_string()));
                }
            }
        }

        // --- STD LIB: SYSTEEM & OS ---
        if name == "systeem.shell" && args.len() == 1 {
            if !ctx.is_privileged {
                return Err(anyhow::anyhow!("[SECURITY]: OS-level Shell vereist Elevated Privileges."));
            }
            let cmd_str = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            match std::process::Command::new("sh").arg("-c").arg(&cmd_str).output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let mut combined = stdout;
                    if !stderr.is_empty() {
                        combined.push_str("\n[STDERR]:\n");
                        combined.push_str(&stderr);
                    }
                    return Ok(Some(combined.trim().to_string()));
                }
                Err(e) => {
                    println!("[ERR]: systeem.shell - Kon commando niet uitvoeren: {}", e);
                    return Ok(Some("null".to_string()));
                }
            }
        }
        if name == "systeem.env" && args.len() == 1 {
            if !ctx.is_privileged {
                return Err(anyhow::anyhow!("[SECURITY]: systeem.env vereist Elevated Privileges."));
            }
            let env_key = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            match std::env::var(&env_key) {
                Ok(val) => return Ok(Some(val)),
                Err(_) => return Ok(Some("null".to_string())),
            }
        }
        if name == "systeem.tijd" {
            let start = std::time::SystemTime::now();
            let since_the_epoch = start
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Tijd ging achteruit");
            return Ok(Some(since_the_epoch.as_secs().to_string()));
        }

        // --- STD LIB: NETWERK ---
        if name == "netwerk.get" && args.len() == 1 {
            let url = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            if !ctx.is_privileged {
                if url.contains("127.0.0.1") || url.contains("localhost") || url.contains("192.168.") || url.contains("10.") || url.contains("169.254.") {
                    return Err(anyhow::anyhow!("[SECURITY]: SSRF Protectie actief. Lokale IPs geblokkeerd."));
                }
            }
            match reqwest::get(&url).await {
                Ok(resp) => {
                    match resp.text().await {
                        Ok(text) => return Ok(Some(text)),
                        Err(e) => {
                            println!("[ERR]: netwerk.get - Kon response tekst niet lezen: {}", e);
                            return Ok(Some("null".to_string()));
                        }
                    }
                }
                Err(e) => {
                    println!("[ERR]: netwerk.get - Fout bij request naar {}: {}", url, e);
                    return Ok(Some("null".to_string()));
                }
            }
        }
        if name == "netwerk.post" && args.len() == 2 {
            let url = memory.resolve_value(&args[0]).trim_matches('"').to_string();
            let body = memory.resolve_value(&args[1]);
            
            if !ctx.is_privileged {
                if url.contains("127.0.0.1") || url.contains("localhost") || url.contains("192.168.") || url.contains("10.") || url.contains("169.254.") {
                    return Err(anyhow::anyhow!("[SECURITY]: SSRF Protectie actief. Lokale IPs geblokkeerd."));
                }
            }

            let mut clean_body = body;
            if clean_body.starts_with('"') && clean_body.ends_with('"') && clean_body.len() >= 2 {
                clean_body = clean_body[1..clean_body.len() - 1].replace("\\\"", "\"").replace("\\n", "\n");
            }

            let client = reqwest::Client::new();
            match client.post(&url)
                .header("Content-Type", "application/json")
                .body(clean_body)
                .send()
                .await 
            {
                Ok(resp) => {
                    match resp.text().await {
                        Ok(text) => return Ok(Some(text)),
                        Err(e) => {
                            println!("[ERR]: netwerk.post - Kon response tekst niet lezen: {}", e);
                            return Ok(Some("null".to_string()));
                        }
                    }
                }
                Err(e) => {
                    println!("[ERR]: netwerk.post - Fout bij request naar {}: {}", url, e);
                    return Ok(Some("null".to_string()));
                }
            }
        }

        // If no native function matches
        Ok(None)
    }
}
