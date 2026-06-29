# Dynamic FFI / Module System Blueprint (Standard Library Decoupling)

**Project:** Helheim — Bare-Metal CodeTaal Compiler  
**Goal:** Completely decouple the core engine (orchestrator/executor) from built-in standard library functionality (math, text, JSON, etc.). Move everything to loadable C-ABI native modules (`.so`, `.dll`, `.dylib`).  
**Current State (pre-blueprint):** `Gebruik` is compile-time only for `.hel` files (see `resolver.rs` + `ModuleLinker`). Native stdlib bits are still hardcoded in `executor.rs` / `system.rs`. TCP and GPU primitives have already been moved to pure AST nodes.

This blueprint delivers **concrete Rust struct definitions**, C-ABI layout, loading code, and exact integration points with existing `Gebruik`, `MemoryManager`, and `Executor`.

## 1. Core Principles

- **C ABI only** for maximum ecosystem reach (C, Zig, Rust, even assembly modules possible).
- **Zero string parsing** over the boundary when possible — use pointers + lengths.
- **Explicit ownership** — no implicit Rust drops across FFI. Use provided allocators or clear "caller frees" rules.
- **Runtime loading** — `gebruik "math"` triggers discovery + load (hybrid with existing compile-time .hel linker).
- **Bare-metal speed** — function call overhead should be comparable to a normal indirect call + a couple of pointer copies.
- **Versioned ABI** — `abi_version` in every module header.
- **Same-process** for speed (no RPC). Isolation comes later via separate processes or seccomp if needed for untrusted modules.

## 2. FFI Interface — The C ABI (Concrete Definitions)

All of this lives in a new crate or module: `helheim-core/src/ffi.rs` (or a separate `helheim-ffi` crate later).

```rust
// helheim-core/src/ffi.rs
use std::os::raw::{c_char, c_int, c_void};

/// Stable ABI version. Bump on breaking changes to the HelValue layout or calling convention.
pub const HEL_ABI_VERSION: u32 = 1;

/// Opaque context passed to modules. Contains allocator callbacks and host services.
#[repr(C)]
pub struct HelFFIContext {
    pub abi_version: u32,
    /// Allocator provided by Helheim (module must use this for any returned complex data).
    pub alloc: Option<extern "C" fn(usize, usize) -> *mut c_void>, // size, align
    pub free: Option<extern "C" fn(*mut c_void)>,
    /// Optional host logging / error reporting.
    pub log: Option<extern "C" fn(*const c_char)>,
    /// User data (pointer back to Helheim runtime structures if needed).
    pub user_data: *mut c_void,
}

/// Tag for HelValue (must stay stable).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelValueTag {
    Null = 0,
    Int = 1,
    Float = 2,
    String = 3,
    List = 4,
    ResourceHandle = 5,
    // Future: Bytes, Dict, etc. Add at the end only.
}

/// C-compatible string view (Helheim owns or module owns depending on direction).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelString {
    pub ptr: *const u8,
    pub len: usize,
}

/// List of HelValues (shallow — elements may contain pointers).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelList {
    pub ptr: *const HelValue,
    pub len: usize,
}

/// Resource handle (maps to Helheim's ResourceHandle from TCP blueprint).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelResourceHandle {
    pub kind: *const c_char, // null-terminated, owned by module or static
    pub id: u64,
}

/// The main FFI value type. This is the only thing that crosses the boundary.
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
    pub _pad: [u8; 16], // Ensure stable size/alignment
}

impl HelValue {
    pub const NULL: HelValue = HelValue {
        tag: HelValueTag::Null,
        data: HelValueData { i: 0 },
    };

    pub fn from_int(v: i64) -> Self {
        HelValue { tag: HelValueTag::Int, data: HelValueData { i: v } }
    }

    pub fn from_float(v: f64) -> Self {
        HelValue { tag: HelValueTag::Float, data: HelValueData { f: v } }
    }

    // Note: For String/List/Resource the caller (Helheim or module) must ensure
    // the pointed-to data outlives the HelValue or uses the context allocator.
}

/// Function signature for native Helheim functions.
/// - `args`: array of input values (length = arity)
/// - `out`: where the module writes the return value
/// - Returns 0 on success, non-zero on error (error details via ctx->log or a future error channel).
pub type HelFunctionCall = extern "C" fn(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int;

/// Description of one exported function.
#[repr(C)]
pub struct HelFunctionDesc {
    pub name: *const c_char,
    pub arity: u32,
    pub call: HelFunctionCall,
}

/// Table returned by a module.
#[repr(C)]
pub struct HelFunctionTable {
    pub count: u32,
    pub entries: *const HelFunctionDesc,
}

/// The entry point every native module **must** export.
/// Called once when the library is loaded.
/// The module should register its functions and can store `ctx->user_data`.
#[no_mangle]
pub extern "C" fn helheim_module_init(ctx: *mut HelFFIContext) -> c_int;

/// Optional cleanup entry point (called when module is unloaded).
#[no_mangle]
pub extern "C" fn helheim_module_shutdown(ctx: *mut HelFFIContext) -> c_int;
```

