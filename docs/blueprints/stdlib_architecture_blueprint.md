# Helheim Standard Library Architecture Blueprint

**Version:** 1.0  
**Date:** 2026  
**Status:** Design Specification  
**Author:** Helheim Core Team  
**Philosophy:** Primitives over Built-ins. Zero-overhead. Bare-metal. Everything above the raw TCP/FFI/AST primitives must be expressible in pure CodeTaal or thin C-ABI shims.

## Executive Summary

The Helheim StdLib must be **completely external** to the core compiler and executor. No more hardcoded `system.rs` functions, no baked-in HTTP clients, no SQLite in the Rust binary.

The StdLib consists of two layers:
- **Pure Helheim Layer** (`.hel` scripts): Implemented using only the raw primitives (`tcp_verbind`, `tcp_stuur`, `tcp_ontvang`, `tcp_accepteer`, `tcp_luister`, FFI calls, basic AST ops). Zero Rust bloat.
- **FFI Bridge Layer** (`.so`/`.dll` via the C-ABI `NativeModuleLoader`): For unavoidable native dependencies (SQLite, crypto primitives, OS syscalls beyond TCP).

The core (`helheim-core`) only provides **discovery, loading, caching, and secure namespace injection**. All semantics live outside.

This design ensures:
- The JIT/PTX path remains untouched and zero-overhead.
- Full Rust module isolation (native modules are separate shared objects).
- Zero-copy data paths where possible (direct `HelValue` / pointer passing across FFI and between pure .hel modules via the existing `HelheimType`).

## 1. Directory Structure & Automatic Discovery / Loading

### Recommended File Hierarchy

The StdLib lives **outside** the compiler binary for easy updates and user extension. Default search order (first match wins):

```
$HOME/.helheim/                  # User override / installed stdlib
└── stdlib/
    ├── core.hel                 # Fundamental types, control flow helpers, error handling (pure)
    ├── prelude.hel              # Auto-imported symbols (like Rust prelude)
    ├── http.hel                 # Pure Helheim HTTP/1.1 (Helheim-First)
    ├── fs.hel                   # Filesystem wrappers (mix of pure + thin FFI for dir ops)
    ├── math.hel                 # Or thin FFI to libhelheim_math.so (see FFI example)
    ├── json.hel                 # Pure parser/serializer using byte primitives
    ├── sqlite.hel               # FFI wrapper (see section 3)
    ├── net.hel                  # Higher-level TCP/UDP helpers (pure, on top of tcp_* primitives)
    └── lib/                     # Native FFI modules (C-ABI)
        ├── libhelheim_math.so
        ├── libhelheim_sqlite.so
        ├── libhelheim_crypto.so
        └── ...

/opt/helheim/stdlib/             # System-wide installation (lower priority)
└── ...

./stdlib/                        # Project-local (highest priority for development)
└── ...
```

Environment overrides:
- `HELHEIM_STDLIB_PATH` (colon/semicolon separated, highest priority)
- `HELHEIM_LIB_PATH` for native `.so` search (in addition to `stdlib/lib/`)

### Boot-Time Discovery & Caching in `helheim-core`

A new dedicated component is introduced:

```rust
// helheim-core/src/stdlib/mod.rs
pub struct StdLibManager {
    pure_modules: DashMap<String, Arc<ExpandedStdModule>>,  // pre-linked AST + symbol table
    native_loader: Arc<Mutex<NativeModuleLoader>>,          // reuses existing FFI loader
    cache: StdLibCache,                                     // on-disk or in-memory LRU for expanded AST
}

pub struct ExpandedStdModule {
    pub namespace: String,           // "http", "fs", etc.
    pub ast: Vec<CodeTaal>,          // fully linked, namespaced AST (no more raw Gebruik)
    pub symbols: SymbolTable,        // for SemanticAnalyzer
    pub source_path: PathBuf,
}
```

