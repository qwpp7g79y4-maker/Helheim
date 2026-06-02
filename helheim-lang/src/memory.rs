use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::ast::CodeTaal;
use crate::persistence;

#[derive(Clone, Debug, PartialEq)]
pub enum HelheimType {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Dict(serde_json::Map<String, serde_json::Value>),
    List(Vec<serde_json::Value>),
    Tensor(Vec<f32>),
    Null,
}

impl std::fmt::Display for HelheimType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HelheimType::String(s) => write!(f, "{}", s),
            HelheimType::Int(i) => write!(f, "{}", i),
            HelheimType::Float(fl) => write!(f, "{}", fl),
            HelheimType::Bool(b) => write!(f, "{}", if *b { "waar" } else { "onwaar" }),
            HelheimType::Dict(d) => write!(f, "{}", serde_json::to_string(d).unwrap_or_else(|_| "{}".to_string())),
            HelheimType::List(l) => write!(f, "{}", serde_json::to_string(l).unwrap_or_else(|_| "[]".to_string())),
            HelheimType::Tensor(t) => write!(f, "tensor({:?})", t),
            HelheimType::Null => write!(f, "null"),
        }
    }
}

impl HelheimType {
    pub fn parse(s: &str) -> Self {
        if s == "waar" || s == "true" { return HelheimType::Bool(true); }
        if s == "onwaar" || s == "false" { return HelheimType::Bool(false); }
        if s == "null" { return HelheimType::Null; }
        
        if let Ok(i) = s.parse::<i64>() { return HelheimType::Int(i); }
        if let Ok(f) = s.parse::<f64>() { return HelheimType::Float(f); }
        
        if s.starts_with("tensor([") && s.ends_with("])") {
            let inner = &s[7..s.len()-1]; // gets "[...]"
            if let Ok(list) = serde_json::from_str::<Vec<f32>>(inner) {
                return HelheimType::Tensor(list);
            }
        }

        if s.starts_with('[') && s.ends_with(']') {
            if let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(s) {
                return HelheimType::List(list);
            }
        }
        
        if s.starts_with('{') && s.ends_with('}') {
            if let Ok(dict) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(s) {
                return HelheimType::Dict(dict);
            }
        }
        
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            return HelheimType::String(s[1..s.len()-1].to_string());
        }
        
        HelheimType::String(s.to_string())
    }
}

