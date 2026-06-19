//! Helheim FFI / Dynamic Module System
//!
//! This module defines the stable C-ABI for native (C, Zig, Rust, etc.) modules
//! that can be loaded at runtime via `gebruik "modulenaam"`.
//!
//! The goal is complete decoupling of the Standard Library and extension
//! functionality from the core engine. Everything (math, JSON, text, custom
//! protocols, etc.) lives in loadable .so/.dll/.dylib files.
//!
//! # Safety & ABI Contract
//! - All structs are `#[repr(C)]` and must remain stable.
//! - Modules MUST use the allocator provided in `HelFFIContext` for any
//!   complex data (String/List) they return.
//! - Input complex data from Helheim is borrowed (const views). Modules
//!   MUST NOT free it.
//! - Error reporting must go through the `report_error` callback when possible.
//! - Keep the `Library` alive as long as any function pointer from it may be called.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::Arc;

use helheim_lang::memory::HelheimType as LangHelheimType;
use wasmtime::*;

// =============================================================================
// ABI Versioning
// =============================================================================

/// Current FFI ABI version. Modules must check this in `helheim_module_init`.
/// Bump only on breaking layout changes.
pub const HEL_ABI_VERSION: u32 = 1;

// =============================================================================
// Error Codes (standardized)
// =============================================================================

/// Standard error codes returned by module functions and init.
/// Negative values are errors. 0 = success.
pub const HEL_ERR_OK: c_int = 0;
pub const HEL_ERR_GENERIC: c_int = -1;
pub const HEL_ERR_INVALID_ARG: c_int = -2;
pub const HEL_ERR_OUT_OF_MEMORY: c_int = -3;
pub const HEL_ERR_NOT_IMPLEMENTED: c_int = -4;
pub const HEL_ERR_ABI_MISMATCH: c_int = -5;
pub const HEL_ERR_INTERNAL: c_int = -6;

// =============================================================================
// Core FFI Value Types (C ABI)
// =============================================================================

/// Tag for `HelValue`. This enum is part of the stable ABI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelValueTag {
    Null = 0,
    Int = 1,
    Float = 2,
    String = 3,
    List = 4,
    ResourceHandle = 5,
    Pointer = 6,
    // Add new variants only at the end. Never reorder.
}

/// C-compatible string view (len + ptr). Lifetime is managed by the owner
/// (Helheim for inputs, module via context allocator for outputs).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HelString {
    pub ptr: *const u8,
    pub len: usize,
}

/// List of `HelValue`s. Shallow — elements may contain further pointers.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HelList {
    pub ptr: *const HelValue,
    pub len: usize,
}

/// Resource handle (maps directly to Helheim's internal ResourceHandle).
/// The `kind` is a null-terminated string owned by the provider of the handle.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HelResourceHandle {
    pub kind: *const c_char,
    pub id: u64,
}

/// The main FFI value that crosses the boundary.
/// This is the only type modules are allowed to see for arguments and returns.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelValue {
    pub tag: HelValueTag,
    pub data: HelValueData,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union HelValueData {
    pub i: i64,
    pub f: f64,
    pub str: HelString,
    pub list: HelList,
    pub res: HelResourceHandle,
    pub ptr: u64,
    /// Padding to keep the union size stable across platforms (at least 16 bytes).
    pub _pad: [u8; 16],
}

impl HelValue {
    /// Null value constant.
    pub const NULL: HelValue = HelValue {
        tag: HelValueTag::Null,
        data: HelValueData { i: 0 },
    };

    pub fn int(v: i64) -> Self {
        HelValue {
            tag: HelValueTag::Int,
            data: HelValueData { i: v },
        }
    }

    pub fn float(v: f64) -> Self {
        HelValue {
            tag: HelValueTag::Float,
            data: HelValueData { f: v },
        }
    }

    /// Create a borrowed string view (for input to modules).
    /// The caller must ensure `s` outlives the `HelValue`.
    pub fn string_borrowed(s: &str) -> Self {
        HelValue {
            tag: HelValueTag::String,
            data: HelValueData {
                str: HelString {
                    ptr: s.as_ptr(),
                    len: s.len(),
                },
            },
        }
    }