**Initialization Sequence** (in `Orchestrator::new` or a new `StdLib::bootstrap()` called early):

1. **Resolve search paths** (env + defaults + entry dir). Prioritize project-local > user > system.
2. **Scan for modules**:
   - Enumerate `*.hel` and `libhelheim_*.so` (or `lib*.so` with manifest).
   - Use a small manifest file per module (optional `module.helmanifest` JSON) for version, dependencies, and whether it is "pure" or "ffi".
3. **Parallel load** (using `rayon` or `tokio::spawn` for I/O):
   - For each `.hel`:
     - Use existing `ModuleLinker::with_std_lib(...)` to expand `Gebruik` (but see namespace evolution below).
     - Parse once, cache the `ExpandedStdModule`.
     - Compute a content hash (blake3 of source + linked AST) for cache invalidation.
   - For each `.so`:
     - Delegate to `NativeModuleLoader::load("http", ctx)` (existing mechanism).
     - Register the returned `HelFunctionTable` under the namespace.
4. **Cache layer**:
   - In-memory `DashMap` for hot modules.
   - Optional on-disk cache under `~/.helheim/cache/stdlib/` (serialized `ExpandedStdModule` + source hash).
   - Cold start: only parse on hash mismatch or missing cache.
5. **Secure isolation**:
   - StdLib modules are loaded in a **restricted `ExecutionContext`** (no privileged native execution unless explicitly marked).
   - Native `.so` modules are loaded with the existing FFI sandboxing (see previous FFI blueprint).

The `StdLibManager` is stored on the `Orchestrator` and passed down to `Executor` and `SemanticAnalyzer`.

**Zero-copy / Performance invariants**:
- .hel sources are mmap'ed where possible (via `memmap2`).
- Expanded AST is `Arc`'d and never cloned during execution.
- Native function pointers are direct (no vtable indirection beyond the existing `NativeModuleLoader` `HashMap`).

## 2. The "Helheim-First" Principle

No high-level operation may be implemented in Rust if it can be expressed using the raw primitives + pure CodeTaal.

### Example: `http.hel` (Pure Implementation)

```hel
# http.hel
# Implements a minimal HTTP/1.1 client using only tcp_* primitives.
# Zero Rust HTTP stack. Zero allocation beyond what the caller provides.

functie http.get met url {
    # Parse URL (minimal, pure Helheim string ops assumed in core.hel)
    zet host = roep_aan url.host url
    zet poort = roep_aan url.poort url 80
    zet pad = roep_aan url.pad url "/"

    # Raw TCP connection (primitive)
    zet sock = tcp_verbind (host + ":" + (roep_aan tekst.van poort))

    # Build request (pure string construction)
    zet req = "GET " + pad + " HTTP/1.1\r\n"
            + "Host: " + host + "\r\n"
            + "User-Agent: Helheim/1.0\r\n"
            + "Connection: close\r\n"
            + "\r\n"

    # Zero-copy send of the request bytes
    # (tcp_stuur accepts Bytes or string; string is treated as UTF-8 bytes)
    tcp_stuur sock req

    # Receive full response (blocking until close or content-length for simplicity)
    zet resp_bytes = tcp_ontvang sock 1048576   # 1MB cap for example

    # Minimal response parsing in pure Helheim (headers + body split)
    # This can be further optimized with byte views / zero-copy slices later
    zet headers_end = roep_aan tekst.index_of resp_bytes "\r\n\r\n"
    zet headers_str = roep_aan tekst.substring resp_bytes 0 headers_end
    zet body = roep_aan tekst.substring resp_bytes (headers_end + 4) (roep_aan tekst.lengte resp_bytes)

    # Return structured result using core types
    zet result = nieuw HttpResponse {
        status = roep_aan http.parse_status headers_str,
        headers = roep_aan http.parse_headers headers_str,
        body = body
    }

    tcp_sluit sock   # explicit resource management (primitive)
    geef_terug result
}

# Supporting pure helpers (can live in http.hel or be pulled from core.hel)
functie http.parse_status met headers { ... }
functie http.parse_headers met headers { ... }
```

