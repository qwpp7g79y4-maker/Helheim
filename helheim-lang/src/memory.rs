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

use dashmap::DashMap;

#[derive(Clone)]
pub struct MemoryManager {
    pub globals: Arc<DashMap<String, HelheimType>>,
    pub local_stack: Arc<Mutex<Vec<HashMap<String, HelheimType>>>>,
    pub func_store: Arc<DashMap<String, String>>,
    pub ast_funcs: Arc<DashMap<String, (Vec<String>, Box<CodeTaal>)>>,
    pub model_store: Arc<DashMap<String, Vec<String>>>,
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

        let globals_map = DashMap::new();
        for (k, v) in globals {
            globals_map.insert(k, HelheimType::parse(&v));
        }

        let func_store = DashMap::new();
        for (k, v) in funcs {
            func_store.insert(k, v);
        }

        Self {
            globals: Arc::new(globals_map),
            local_stack: Arc::new(Mutex::new(Vec::new())),
            func_store: Arc::new(func_store),
            ast_funcs: Arc::new(DashMap::new()),
            model_store: Arc::new(DashMap::new()),
        }
    }

    pub fn spawn_daemon_memory(&self) -> Arc<Self> {
        Arc::new(Self {
            globals: self.globals.clone(),
            local_stack: Arc::new(Mutex::new(Vec::new())),
            func_store: self.func_store.clone(),
            ast_funcs: self.ast_funcs.clone(),
            model_store: self.model_store.clone(),
        })
    }

    pub fn push_scope(&self) {
        let mut store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        store.push(HashMap::new());
        println!("[SCOPE]: Gepusht naar level {}", store.len());
    }

    pub fn pop_scope(&self) {
        let mut store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        if !store.is_empty() {
            store.pop();
            println!("[SCOPE]: Gepopt naar level {}", store.len());
        } else {
            println!("[SCOPE]: Kan globaal scope niet poppen.");
        }
    }

    pub fn get_var_native(&self, key: &str) -> Option<HelheimType> {
        let store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        for scope in store.iter().rev() {
            if let Some(val) = scope.get(key) {
                return Some(val.clone());
            }
        }
        if let Some(v) = self.globals.get(key) {
            return Some(v.value().clone());
        }
        None
    }

    pub fn get_var(&self, key: &str) -> Option<String> {
        self.get_var_native(key).map(|v| v.to_string())
    }

    pub fn set_var_native(&self, key: String, value: HelheimType) {
        let mut store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(scope) = store.last_mut() {
            scope.insert(key, value);
        } else {
            self.globals.insert(key, value);
        }
    }

    pub fn set_var(&self, key: String, value: String) {
        self.set_var_native(key, HelheimType::parse(&value));
    }

    pub async fn persist(&self) {
        println!("[CACHE]: Bezig met opslaan naar persistent geheugen...");
        
        let mut globals_map = std::collections::HashMap::new();
        for entry in self.globals.iter() {
            globals_map.insert(entry.key().clone(), entry.value().to_string());
        }

        let mut funcs_map = std::collections::HashMap::new();
        for entry in self.func_store.iter() {
            funcs_map.insert(entry.key().clone(), entry.value().clone());
        }

        match persistence::MemoryState::save(&globals_map, &funcs_map).await {
            Ok(msg) => println!("✅ {}", msg),
            Err(e) => println!("❌ Opslaan mislukt: {}", e),
        }
    }

    pub async fn recall(&self) {
        println!("[CACHE]: Geheugen opnieuw laden...");
        match persistence::MemoryState::load().await {
            Ok(state) => {
                self.globals.clear();
                for (k, v) in state.globals {
                    self.globals.insert(k, HelheimType::parse(&v));
                }
                
                self.func_store.clear();
                for (k, v) in state.functions {
                    self.func_store.insert(k, v);
                }
                
                println!(
                    "✅ Geheugen hersteld ({} vars, {} funcs)",
                    self.globals.len(),
                    self.func_store.len()
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

/// RAII ScopeGuard.
/// Garandeert dat een lokale scope (voor een functie-aanroep) altijd wordt gepopt,
/// ook bij early return, error (`?`), of panic.
pub struct ScopeGuard<'a> {
    memory: &'a MemoryManager,
    active: bool,
}

impl<'a> ScopeGuard<'a> {
    /// Pusht direct een nieuwe scope en retourneert een guard die bij drop zal poppen.
    pub fn new(memory: &'a MemoryManager) -> Self {
        memory.push_scope();
        Self { memory, active: true }
    }

    /// Vroege pop (bijv. als je de guard expliciet wilt opruimen na een return).
    /// Na deze call doet Drop niets meer.
    pub fn pop_now(mut self) {
        if self.active {
            self.memory.pop_scope();
            self.active = false;
        }
    }
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.memory.pop_scope();
        }
    }
}
