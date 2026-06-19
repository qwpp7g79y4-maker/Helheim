use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::ast::CodeTaal;
use crate::persistence;

use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub globals: std::collections::BTreeMap<String, HelheimType>,
    pub local_stack: Vec<std::collections::BTreeMap<String, HelheimType>>,
    pub func_store: std::collections::BTreeMap<String, String>,
    pub ast_funcs: std::collections::BTreeMap<String, (Vec<String>, Box<CodeTaal>, bool)>,
    pub model_store: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HelheimType {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Dict(serde_json::Map<String, serde_json::Value>),
    List(Vec<serde_json::Value>),
    Tensor(Vec<f32>),
    /// Raw bytes (for TCP primitives etc.)
    Bytes(Vec<u8>),
    /// Opaque handle to a runtime resource (TcpStream, listener, etc.).
    /// Never serialized across nodes or persisted; local to Executor.
    ResourceHandle { kind: String, id: u64 },
    /// Raw foreign pointer for zero-cost FFI.
    Pointer(u64),
    Null,
}

impl std::fmt::Display for HelheimType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HelheimType::String(s) => write!(f, "{}", s),
            HelheimType::Int(i) => write!(f, "{}", i),
            HelheimType::Float(fl) => {
                if fl.fract() == 0.0 {
                    write!(f, "{}.0", fl)
                } else {
                    write!(f, "{}", fl)
                }
            },
            HelheimType::Bool(b) => write!(f, "{}", if *b { "waar" } else { "onwaar" }),
            HelheimType::Dict(d) => write!(f, "{}", serde_json::to_string(d).unwrap_or_else(|_| "{}".to_string())),
            HelheimType::List(l) => write!(f, "{}", serde_json::to_string(l).unwrap_or_else(|_| "[]".to_string())),
            HelheimType::Tensor(t) => write!(f, "tensor({:?})", t),
            HelheimType::Bytes(b) => {
                if let Ok(s) = std::str::from_utf8(b) {
                    if s.chars().all(|c| c.is_ascii_graphic() || c == ' ' || c == '\r' || c == '\n') {
                        return write!(f, "b\"{}\"", s);
                    }
                }
                let hex: Vec<String> = b.iter().map(|byte| format!("{:02x}", byte)).collect();
                write!(f, "b[{}]", hex.join(" "))
            }
            HelheimType::ResourceHandle { kind, id } => write!(f, "handle({}:{})", kind, id),
            HelheimType::Pointer(addr) => write!(f, "ptr(0x{:x})", addr),
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

        // Basic support for byte literal display forms (b"..." or b[hex])
        if s.starts_with("b\"") && s.ends_with('"') && s.len() >= 3 {
            let inner = &s[2..s.len()-1];
            return HelheimType::Bytes(inner.as_bytes().to_vec());
        }
        if s.starts_with("b[") && s.ends_with(']') {
            let inner = &s[2..s.len()-1];
            let parts: Vec<&str> = inner.split_whitespace().collect();
            let mut bytes = Vec::new();
            for p in parts {
                if let Ok(b) = u8::from_str_radix(p, 16) {
                    bytes.push(b);
                }
            }
            if !bytes.is_empty() {
                return HelheimType::Bytes(bytes);
            }
        }
        if s.starts_with("handle(") && s.ends_with(')') {
            // ResourceHandle - parse kind:id , but id is runtime only; treat as opaque for now
            let inner = &s[7..s.len()-1];
            if let Some((kind, id_str)) = inner.split_once(':') {
                if let Ok(id) = id_str.parse::<u64>() {
                    return HelheimType::ResourceHandle { kind: kind.to_string(), id };
                }
            }
        }
        if s.starts_with("ptr(0x") && s.ends_with(')') {
            let inner = &s[6..s.len()-1];
            if let Ok(addr) = u64::from_str_radix(inner, 16) {
                return HelheimType::Pointer(addr);
            }
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
    pub ast_funcs: Arc<DashMap<String, (Vec<String>, Box<CodeTaal>, bool)>>,
    pub model_store: Arc<DashMap<String, Vec<String>>>,
    // For time-travel REPL (Vraag 5)
    history: Arc<Mutex<Vec<MemorySnapshot>>>,
}