This is **the** Helheim-First contract:
- `http.get` is just another user-level function once `tcp_verbind` / `tcp_stuur` / `tcp_ontvang` / `tcp_sluit` exist as AST primitives.
- The executor never sees "HTTP". It only sees raw TCP primitives + string/bytes operations.
- Any performance work (better header parsing, keep-alive, HTTP/2) happens in the `.hel` layer or via a better FFI plugin — never in the core.

## 3. The FFI Bridge: `sqlite.hel` + Native Plugin

For functionality that cannot be expressed efficiently in pure CodeTaal (complex C libraries, hardware access beyond TCP), we use a thin namespaced `.hel` wrapper around the C-ABI FFI.

### Architectural Plan

**sqlite.hel** (pure wrapper, loaded via the .hel path):

```hel
# sqlite.hel
# Thin, namespaced bridge. No logic — only FFI declarations + ergonomic wrappers.

# The actual symbols come from the native module registered under "sqlite"
# during StdLib bootstrap (libhelheim_sqlite.so exports sqlite_open, sqlite_exec, etc.
# via the HelFunctionTable).

functie sqlite.open met path {
    # Direct FFI call through the registered native function
    # The runtime resolves "sqlite::open" to the C-ABI entry point.
    zet handle = roep_aan sqlite.open path
    geef_terug handle   # ResourceHandle (see TCP primitives)
}

functie sqlite.exec met db statement {
    # Zero-copy: statement can be a Bytes or string view
    zet result = roep_aan sqlite.exec db statement
    geef_terug result   # List of rows or error Resource
}

# Higher-level ergonomic API built on top (still pure .hel)
functie sqlite.query met db sql {
    zet rows = roep_aan sqlite.exec db sql
    # ... post-processing in Helheim if desired ...
    geef_terug rows
}
```

**Native side** (`libhelheim_sqlite.so`):

- Implements the exact C-ABI from the FFI blueprint (`helheim_module_init`, `helheim_get_function_table`).
- Exports:
  - `sqlite::open(path: String) -> ResourceHandle`
  - `sqlite::exec(handle: ResourceHandle, sql: String) -> List`
  - etc.
- Uses the `HelFFIContext` allocator for any returned `List` / `String`.
- Internally uses the real SQLite C API (or rusqlite, etc.) but the boundary is strictly the `HelValue` union.

**Registration during bootstrap**:
- The `StdLibManager` scans `stdlib/lib/libhelheim_sqlite.so`.
- Calls `NativeModuleLoader::load("sqlite", ctx)`.
- All exported functions are registered in the global symbol table under the `sqlite::` prefix.
- The `sqlite.hel` file itself is still processed by `ModuleLinker` so that users can `gebruik "sqlite"` and get the nice wrapper functions (or the runtime can auto-inject).

This gives **seamless** usage from .hel code while keeping the heavy lifting (and all SQLite symbols) completely isolated in the `.so`.

## 4. Bootstrapping the Environment

### Namespace Injection Points

#### A. SemanticAnalyzer (Compile-time / Link-time)

Extend `SemanticAnalyzer` (or introduce `StdLibSymbolProvider`):

```rust
// During analysis of any script
pub fn analyze_with_stdlib(&mut self, ast: &mut [CodeTaal], stdlib: &StdLibManager) -> Result<()> {
    for (ns, module) in &stdlib.pure_modules {
        for symbol in &module.symbols {
            self.symbol_table.register_qualified(ns, symbol.name, symbol.ty);
        }
    }
    // Native modules contribute their function signatures (inferred or declared)
    for (ns, funcs) in &stdlib.native_functions {
        for (name, sig) in funcs {
            self.symbol_table.register_ffi(ns, name, sig);
        }
    }
    // ...
}
```

