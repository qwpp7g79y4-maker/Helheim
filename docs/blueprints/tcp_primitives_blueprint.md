# Primitives-First Asynchronous TCP Networking Blueprint

**Project:** Helheim — Bare-Metal CodeTaal JIT Compiler (SNN + PTX path)  
**Status:** Design (pre-implementation)  
**Author:** Grok (based on architect requirements)  
**Date:** 2026  
**Goal:** Remove high-level bloat (`HttpOp`, stringly-typed `TcpOp`) and provide ruthless C-style TCP primitives directly in the AST. Users build HTTP, WebSocket, DB drivers, custom protocols etc. themselves in pure `.hel`.

## Core Principles

- **Primitives only.** No built-in HTTP, no "easy server". Just `listen` / `connect` / `send` / `recv` on raw bytes/streams.
- **Raw bytes first.** Strings are a convenience on top. The wire deals in `bytes`.
- **Non-blocking by construction.** All network ops integrate with the existing async executor.
- **Resource handles, not objects.** You cannot put a `TcpStream` in a `HelheimType` that must be serializable/persistable/distributable. Use opaque handles + side table.
- **Zero bloat in the language core.** Everything above the primitives lives in user `.hel` libraries.
- **VRAM bridge as a first-class concern.** Incoming network data for SNN workloads must be able to land in device memory with minimal host copies.
- **Privileged by default.** Network is a privileged operation (like `FileOp` today).

## 1. AST Nodes (CodeTaal)

Add these four (plus `TcpAccept` for complete listener semantics) to `helheim-lang/src/ast.rs`:

```rust
// In the CodeTaal enum (after the existing HOST OPERATIONS section)

/// tcp_luister "0.0.0.0:8080"          → resource handle (listener)
TcpListen {
    addr: Box<CodeTaal>,
},

/// tcp_accepteer listener_handle      → resource handle (new stream)
TcpAccept {
    listener: Box<CodeTaal>,
},

/// tcp_verbind "93.184.216.34:80"     → resource handle (stream)
TcpConnect {
    addr: Box<CodeTaal>,
},

/// tcp_stuur socket_handle, data
TcpSend {
    socket: Box<CodeTaal>,   // must resolve to ResourceHandle { kind: "tcp_stream", ... }
    data: Box<CodeTaal>,     // Literal(Bytes(..)), List of int, or String (treated as utf8 bytes)
},

/// tcp_ontvang socket_handle [max_len]
TcpReceive {
    socket: Box<CodeTaal>,
    max_bytes: Option<Box<CodeTaal>>,
},
```

**Notes on design:**
- All address expressions are `Box<CodeTaal>` so you can do `tcp_verbind ( "192.168.1.1:" + poort )`.
- `data` on send and result of receive are **byte-oriented**.
- We will extend `LiteralValue` (and `HelheimType`) with `Bytes(Vec<u8>)`.
- `TcpAccept` is included because a bare listener without an explicit accept primitive forces bad abstractions higher up. It is still a primitive.

Corresponding parser rules (Dutch keywords for CodeTaal consistency):
- `tcp_luister`, `tcp_accepteer`, `tcp_verbind`, `tcp_stuur`, `tcp_ontvang`

## 2. Asynchronous Tokio Integration (Orchestrator / Executor)

The `Executor::execute_ast` is already `Pin<Box<dyn Future<Output = Result<...>> + Send>>` and runs inside an async context (tokio runtime via the CLI).

### Strategy

1. **All new TCP nodes are `.await`-able inside the existing loop.**
   - `TcpListen` → `tokio::net::TcpListener::bind(addr).await`
   - `TcpAccept` / `TcpConnect` → the corresponding `.await` calls.
   - `TcpSend` / `TcpReceive` use `AsyncWriteExt` / `AsyncReadExt`.

2. **Long-running listeners** are the user's responsibility via the existing `Daemon` / `Concurrent` constructs or a future `spawn` primitive. The runtime does **not** auto-spawn accept loops.

3. **No blocking the executor thread.**
   - Use only the async `tokio::net` types.
   - Current legacy `CodeTaal::TcpOp` (which uses `std::net`) must be removed or deprecated.

4. **MPSC / Event loop considerations (for advanced use)**
   - For very high connection counts, a single listener task can send accepted streams over an `tokio::sync::mpsc` channel into a shared `ResourceTable`.
   - The main executor only ever does short `.await` points for individual send/recv.
   - This keeps the "one statement = one island" distributed model intact.