    /// Create a resource handle view.
    pub fn resource(kind: *const c_char, id: u64) -> Self {
        HelValue {
            tag: HelValueTag::ResourceHandle,
            data: HelValueData {
                res: HelResourceHandle { kind, id },
            },
        }
    }

    /// Create a zero-cost pointer.
    pub fn pointer(addr: u64) -> Self {
        HelValue {
            tag: HelValueTag::Pointer,
            data: HelValueData { ptr: addr },
        }
    }
}

// =============================================================================
// FFI Context with Solid Error Reporting
// =============================================================================

/// Context passed to every FFI call. Contains allocator and error reporting.
///
/// This is the primary safety and error channel across the FFI boundary.
/// Modules should use `report_error` for all failures instead of just returning
/// an error code (the code can be accompanied by a human-readable message).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelFFIContext {
    /// Must match `HEL_ABI_VERSION` at load time.
    pub abi_version: u32,

    /// Allocator that modules MUST use for any complex data they return
    /// (String/List buffers). Helheim will call `free` on them.
    pub alloc: Option<extern "C" fn(size: usize, align: usize) -> *mut c_void>,
    pub free: Option<extern "C" fn(ptr: *mut c_void)>,

    /// Solid error reporting channel.
    /// Modules should call this on any failure.
    /// `code` should be one of the HEL_ERR_* constants.
    /// `message` should be a null-terminated UTF-8 string (preferably allocated
    /// via the context allocator if it needs to outlive the call).
    pub report_error: Option<extern "C" fn(ctx: *mut HelFFIContext, code: c_int, message: *const c_char)>,

    /// Optional simple logging (for debug/info from modules).
    pub log: Option<extern "C" fn(message: *const c_char)>,

    /// Opaque pointer back into Helheim runtime (e.g. for advanced resource access).
    /// Modules should treat this as opaque unless they have explicit agreement
    /// with the host.
    pub user_data: *mut c_void,

    /// Internal: last reported error (for simple polling if needed).
    /// Modules should prefer the callback.
    pub last_error_code: c_int,
    pub last_error_message: *const c_char, // may be null
    pub owned_last_error_message: bool, // indicates if we need to free it
}

/// Helper to create a default HelFFIContext for a host.
/// The `user_data` is usually a pointer to the Executor or Orchestrator.
pub fn create_ffi_context(user_data: *mut c_void) -> HelFFIContext {
    HelFFIContext {
        abi_version: HEL_ABI_VERSION,
        alloc: None, // Filled by the actual loader / host
        free: None,
        report_error: None,
        log: None,
        user_data,
        last_error_code: HEL_ERR_OK,
        last_error_message: std::ptr::null(),
        owned_last_error_message: false,
    }
}

/// Convenience for modules to report an error through the context.
pub unsafe fn report_error(ctx: *mut HelFFIContext, code: c_int, msg: &str) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: caller guarantees ctx is a valid HelFFIContext for the duration of the call.
    let ctx_ref = unsafe { &mut *ctx };

    // Store last error for polling
    ctx_ref.last_error_code = code;

    if ctx_ref.owned_last_error_message && !ctx_ref.last_error_message.is_null() {
        let _ = unsafe { CString::from_raw(ctx_ref.last_error_message as *mut c_char) };
    }

    let c_msg = CString::new(msg).unwrap_or_else(|_| CString::new("invalid utf8 in error").unwrap());
    ctx_ref.last_error_message = c_msg.into_raw();
    ctx_ref.owned_last_error_message = true;

    if let Some(reporter) = ctx_ref.report_error {
        // We pass a temporary pointer. The reporter must copy if it needs to keep it.
        reporter(ctx, code, ctx_ref.last_error_message);
    }
}

// =============================================================================
// Function Table & Module Registration
// =============================================================================