Qualified names (`http::get`, `sqlite::open`) are first-class in the symbol table. This enables proper name resolution and arity checking without polluting the global namespace.

#### B. Executor / MemoryManager (Runtime)

In `Executor::new` (or on first use):

```rust
// Pre-populate global scope
for (ns, module) in &stdlib_manager.pure_modules {
    for (name, ast_fn) in &module.functions {
        memory.register_ast_function(format!("{}::{}", ns, name), ast_fn.clone());
    }
}

for (ns, native_mod) in &stdlib_manager.native_modules {
    for (name, call) in &native_mod.functions {
        memory.register_native_function(format!("{}::{}", ns, name), call);
    }
}
```

**Fast dispatch** (critical for zero JIT overhead):
- `FunctionCall` in the executor first does a O(1) `DashMap` lookup in `memory.std_functions` / `memory.native_functions`.
- Native calls go straight to the `HelFunctionCall` pointer (the existing FFI path).
- Pure .hel std functions are either inlined at link time (for hot std) or executed via the normal AST interpreter path (still very fast because they are tiny primitives).
- No dynamic string parsing at call time.

**Security**:
- StdLib functions run under the caller's `ExecutionContext` (sandbox vs privileged).
- Native modules can only access what their `HelFFIContext` grants them (allocator + error reporter + optional user_data for advanced resources).
- `ResourceHandle`s (TCP sockets, DB connections, file descriptors) are the only way to cross boundaries — they are opaque u64 + kind strings.

**Lazy vs Eager**:
- Core std (math, list, tekst) can be eagerly loaded at Orchestrator construction.
- Larger modules (http, sqlite, fs) can be lazy-loaded on first `Gebruik` or first qualified call. The `NativeModuleLoader` already supports this.

**Zero-copy data flow**:
- Between pure .hel std modules: direct `HelheimType` references (existing scope stack).
- To/from FFI: the marshal/unmarshal helpers (see previous task) that only copy when crossing the C boundary and always respect the context allocator.

## 5. Current State (Nulmeting) & Refactoring Path

**As of June 2026:**
The namespace system is currently implemented using a "pure inlining with prefix" hack in the executor.
- `CodeTaal::Gebruik` simply formats `"{ns}::{name}"` as a flat string and registers it in the global `memory.ast_funcs`.
- This bypasses proper semantic analysis, arity checks, and effect handler resolution (e.g. `perform Net.luister` is brittle).
- The recent `webserver_demo.hel` works using this hack, but it is not scalable or aligned with the PEPAI/NEXUS architecture goals.

**Action Plan (The Great Namespace Refactor):**
1. **SemanticAnalyzer**: Implement `register_qualified(ns, name, ty)` in the symbol table so namespaces are first-class constructs verified at compile-time.
2. **ModuleLinker**: Stop flattening module imports into a single string prefix. Preserve module boundaries.
3. **MemoryManager & Executor**: 
   - Pre-populate global scope cleanly using `memory.register_ast_function(ns, name, ...)` with a nested or robust DashMap structure capable of O(1) qualified lookups.
   - Update `FunctionCall` and `Perform` effect dispatch to resolve `ns::name` reliably without runtime string parsing hacks.

## Open Questions & Future Work (for the team)

- Provide a `helheim_ffi.h` + Rust `helheim-ffi` crate so third-party module authors have an easy time.
- Hot-reload of stdlib modules (development mode) without restarting the whole node.
- Capability-based security on `ResourceHandle` (e.g., a DB handle cannot be used for arbitrary TCP).

This blueprint keeps the **core engine** (AST, executor, PTX lowering, FFI loader, TCP primitives) as a minimal, auditable, zero-overhead substrate. All higher-level power lives in auditable, replaceable `.hel` and `.so` artifacts.

The "Primitives over Built-ins" contract is now enforceable at the architectural level.