Example skeleton inside `executor.rs` (match arm):

```rust
CodeTaal::TcpListen { addr } => {
    if !ctx.is_privileged { return Err(...); }
    let addr_str = self.code_taal_to_string_sync(&addr);
    let listener = tokio::net::TcpListener::bind(&addr_str).await?;
    let handle_id = self.resources.insert_tcp_listener(listener);
    let h = HelheimType::ResourceHandle { kind: "tcp_listener".into(), id: handle_id };
    self.memory.set_var_native(... , h);   // or return it if used as expression
}
```

The `resources` field will live on `Executor` (or be reachable via the `Orchestrator`).

## 3. Resource Handle System & Socket Persistence

### The Problem
`HelheimType` must remain:
- Clone + Debug + PartialEq + (de)serializable (for `ast_json:`, persistence, distributed execution).
- Small.

A `tokio::net::TcpStream` is `Send + Sync` (when wrapped properly) but **not** something you want to serialize or put in the DashMap directly.

### Solution: Opaque Resource Handles + Side Table

Extend in `helheim-lang/src/memory.rs`:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum HelheimType {
    // ... existing variants ...
    Bytes(Vec<u8>),
    ResourceHandle { kind: String, id: u64 },
    // ...
}
```

In `helheim-core` (new file recommended: `src/orchestra/resources.rs`):

```rust
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex; // or parking_lot if we want sync mutex
use tokio::net::{TcpListener, TcpStream};

pub enum Resource {
    TcpListener(Arc<TcpListener>),
    // Full stream behind mutex for simple full-duplex access.
    // Alternative (more efficient): store split halves.
    TcpStream(Arc<TokioMutex<TcpStream>>),
}

pub struct ResourceTable {
    next_id: AtomicU64,
    table: DashMap<u64, Resource>,
}

impl ResourceTable {
    pub fn new() -> Self { ... }

    pub fn insert_tcp_listener(&self, l: TcpListener) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.table.insert(id, Resource::TcpListener(Arc::new(l)));
        id
    }

    pub fn insert_tcp_stream(&self, s: TcpStream) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.table.insert(id, Resource::TcpStream(Arc::new(TokioMutex::new(s))));
        id
    }

    pub fn get_tcp_listener(&self, id: u64) -> Option<Arc<TcpListener>> { ... }
    pub fn get_tcp_stream(&self, id: u64) -> Option<Arc<TokioMutex<TcpStream>>> { ... }

    /// Remove on explicit close or drop of handle (optional RAII).
    pub fn close(&self, id: u64) -> bool { self.table.remove(&id).is_some() }
}
```

**Usage in variable:**
```hel
mijn_listener = tcp_luister "0.0.0.0:9001"
mijn_conn     = tcp_verbind "1.1.1.1:443"
```

The variable stores `ResourceHandle { kind: "tcp_stream", id: 42 }`.

The actual `Arc<...>` lives only in the `ResourceTable` owned by the `Executor` (or `Orchestrator`).

**Lifetime rules:**
- Handles are valid only for the lifetime of the current process / Executor.
- On `persist` / distributed move, handles become invalid (user code must re-connect).
- Provide a `tcp_sluit handle` primitive (or reuse a general `close`).

## 4. Memory Management & The VRAM Bridge

### Direct path goal
Network → (minimal host staging) → VRAM for SNN spike processing or tensor input, with as few copies as possible.

### Proposed mechanism

1. **Host-side receive** always lands in a `Vec<u8>` (or pinned buffer) first. This is unavoidable for most protocols.

2. **Pinned staging when targeting GPU.**
   - Extend `TcpReceive` (or add a sibling node) with an optional destination hint:
     ```rust
     TcpReceive {
         socket: Box<CodeTaal>,
         max_bytes: Option<Box<CodeTaal>>,
         target: Option<Box<CodeTaal>>,   // if this is a tensor var name, go direct
     }
     ```
   - In the executor, when `target` resolves to a `HelheimType::Tensor` (or a registered GPU buffer), the runtime:
     - Allocates (or reuses) a CUDA pinned host buffer (`cudarc` / `cudaHostAlloc`).
     - Performs the async TCP read into the pinned buffer.
     - Issues `cudaMemcpyAsync` (host → device) directly from the pinned buffer.
     - The tensor variable is updated to point at the device memory (or we have a separate GPU memory manager).

3. **Alternative (more general):** A separate primitive `gpu_upload_host_bytes buffer_handle, tensor` after a normal `tcp_ontvang`. This keeps the TCP nodes pure while still giving the zero-copy-ish path.

4. **For SNN spike packets** specifically: the user can receive raw bytes and immediately feed them into a `GpuKernel` or a dedicated `spike_unpack` op. The receive primitive itself does not know about spikes.

**Implementation note:** The GPU backend already has `cudarc`. The resource table can expose a method `receive_into_pinned(...)` that the executor calls when it detects a GPU destination.

## 5. Security, Privileges & Cleanup

- All TCP nodes require `ctx.is_privileged` (same as current `FileOp` / old `TcpOp`).
- Sandbox mode can still allow them if the architect explicitly relaxes the check (for trusted internal services).
- On scope exit or explicit `tcp_sluit`, the handle should be removed from the table (best-effort).
- Add a `ResourceGuard` similar to the existing `ScopeGuard` if we want automatic close on function return (optional, can be user-controlled for servers that want long-lived sockets).

## 6. Parser / Semantic / Distributed Considerations

- Parser must produce the new strongly-typed nodes instead of falling back to `TcpOp`.
- Semantic analyzer should validate that `socket` expressions are used in the right context (future: simple type tags on handles).
- **Distributed / `ast_json`:** Resource handles are **node-local**. When a `Concurrent` or remote dispatch happens, any statement containing a live socket handle must either:
  - Fail at dispatch time, or
  - Be executed locally only (the scheduler already has some GPU vs CPU affinity logic — extend it with "has_socket_handle").
- Persistence layer (`helheim-lang/src/persistence.rs`) should serialize `ResourceHandle` as a special marker that becomes `Null` or an error on reload.

## 7. Minimal Example Usage (what the primitives enable)

```hel
# A user-written "http_get" in pure .hel (illustrative, not part of core)