/// Signature for a native Helheim function exposed over FFI.
pub type HelFunctionCall = extern "C" fn(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int;

/// Description of one function exported by a native module.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelFunctionDesc {
    pub name: *const c_char,
    pub arity: u32,
    pub call: HelFunctionCall,
}

/// Table of functions returned by a module (via `helheim_get_function_table`).
#[repr(C)]
pub struct HelFunctionTable {
    pub count: u32,
    pub entries: *const HelFunctionDesc,
}

// =============================================================================
// Wasm Module Loader (wasmtime)
// =============================================================================

/// A loaded Wasm module.
pub struct LoadedWasmModule {
    pub name: String,
    // Provide an empty/stub 'functions' hashmap for now so the executor doesn't break compiling.
    // In a fully implemented Phase 2, this will be mapped to wasmtime::TypedFuncs.
    pub functions: HashMap<String, HelFunctionCall>,
    pub context: std::sync::Mutex<HelFFIContext>,
}

unsafe impl Send for LoadedWasmModule {}
unsafe impl Sync for LoadedWasmModule {}

/// Loads and manages Wasm modules at runtime using Wasmtime.
pub struct WasmModuleLoader {
    search_paths: Vec<PathBuf>,
    loaded: HashMap<String, Arc<LoadedWasmModule>>,
    engine: Engine,
}

impl WasmModuleLoader {
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        let mut config = Config::new();
        config.wasm_component_model(true); // Phase 2: Wasm Component Model support
        let engine = Engine::new(&config).expect("Failed to initialize Wasmtime Engine");
        
        Self {
            search_paths,
            loaded: HashMap::new(),
            engine,
        }
    }

    pub fn add_search_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    pub fn load(
        &mut self,
        module_name: &str,
        user_data: *mut c_void,
    ) -> anyhow::Result<Arc<LoadedWasmModule>> {
        if let Some(existing) = self.loaded.get(module_name) {
            return Ok(existing.clone());
        }

        let candidates = self.find_library_candidates(module_name);
        
        for candidate in candidates {
            if candidate.exists() {
                // Here we would normally compile the Wasm module:
                // let module = Module::from_file(&self.engine, &candidate)?;
                // let mut store = Store::new(&self.engine, ());
                // let instance = Instance::new(&mut store, &module, &[])?;
                
                // For now, return a stub loaded module to keep Helheim compiling
                let ctx = create_ffi_context(user_data);
                
                let loaded = Arc::new(LoadedWasmModule {
                    name: module_name.to_string(),
                    functions: HashMap::new(), // Stub: no functions loaded yet
                    context: std::sync::Mutex::new(ctx),
                });
                
                self.loaded.insert(module_name.to_string(), loaded.clone());
                tracing::info!("Mock-Loaded WASM Sandbox for '{}'", module_name);
                return Ok(loaded);
            }
        }
        
        anyhow::bail!("Failed to find .wasm plugin for module '{}'", module_name);
    }

    fn find_library_candidates(&self, module_name: &str) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        let base_names = vec![
            format!("{}.wasm", module_name),
            format!("lib{}.wasm", module_name),
        ];

        for base in &self.search_paths {
            for name in &base_names {
                candidates.push(base.join(name));
            }
        }
        candidates
    }

    pub fn get(&self, module_name: &str) -> Option<Arc<LoadedWasmModule>> {
        self.loaded.get(module_name).cloned()
    }

    pub fn unload(&mut self, module_name: &str) -> bool {
        self.loaded.remove(module_name).is_some()
    }

    pub fn reload(
        &mut self,
        module_name: &str,
        user_data: *mut c_void,
    ) -> anyhow::Result<Arc<LoadedWasmModule>> {
        self.unload(module_name);
        self.load(module_name, user_data)
    }
}

/// Default error reporter implementation that just stores the error in the context.
extern "C" fn default_error_reporter(ctx: *mut HelFFIContext, code: c_int, message: *const c_char) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        let ctx_ref = &mut *ctx;
        ctx_ref.last_error_code = code;
        ctx_ref.last_error_message = message;

        // Also log if possible
        if let Some(log_fn) = ctx_ref.log {
            if !message.is_null() {
                log_fn(message);
            }
        }
    }
}