#[derive(Clone)]
pub struct MemoryManager {
    pub var_store: Arc<Mutex<Vec<HashMap<String, HelheimType>>>>,
    pub func_store: Arc<Mutex<HashMap<String, String>>>,
    pub ast_funcs: Arc<Mutex<HashMap<String, (Vec<String>, Box<CodeTaal>)>>>,
    pub model_store: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl MemoryManager {
    pub fn new() -> Self {
        let (globals, funcs) = match persistence::MemoryState::load_sync() {
            Ok(state) => {
                println!("[MEMORY]: 🧠 Local CLI Cache geladen.");
                println!("          > {} variabelen", state.globals.len());
                println!("          > {} functies", state.functions.len());
                (state.globals, state.functions)
            }
            Err(e) => {
                println!("[MEMORY]: Geen vorig geheugen gevonden of corrupt ({})", e);
                (HashMap::new(), HashMap::new())
            }
        };

        let mut initial_scope = HashMap::new();
        for (k, v) in globals {
            initial_scope.insert(k, HelheimType::parse(&v));
        }

        Self {
            var_store: Arc::new(Mutex::new(vec![initial_scope])),
            func_store: Arc::new(Mutex::new(funcs)),
            ast_funcs: Arc::new(Mutex::new(HashMap::new())),
            model_store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn push_scope(&self) {
        let mut store = self.var_store.lock().unwrap_or_else(|e| e.into_inner());
        store.push(HashMap::new());
        println!("[SCOPE]: Gepusht naar level {}", store.len());
    }

    pub fn pop_scope(&self) {
        let mut store = self.var_store.lock().unwrap_or_else(|e| e.into_inner());
        if store.len() > 1 {
            store.pop();
            println!("[SCOPE]: Gepopt naar level {}", store.len());
        } else {
            println!("[SCOPE]: Kan globaal scope niet poppen.");
        }
    }

    pub fn get_var_native(&self, key: &str) -> Option<HelheimType> {
        let store = self.var_store.lock().unwrap_or_else(|e| e.into_inner());
        for scope in store.iter().rev() {
            if let Some(val) = scope.get(key) {
                return Some(val.clone());
            }
        }
        None
    }

    pub fn get_var(&self, key: &str) -> Option<String> {
        self.get_var_native(key).map(|v| v.to_string())
    }

    pub fn set_var_native(&self, key: String, value: HelheimType) {
        let mut store = self.var_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(scope) = store.last_mut() {
            scope.insert(key, value);
        }
    }

    pub fn set_var(&self, key: String, value: String) {
        self.set_var_native(key, HelheimType::parse(&value));
    }

    pub async fn persist(&self) {
        println!("[CACHE]: Bezig met opslaan naar persistent geheugen...");
        let (globals, funcs) = {
            let g = self.var_store.lock().unwrap_or_else(|e| e.into_inner());
            let f = self.func_store.lock().unwrap_or_else(|e| e.into_inner());
            let global_scope = if !g.is_empty() {
                let mut stringified = HashMap::new();
                for (k, v) in &g[0] {
                    stringified.insert(k.clone(), v.to_string());
                }
                stringified
            } else {
                HashMap::new()
            };
            (global_scope, f.clone())
        };

        match persistence::MemoryState::save(&globals, &funcs).await {
            Ok(msg) => println!("✅ {}", msg),
            Err(e) => println!("❌ Opslaan mislukt: {}", e),
        }
    }

    pub async fn recall(&self) {
        println!("[CACHE]: Geheugen opnieuw laden...");
        match persistence::MemoryState::load().await {
            Ok(state) => {
                let mut g = self.var_store.lock().unwrap_or_else(|e| e.into_inner());
                let mut f = self.func_store.lock().unwrap_or_else(|e| e.into_inner());
                let mut typed_globals = HashMap::new();
                for (k, v) in state.globals {
                    typed_globals.insert(k, HelheimType::parse(&v));
                }
                *g = vec![typed_globals];
                *f = state.functions;
                println!(
                    "✅ Geheugen hersteld ({} vars, {} funcs)",
                    g[0].len(),
                    f.len()
                );
            }
            Err(e) => println!("❌ Laden mislukt: {}", e),
        }
    }

    pub fn resolve_value(&self, token: &str) -> String {
        // String Interpolation check ({VAR} format)
        let mut final_token = token.to_string();
        if final_token.contains('{') && final_token.contains('}') {
            lazy_static::lazy_static! {
                static ref RE: regex::Regex = regex::Regex::new(r"\{\s*([a-zA-Z0-9_]+)\s*\}").unwrap();
            }
            let mut result = final_token.clone();
            for cap in RE.captures_iter(&final_token) {
                if let Some(mat) = cap.get(1) {
                    let var_name = mat.as_str();
                    if let Some(val) = self.get_var(var_name) {
                        let clean_val = val.trim_matches('"');
                        result = result.replace(&cap[0], clean_val);
                    }
                }
            }
            final_token = result;
        }

        // String Interpolation check ($VAR format)
        if final_token.contains('$') {
            lazy_static::lazy_static! {
                static ref RE_DOLLAR: regex::Regex = regex::Regex::new(r"\$([a-zA-Z0-9_]+)").unwrap();
            }
            let mut result = final_token.clone();
            for cap in RE_DOLLAR.captures_iter(&final_token) {
                if let Some(mat) = cap.get(1) {
                    let var_name = mat.as_str();
                    if let Some(val) = self.get_var(var_name) {
                        let clean_val = val.trim_matches('"');
                        result = result.replace(&cap[0], clean_val);
                    }
                }
            }
            final_token = result;
        }

        let mut key = final_token.as_str();

        // Strip sigil if present (e.g. $Waarde -> Waarde) ONLY if it perfectly matches the whole string. 
        // We already interpolated $VAR inside strings above.
        if key.starts_with('$') && !key[1..].contains(' ') {
            key = &key[1..];
        }

        let mut index_str: Option<&str> = None;

        if let Some(start) = final_token.find('[')
            && final_token.ends_with(']') {
                key = &final_token[..start];
                index_str = Some(&final_token[start + 1..final_token.len() - 1]);
            }

        if let Some(val) = self.get_var(key) {
            if let Some(idx_s) = index_str {
                let clean_idx = idx_s.trim_matches('"');
                if let Ok(idx) = clean_idx.parse::<usize>() {
                    // Array Indexing
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&val)
                        && idx < arr.len() {
                            if let Some(s) = arr[idx].as_str() {
                                return s.to_string();
                            }
                            return arr[idx].to_string();
                        }
                }
                // Dictionary Label Lookup
                if let Ok(map) =
                    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&val)
                    && let Some(res) = map.get(clean_idx) {
                        if let Some(s) = res.as_str() {
                            return s.to_string();
                        }
                        return res.to_string();
                    }
            }
            val.clone()
        } else {
            final_token
        }
    }
}