functie http_get met host pad {
    zet sock = tcp_verbind host
    zet req = "GET " + pad + " HTTP/1.0\r\nHost: " + host + "\r\n\r\n"
    tcp_stuur sock, req

    zet response = tcp_ontvang sock 8192
    tcp_sluit sock
    geef_terug response
}

zet body = roep_aan http_get "example.com:80" "/"
druk_af body
```

A full web server would be an accept loop inside a `daemon` or `zolang` with `tcp_accepteer`.

## 8. Implementation Roadmap (recommended order)

1. Extend `LiteralValue` + `HelheimType` with `Bytes` and `ResourceHandle`.
2. Create `helheim-core/src/orchestra/resources.rs` (ResourceTable + Resource enum).
3. Add the five new `CodeTaal` variants.
4. Implement parser productions (keep old `TcpOp` temporarily for transition).
5. Wire the match arms in `executor.rs` using the async tokio APIs + resource table.
6. Update `MemoryManager` resolve / set paths if needed for the new types.
7. Add `tcp_sluit` primitive + basic error handling (return error strings or use existing TryCatch).
8. Deprecate / remove the old string-based `TcpOp` and `HttpOp` (HTTP can be a pure .hel library on top of these primitives).
9. Add tests: basic echo server/client in `.hel`, large transfer, multiple concurrent connections.
10. VRAM bridge (pinned + cudaMemcpyAsync path) as a follow-up once basic primitives are solid.

## 9. Open Questions / Trade-offs

- **Split halves vs Mutex?** Splitting gives true concurrent read/write but complicates the handle model (two handles per connection). Mutex is simpler for v1.
- **Explicit buffer management?** C programmers love `recv(fd, buf, len)`. We can later add a `tcp_ontvang_in buffer_var, socket, len` form for zero-allocation hot paths.
- **TLS?** Out of scope for the primitive layer. User builds or links a TLS lib on top (or we later add a `tls_upgrade handle` primitive).
- **UDP?** Same pattern (`udp_verbind`, `udp_stuur`, `udp_ontvang`) can be added later using the same resource table.
- **Zero-copy on the send side?** If user has a tensor they want to transmit, a future `tcp_stuur_van_gpu` can do device-to-host pinned + write.

---

This blueprint gives the **compiler layer** the architect asked for: small, mean, C-style, async-native primitives that make everything else (HTTP, custom protocols, SNN-over-TCP, etc.) possible in the language itself without ever touching Rust again for the wire format.

Ready for implementation when you give the signal. The core is already clean enough to host this without the old bloat fighting us.