extern "C" fn default_ffi_alloc(size: usize, _align: usize) -> *mut c_void {
    unsafe { libc::malloc(size) as *mut c_void }
}

extern "C" fn default_ffi_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        unsafe { libc::free(ptr as *mut libc::c_void) }
    }
}

// End of ffi.rs
// The types are re-exported from the crate root in lib.rs for external use.

// =============================================================================
// Marshal / Unmarshal Helpers (HelheimType <-> HelValue)
// Careful with ownership:
// - marshal (Helheim -> FFI for call args): uses borrowed views where possible.
//   Complex data (String/List) points into the original HelheimType data.
//   The FFI call must complete before the original data is dropped.
// - unmarshal (FFI -> Helheim for return values): copies data into owned
//   HelheimType. Frees the original buffers allocated by the plugin via ctx->free.
// =============================================================================

/// Convert a HelheimType into a HelValue suitable for passing as argument
/// to a native module function.
///
/// For String/List/Bytes: creates a borrowed view (pointer into the source data).
/// Caller must ensure the source `ht` lives until the FFI call returns.
///
/// ResourceHandle kind is temporarily leaked (small CString) for the duration
/// of the call. This is acceptable for short-lived FFI calls.
pub unsafe fn marshal_helheimtype_to_helvalue(
    ht: &LangHelheimType,
    _ctx: *mut HelFFIContext, // reserved for future allocator use in marshal
) -> HelValue {
    match ht {
        LangHelheimType::Int(i) => HelValue::int(*i),
        LangHelheimType::Float(f) => HelValue::float(*f),
        LangHelheimType::Bool(b) => HelValue::int(if *b { 1 } else { 0 }),
        LangHelheimType::String(s) => HelValue::string_borrowed(s),
        LangHelheimType::ResourceHandle { kind, id } => {
            let ptr = match kind.as_str() {
                "tcp" => b"tcp\0".as_ptr() as *const c_char,
                "file" => b"file\0".as_ptr() as *const c_char,
                "sqlite" => b"sqlite\0".as_ptr() as *const c_char,
                "gpu" => b"gpu\0".as_ptr() as *const c_char,
                _ => b"unknown\0".as_ptr() as *const c_char,
            };
            HelValue::resource(ptr, *id)
        }
        LangHelheimType::Pointer(addr) => HelValue::pointer(*addr),
        LangHelheimType::List(items) => {
            let ffi_elems: Vec<HelValue> = items
                .iter()
                .map(|jv| {
                    if let Some(i) = jv.as_i64() {
                        HelValue::int(i)
                    } else if let Some(f) = jv.as_f64() {
                        HelValue::float(f)
                    } else if let Some(s) = jv.as_str() {
                        HelValue::string_borrowed(s)
                    } else if let Some(b) = jv.as_bool() {
                        HelValue::int(if b { 1 } else { 0 })
                    } else {
                        HelValue::NULL
                    }
                })
                .collect();

            let len = ffi_elems.len();
            let size = len * std::mem::size_of::<HelValue>();
            let mut ptr: *mut HelValue = std::ptr::null_mut();
            if !_ctx.is_null() {
                let ctx_ref = unsafe { &*_ctx };
                if let Some(alloc_fn) = ctx_ref.alloc {
                    ptr = alloc_fn(size, std::mem::align_of::<HelValue>()) as *mut HelValue;
                }
            }
            if ptr.is_null() {
                ptr = unsafe { libc::malloc(size) } as *mut HelValue;
            }
            if !ptr.is_null() {
                unsafe { std::ptr::copy_nonoverlapping(ffi_elems.as_ptr(), ptr, len); }
            }

            HelValue {
                tag: HelValueTag::List,
                data: HelValueData {
                    list: HelList { ptr, len },
                },
            }
        }
        LangHelheimType::Bytes(b) => {
            let ffi_elems: Vec<HelValue> = b.iter().map(|&byte| HelValue::int(byte as i64)).collect();
            let len = ffi_elems.len();
            let size = len * std::mem::size_of::<HelValue>();
            let mut ptr: *mut HelValue = std::ptr::null_mut();
            if !_ctx.is_null() {
                let ctx_ref = unsafe { &*_ctx };
                if let Some(alloc_fn) = ctx_ref.alloc {
                    ptr = alloc_fn(size, std::mem::align_of::<HelValue>()) as *mut HelValue;
                }
            }
            if ptr.is_null() {
                ptr = unsafe { libc::malloc(size) } as *mut HelValue;
            }
            if !ptr.is_null() {
                unsafe { std::ptr::copy_nonoverlapping(ffi_elems.as_ptr(), ptr, len); }
            }

            HelValue {
                tag: HelValueTag::List,
                data: HelValueData {
                    list: HelList { ptr, len },
                },
            }
        }
        LangHelheimType::Dict(_) | LangHelheimType::Tensor(_) | LangHelheimType::Null => HelValue::NULL,
    }
}