impl MemoryManager {
    pub fn new() -> Self {
        let (globals, funcs) = match persistence::MemoryState::load_sync() {
            Ok(state) => {
                tracing::info!("[MEMORY]: 🧠 Local CLI Cache geladen.");
                tracing::info!("          > {} variabelen", state.globals.len());
                tracing::info!("          > {} functies", state.functions.len());
                (state.globals, state.functions)
            }
            Err(e) => {
                tracing::info!("[MEMORY]: Geen vorig geheugen gevonden of corrupt ({})", e);
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
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn spawn_daemon_memory(&self) -> Arc<Self> {
        Arc::new(Self {
            globals: self.globals.clone(),
            local_stack: Arc::new(Mutex::new(Vec::new())),
            func_store: self.func_store.clone(),
            ast_funcs: self.ast_funcs.clone(),
            model_store: self.model_store.clone(),
            history: self.history.clone(),
        })
    }

    pub fn spawn_isolated(snapshot: &MemorySnapshot) -> Arc<Self> {
        // Maak volledig nieuwe, schone DashMaps, niets gedeeld met host
        let fresh = Arc::new(Self {
            globals: Arc::new(DashMap::new()),
            local_stack: Arc::new(Mutex::new(Vec::new())),
            func_store: Arc::new(DashMap::new()),
            ast_funcs: Arc::new(DashMap::new()),
            model_store: Arc::new(DashMap::new()),
            history: Arc::new(Mutex::new(Vec::new())),
        });
        fresh.restore_snapshot(snapshot);
        fresh
    }

    pub fn push_scope(&self) {
        let mut store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        store.push(HashMap::new());
    }

    pub fn pop_scope(&self) {
        let mut store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        if !store.is_empty() {
            store.pop();
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
        tracing::debug!("[CACHE]: Bezig met opslaan naar persistent geheugen...");
        
        let mut globals_map = std::collections::HashMap::new();
        for entry in self.globals.iter() {
            globals_map.insert(entry.key().clone(), entry.value().to_string());
        }

        let mut funcs_map = std::collections::HashMap::new();
        for entry in self.func_store.iter() {
            funcs_map.insert(entry.key().clone(), entry.value().clone());
        }

        match persistence::MemoryState::save(&globals_map, &funcs_map).await {
            Ok(msg) => tracing::debug!("✅ {}", msg),
            Err(e) => tracing::error!("❌ Opslaan mislukt: {}", e),
        }
    }

    pub async fn recall(&self) {
        tracing::debug!("[CACHE]: Geheugen opnieuw laden...");
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
                
                tracing::debug!(
                    "✅ Geheugen hersteld ({} vars, {} funcs)",
                    self.globals.len(),
                    self.func_store.len()
                );
            }
            Err(e) => tracing::error!("❌ Laden mislukt: {}", e),
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

        if let Some(start) = final_token.find('[') {
            if final_token.ends_with(']') {
                key = &final_token[..start];
                index_str = Some(&final_token[start + 1..final_token.len() - 1]);
            }
        }

        if let Some(val) = self.get_var(key) {
            if let Some(idx_s) = index_str {
                let clean_idx = idx_s.trim_matches('"');
                if let Ok(idx) = clean_idx.parse::<usize>() {
                    // Array Indexing
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&val) {
                        if idx < arr.len() {
                            if let Some(s) = arr[idx].as_str() {
                                return s.to_string();
                            }
                            return arr[idx].to_string();
                        }
                    }
                }
                // Dictionary Label Lookup
                if let Ok(map) =
                    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&val)
                {
                    if let Some(res) = map.get(clean_idx) {
                        if let Some(s) = res.as_str() {
                            return s.to_string();
                        }
                        return res.to_string();
                    }
                }
            }
            val.clone()
        } else {
            final_token
        }
    }

    // === Time-Travel REPL support (Vraag 5) ===
    pub fn take_snapshot(&self) -> MemorySnapshot {
        let globals: std::collections::BTreeMap<_, _> = self.globals.iter().map(|e| (e.key().clone(), e.value().clone())).collect();
        let func_store: std::collections::BTreeMap<_, _> = self.func_store.iter().map(|e| (e.key().clone(), e.value().clone())).collect();
        let ast_funcs: std::collections::BTreeMap<_, _> = self.ast_funcs.iter().map(|e| (e.key().clone(), (e.value().0.clone(), e.value().1.clone(), e.value().2))).collect();
        let model_store: std::collections::BTreeMap<_, _> = self.model_store.iter().map(|e| (e.key().clone(), e.value().clone())).collect();

        let mut local_stack = Vec::new();
        let store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        for scope in store.iter() {
            let mut tree_scope = std::collections::BTreeMap::new();
            for (k, v) in scope {
                tree_scope.insert(k.clone(), v.clone());
            }
            local_stack.push(tree_scope);
        }

        MemorySnapshot {
            globals,
            local_stack,
            func_store,
            ast_funcs,
            model_store,
        }
    }

    pub fn snapshot(&self) {
        let snap = self.take_snapshot();

        let mut hist = self.history.lock().unwrap_or_else(|e| e.into_inner());
        hist.push(snap);
        if hist.len() > 100 {
            hist.remove(0); // limit history to prevent unbounded memory
        }
    }

    pub fn rollback(&self, steps: usize) -> bool {
        let mut hist = self.history.lock().unwrap_or_else(|e| e.into_inner());
        if steps == 0 || steps > hist.len() {
            return false;
        }
        // Restore state from 'steps' ago
        let snap = hist[hist.len() - steps].clone();
        let new_len = hist.len() - steps;
        hist.truncate(new_len);
        self.restore_snapshot(&snap);
        true
    }

    pub fn restore_snapshot(&self, snap: &MemorySnapshot) {
        self.globals.clear();
        for (k, v) in &snap.globals {
            self.globals.insert(k.clone(), v.clone());
        }
        self.func_store.clear();
        for (k, v) in &snap.func_store {
            self.func_store.insert(k.clone(), v.clone());
        }
        self.ast_funcs.clear();
        for (k, v) in &snap.ast_funcs {
            self.ast_funcs.insert(k.clone(), (v.0.clone(), v.1.clone(), v.2));
        }
        self.model_store.clear();
        for (k, v) in &snap.model_store {
            self.model_store.insert(k.clone(), v.clone());
        }
        
        let mut store = self.local_stack.lock().unwrap_or_else(|e| e.into_inner());
        store.clear();
        for scope in &snap.local_stack {
            let mut hash_scope = HashMap::new();
            for (k, v) in scope {
                hash_scope.insert(k.clone(), v.clone());
            }
            store.push(hash_scope);
        }
    }

    /// Fase C - Qualified name registry (O(1) DashMap lookup zonder string hacks)
    pub fn register_ast_function(&self, ns: Option<&str>, name: String, params: Vec<String>, body: Box<CodeTaal>, is_pub: bool) {
        let qualified_name = if let Some(namespace) = ns {
            format!("{}::{}", namespace, name)
        } else {
            name
        };
        self.ast_funcs.insert(qualified_name, (params, body, is_pub));
    }

    pub fn register_model(&self, ns: Option<&str>, name: String, fields: Vec<String>) {
        let qualified_name = if let Some(namespace) = ns {
            format!("{}::{}", namespace, name)
        } else {
            name
        };
        self.model_store.insert(qualified_name, fields);
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