**Usage rules (must be documented):**
- For `String` / `List` returned by the module: the module **must** allocate the buffers using `ctx->alloc` and Helheim will call `ctx->free`.
- Input `String`/`List` from Helheim are **borrowed** (const views). Module must not free them.
- ResourceHandle is just an ID — the module talks to the host via other means or via the `user_data` pointer if it needs deep access.
- All strings are UTF-8.

## 3. Rust-Side Loading Mechanism (libloading)

```rust
// helheim-core/src/ffi/loader.rs
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ffi::{HelFFIContext, HelFunctionCall, HelFunctionDesc, HelFunctionTable, HelValue};

pub struct LoadedNativeModule {
    /// Keep the Library alive for the lifetime of the function pointers.
    _library: Library,
    pub name: String,
    pub functions: HashMap<String, HelFunctionCall>,
    pub context: *mut HelFFIContext, // owned by us
}

pub struct NativeModuleLoader {
    search_paths: Vec<PathBuf>,
    loaded: HashMap<String, Arc<LoadedNativeModule>>,
}

impl NativeModuleLoader {
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths, loaded: HashMap::new() }
    }

    /// Finds and loads `lib{name}.so` / `.dll` / `.dylib`.
    /// Returns a handle that must be kept alive as long as any of its functions may be called.
    pub fn load(&mut self, module_name: &str, host_context: *mut HelFFIContext) -> anyhow::Result<Arc<LoadedNativeModule>> {
        if let Some(existing) = self.loaded.get(module_name) {
            return Ok(existing.clone());
        }

        let lib_name = format!("lib{}", module_name);
        let candidates = self.build_candidates(&lib_name);

        let mut last_err = None;
        for candidate in candidates {
            match unsafe { Library::new(&candidate) } {
                Ok(lib) => {
                    // Get the mandatory init symbol
                    let init: Symbol<extern "C" fn(*mut HelFFIContext) -> i32> = unsafe {
                        lib.get(b"helheim_module_init\0")?
                    };

                    let ret = unsafe { init(host_context) };
                    if ret != 0 {
                        anyhow::bail!("Module {} init failed with code {}", module_name, ret);
                    }

                    // Get the function table (recommended way)
                    let mut functions = HashMap::new();
                    if let Ok(get_table) = unsafe {
                        lib.get::<extern "C" fn() -> *const HelFunctionTable>(b"helheim_get_function_table\0")
                    } {
                        let table = unsafe { &*get_table() };
                        for i in 0..table.count as usize {
                            let desc = unsafe { &*table.entries.add(i) };
                            if !desc.name.is_null() {
                                let name = unsafe { std::ffi::CStr::from_ptr(desc.name) }
                                    .to_string_lossy()
                                    .into_owned();
                                functions.insert(name, desc.call);
                            }
                        }
                    }

                    let loaded = Arc::new(LoadedNativeModule {
                        _library: lib,
                        name: module_name.to_string(),
                        functions,
                        context: host_context,
                    });

                    self.loaded.insert(module_name.to_string(), loaded.clone());
                    return Ok(loaded);
                }
                Err(e) => last_err = Some(e),
            }
        }

        anyhow::bail!("Failed to load module '{}': {:?}", module_name, last_err)
    }

    fn build_candidates(&self, lib_name: &str) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for base in &self.search_paths {
            // libmath.so, libmath.dylib, libmath.dll
            for ext in &["so", "dylib", "dll"] {
                out.push(base.join(format!("{}.{}", lib_name, ext)));
            }
            // Also try without lib prefix on some platforms
            for ext in &["so", "dylib", "dll"] {
                out.push(base.join(format!("{}.{}", lib_name.trim_start_matches("lib"), ext)));
            }
        }
        out
    }
}
```

## 4. Integration with `gebruik` + MemoryManager + SymbolTable

Extend the existing `ModuleLinker` (or create a parallel runtime loader).

In `helheim-core/src/orchestra/memory.rs` (or a new `native.rs`):