/// Convert a HelValue received from a native module back into a HelheimType.
///
/// For String and List: copies the data into owned structures.
/// Then frees the original memory that the module allocated using ctx->alloc
/// (via ctx->free). This prevents leaks when Helheim drops the variable quickly.
pub unsafe fn unmarshal_helvalue_to_helheimtype(
    hv: HelValue,
    ctx: *mut HelFFIContext,
) -> LangHelheimType { unsafe {
    if ctx.is_null() {
        return LangHelheimType::Null;
    }

    let free_fn = (*ctx).free;

    match hv.tag {
        HelValueTag::Null => LangHelheimType::Null,
        HelValueTag::Int => LangHelheimType::Int(hv.data.i),
        HelValueTag::Float => LangHelheimType::Float(hv.data.f),
        HelValueTag::String => {
            if hv.data.str.ptr.is_null() {
                return LangHelheimType::String(String::new());
            }
            let slice = std::slice::from_raw_parts(hv.data.str.ptr, hv.data.str.len);
            let owned = String::from_utf8_lossy(slice).into_owned();

            // Free the buffer the plugin allocated for this string
            if let Some(free) = free_fn {
                free(hv.data.str.ptr as *mut c_void);
            }

            LangHelheimType::String(owned)
        }
        HelValueTag::List => {
            if hv.data.list.ptr.is_null() || hv.data.list.len == 0 {
                // Still free the (empty) array if present
                if let Some(free) = free_fn {
                    if !hv.data.list.ptr.is_null() {
                        free(hv.data.list.ptr as *mut c_void);
                    }
                }
                return LangHelheimType::List(vec![]);
            }

            let elems = std::slice::from_raw_parts(hv.data.list.ptr, hv.data.list.len);
            let mut json_items = Vec::with_capacity(elems.len());

            for &elem in elems {
                let item = unmarshal_helvalue_to_helheimtype(elem, ctx);
                let jv = match item {
                    LangHelheimType::Int(i) => serde_json::json!(i),
                    LangHelheimType::Float(f) => serde_json::json!(f),
                    LangHelheimType::String(s) => serde_json::json!(s),
                    LangHelheimType::Bool(b) => serde_json::json!(b),
                    LangHelheimType::List(sub) => serde_json::json!(sub),
                    LangHelheimType::Bytes(b) => serde_json::json!(b),
                    _ => serde_json::json!(null),
                };
                json_items.push(jv);
            }

            // Free the array of HelValues that the plugin allocated
            if let Some(free) = free_fn {
                free(hv.data.list.ptr as *mut c_void);
            }

            LangHelheimType::List(json_items)
        }
        HelValueTag::ResourceHandle => {
            let kind = if hv.data.res.kind.is_null() {
                String::new()
            } else {
                CStr::from_ptr(hv.data.res.kind)
                    .to_string_lossy()
                    .into_owned()
            };
            LangHelheimType::ResourceHandle {
                kind,
                id: hv.data.res.id,
            }
        }
        HelValueTag::Pointer => LangHelheimType::Pointer(hv.data.ptr),
    }
}}