```rust
pub struct NativeFunction {
    pub module: Arc<LoadedNativeModule>,
    pub call: HelFunctionCall,
}

pub struct MemoryManager {
    // ... existing fields ...
    pub native_funcs: Arc<DashMap<String, NativeFunction>>,  // "math::sin" -> thunk
}
```

**In the Orchestrator / Executor (runtime handling of `Gebruik` for natives):**

```rust
// In executor.rs, inside execute_ast or a dedicated import handler
CodeTaal::Gebruik { path } => {
    if path.ends_with(".hel") || /* .hel resolution succeeds */ {
        // existing compile-time behavior (already expanded)
    } else {
        // Native module
        let loader = &mut self.native_loader; // stored on Executor or Orchestrator
        let module = loader.load(&path, self.ffi_context)?;  // creates HelFFIContext

        for (name, call) in &module.functions {
            let full_name = format!("{}::{}", path, name); // or just name
            self.memory.native_funcs.insert(
                full_name.clone(),
                NativeFunction { module: module.clone(), call: *call },
            );
            // Also register in the old func_store for compatibility if needed
            self.memory.func_store.insert(full_name, "native".to_string());
        }
        println!("[FFI]: Loaded native module '{}' ({} functions)", path, module.functions.len());
    }
}
```

**Function call dispatch (in `FunctionCall` arm):**

```rust
CodeTaal::FunctionCall { name, args } => {
    if let Some(native) = self.memory.native_funcs.get(&name) {
        // Marshal HelheimType -> HelValue (see helper below)
        let ffi_args: Vec<HelValue> = args.iter()
            .map(|a| self.marshal_to_ffi(a, ctx.clone()))
            .collect::<Result<Vec<_>>>()?;

        let mut out = HelValue::NULL;
        let rc = (native.call)(
            native.module.context,
            ffi_args.as_ptr(),
            ffi_args.len() as u32,
            &mut out,
        );

        if rc != 0 {
            return Err(anyhow::anyhow!("Native function {} failed", name));
        }

        let result = self.unmarshal_from_ffi(out)?;
        // store in memory or return
        ...
        return Ok(...);
    }

    // fall back to AST functions or old hardcoded
}
```

**Marshal / Unmarshal helpers** (in a `ffi/bridge.rs`):

- `String` → `HelString { ptr: s.as_ptr(), len: s.len() }` (borrowed)
- `List` → build `HelList`
- `ResourceHandle` → `HelResourceHandle`
- Reverse: for returned `String`/`List`, the data **must** have been allocated via the context allocator. Helheim takes ownership and will free later.

This gives pointer-passing speed for hot paths.

## 5. Search Path & Discovery (same spirit as current resolver)

Extend `ModuleLinker` or create `NativeModuleRegistry` with:

- `HELHEIM_LIB_PATH` env var (colon/semicolon separated)
- `~/.helheim/lib/`
- `./lib/`
- Relative to current script
- Built-in mapping: `"math"` → `libmath.so`, `"json"` → `libjson.so`, etc.

For `gebruik "wiskunde"` the loader first tries native, falls back to `.hel` (or vice versa).

## 6. Concrete Usage Example (from .hel side)

```hel
gebruik "math"          # loads libmath.so (or math.hel)
gebruik "json"          # loads libjson.so

zet pi = roep_aan math::pi
zet result = roep_aan math::sin 1.57
zet data = roep_aan json::parse "{ \"a\": 42 }"
```

The `Gebruik` no longer needs to be fully expanded at compile time for native modules — it becomes a runtime import that populates the symbol table.

## 7. ABI Safety & Versioning Notes

- Always check `ctx->abi_version` in `helheim_module_init`.
- Provide a small C header (`helheim_ffi.h`) that people can use when writing modules in C/Zig.
- For Rust modules, we can later offer a `helheim-ffi` proc-macro or derive to generate the table safely.
- Keep the `Library` alive in `LoadedNativeModule` — this is critical.

---

This is the **concrete foundation** you asked for. No fluff.

The structs above (`HelValue`, `HelFFIContext`, `HelFunctionDesc`, etc.) are the actual C ABI that external modules will implement against.

The `NativeModuleLoader` + registration into `MemoryManager.native_funcs` + dispatch in the executor is the minimal runtime glue.

Next steps (if you want me to implement):
- Create the `ffi/` module with the structs + loader.
- Wire a basic `helheim_module_init` example (math module).
- Extend `Gebruik` handling + add the marshal helpers.
- Update `MemoryManager` with the native func table.

Tell me when to start coding the Rust side, or if you want changes to the ABI layout first. This pairs perfectly with the TCP primitives you just landed — now stdlib can live outside the core while still being first-class in the language.