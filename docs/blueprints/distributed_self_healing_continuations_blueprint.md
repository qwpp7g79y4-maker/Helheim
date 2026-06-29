# Distributed Self-Healing via Continuations Blueprint

**Project:** Helheim CodeTaal — Bare-Metal Swarm Language  
**Phase:** 6 (Killer Feature)  
**Version:** 1.0  
**Date:** 2026  
**Status:** Architecture + Implementation Roadmap  
**Author:** Helheim Core Team  
**References:**
- HELHEIM_VERBETERINGEN.md (Phase 5 completed + Phase 6 definition)
- actor.rs (SerializableContinuation, capture_continuation, resume_from_serialized)
- executor.rs (effect handler capture points, resume paths)
- effects.rs (CORE_EFFECTS including Swarm.dispatch)
- distributed.rs (DistributedMemory + Lamport deltas)
- distributed_teleport.hel (current P2P teleport demo)
- shield/crypto.rs (SwarmSigner for continuation signatures)
- tcp_resources.rs + orchestra (resource handles are node-local)
- stdlib_architecture_blueprint.md and first_class_namespaces... (namespaces now first-class)

## Executive Summary

The goal is **true distributed self-healing**:

A running Helheim computation (or effect, e.g. a long TCP stream, heavy Actor work, or inline computation) can be **paused at any statement boundary**, captured as a cryptographically signed `SerializableContinuation`, shipped over the encrypted Swarm (HSP + ECDH), and **resumed on any other healthy node** exactly where it left off — including the remaining AST stack, full memory snapshot, effect context, and a resume value.

This turns Helheim into a research-grade language for:
- Fault-tolerant edge / swarm computing
- Live process migration
- Self-healing services (node dies → continuation teleports and continues)
- Supervisor-like patterns on top of the existing Actor model

The foundation is already in place (Phase 5 + existing continuations work). This blueprint completes it into a coherent, production-viable self-healing system.

## Core Principles (Helheim Philosophy)

- **Continuations are first-class and delimited.** Not full call/cc, but safe, serializable "the rest of this effect handler + remaining statements".
- **Everything travels as AST + Snapshot.** Memory is pure (HelheimType + snapshots), the program counter is the remaining `Vec<CodeTaal>`.
- **Cryptographic trust boundary.** Every continuation that crosses nodes **must** be signed (existing SwarmSigner + base64 `resume_k`). Unsigned or tampered continuations are rejected.
- **Resource handles are node-local by design.** TCP sockets, FFI handles, GPU buffers etc. do **not** serialize. On resume the receiving node either:
  - Re-establishes equivalent resources (user code in the handler), or
  - The continuation is written so that resources are re-acquired after resume.
- **Zero-overhead when not migrating.** Capture only happens explicitly on `perform` effect boundaries or explicit `teleport` points.
- **Self-healing is policy-driven, not magic.** The language provides the primitive (`perform Swarm.migrate` or automatic on detected failure via flight recorder + supervisor actors). Higher-level healing logic lives in `.hel` (or thin FFI supervisors).
- **CPU-first + old-laptop compatible.** The entire mechanism must work without GPU. PTX/GPU is only an accelerator.

## Current Foundation (What Already Exists)

- `SerializableContinuation` (actor.rs):
  - `id`, `captured_memory: MemorySnapshot`, `captured_stack_json: String` (remaining AST), `effect`, `signature`.
- `capture_continuation(stmt, memory, effect)` — walks current stmt + `__REST_AST_*` key for the tail.
- Signing with `SwarmSigner` + base64 `resume_k`.
- `resume cont_str, value` primitive + `resume_from_serialized`.
- Effect handler pattern: `handle Effect { op => { ... capture happens here ... } } in { ... }`
- `distributed_teleport.hel` demo using a custom `Cluster.verplaats_naar` effect + `Swarm.dispatch`.
- `DistributedMemory` with Lamport clocks and lock-free delta queue (good for state reconciliation after resume).
- Actor isolation (each "Ziel" has private scope + MPSC).

The "teleport" demo already proves the concept works for a controlled case.

## Gaps to Close for Real Self-Healing

1. **Explicit, safe migration primitive** (not just a demo effect).
2. **Automatic / supervisor-driven capture on failure** (detect crash, last good continuation).
3. **Cross-node resumption protocol** over HSP (not just building a string script).
4. **Resource re-acquisition story** after teleport (especially TCP/FFI).
5. **Security & replay protection** for migrated continuations.
6. **Integration with existing distributed primitives** (Concurrent islands, Swarm, flight recorder for observability of migrations).
7. **Supervisor / healing policies** as first-class or easy-to-express patterns.

## Proposed Design

### 1. New / Extended AST Nodes (or effect-based)

Keep most as effects for flexibility:

```rust
// In effects.rs or a new distributed_effects
pub const DISTRIBUTED_EFFECTS: &[(&str, &[&str])] = &[
    ("Swarm", &["dispatch", "migrate", "heal"]),
];
```

User-level surface (in pure .hel or thin prelude):

- `perform Swarm.migrate(target_ip, target_port)` — explicit teleport point.
- Inside a handler for a custom effect or a new top-level `teleport { ... }` construct (sugar).
- Automatic path: supervisor actors can request capture of a running island.

The capture always happens at a `Perform` boundary (clean delimited point).

### 2. Enhanced SerializableContinuation

Extend for self-healing:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableContinuation {
    pub id: u64,
    pub captured_memory: MemorySnapshot,
    pub captured_stack_json: String,
    pub effect: String,                    // which effect was active
    pub resume_value_hint: Option<String>, // type or description
    pub source_node: String,
    pub lamport: u64,                      // for ordering
    pub resource_requirements: Vec<String>, // e.g. ["tcp", "ffi:sqlite"] — advisory
    pub signature: Option<String>,
}
```

On capture, also snapshot relevant `DistributedMemory` deltas if the continuation wants to participate in global state.

### 3. Migration / Teleport Protocol (over existing HSP + Swarm)

Use the existing `Swarm.dispatch` or a dedicated `HspNode::teleport_continuation` path.

Flow:

1. On Node A, during effect handler:
   - `let cont = capture_continuation(...)`
   - Sign it.
   - Serialize (the struct is already serde).
   - Send via HSP (encrypted channel) to target node: a special command type `TeleportContinuation { cont: ..., resume_value: ... }` or embed in the existing command+result protocol.

2. On Node B (receiver):
   - Verify signature (SwarmSigner + node identity).
   - Check replay (lamport or nonce).
   - Allocate a fresh local `MemoryManager`, restore snapshot.
   - Re-acquire any declared resources (user code or policy can decide: re-connect TCP, re-open FFI handles, etc.).
   - Inject `resume_value`.
   - Execute the remaining stack via `evaluate_remaining_stack` (or a new privileged "continuation executor" path).
   - Return result back to origin if it was a request-response teleport, or fire-and-forget for fire-and-recover.

3. On origin after successful ack: the local continuation is considered "migrated" and the local execution aborts the outer statement (as already done in the demo).

### 4. Self-Healing Triggers

Two modes:

**Explicit (user-controlled):**
```hel
effect SelfHeal { on_crash }

handle SelfHeal {
    on_crash => {
        let cont = ... // captured by the runtime on failure detection
        perform Swarm.migrate("healthy-node-3", 9001);
    }
}
```

**Supervisor / Automatic:**
- Extend the Actor model with supervisors (inspired by Erlang).
- A supervisor actor monitors child islands (via flight recorder heartbeats or explicit pings).
- On detected failure (timeout, connection drop, explicit crash signal), the supervisor requests the last captured continuation (or forces a capture point) and triggers teleport to a standby node.
- Flight Recorder can emit "last known good continuation id" events.

### 5. Resource & State Handling on Resume

Critical rule: **Do not pretend resources travel.**

- `ResourceHandle` (TCP, FFI, GPU) IDs are only valid on the original node.
- On resume the continuation should be written so the first statements after resume re-establish what is needed (re-`tcp_verbind`, re-open FFI, etc.) and store new handles.
- The `resume_value` can carry connection parameters or "I was in the middle of sending X bytes, here is the offset".
- For pure computation (no external resources) — perfect migration with zero re-work.

Advisory `resource_requirements` in the continuation help the receiving node or supervisor decide "can I actually resume this here?"

### 6. Security Model

- Every teleported continuation is signed with the originating node's Swarm key (existing `SwarmSigner`).
- Receiving node verifies against known swarm members (DiscoveryService).
- Base64 + explicit `resume` primitive (already present) prevents easy injection.
- Add a short-lived nonce or increasing lamport to prevent replay of old continuations.
- Privileged context on resume only if the original continuation was captured in privileged mode and the target node trusts the source.

### 7. Integration Points

- **Actor model**: Each actor can be treated as a migratable island. `spawn` + supervisor can decide to migrate an actor.
- **Effects system**: The cleanest capture points are exactly the existing `Perform` + handler boundaries.
- **DistributedMemory**: After resume, replay or apply pending deltas using Lamport ordering.
- **Flight Recorder**: Record "continuation captured", "teleported to X", "resumed on Y" as first-class trace events. Enables visual audit and debugging of healing events.
- **HSP / network layer**: Extend the command enum with a `Teleport` variant (or reuse existing dispatch + a well-known remote script pattern, then evolve to native).
- **Namespaces**: Now that first-class namespaces are done (Phase 5), qualified names like `Swarm.migrate` and `SelfHeal.on_crash` are clean and analyzable.

## Implementation Roadmap (Recommended Order)

1. **Stabilize & Document Current Continuation Primitive**
   - Clean up `__REST_AST__` key naming.
   - Make `capture_continuation` and resume paths the single source of truth (remove duplication between executor.rs and actor.rs).
   - Add proper error types for "resume failed verification", "resource not re-acquirable".

2. **Add Explicit `Swarm.migrate` / Teleport Effect**
   - Register in CORE_EFFECTS.
   - Implement handler that does the HSP send of the signed `SerializableContinuation`.
   - Receiving side: native command handler that accepts a continuation blob, verifies, resumes in a fresh or supervised context.

3. **Resource Re-establishment Contract**
   - Document + provide helper functions in a `migration.hel` stdlib module (or core prelude).
   - Example pattern: on resume, if you had a TCP handle, the first thing is `mijn_sock = tcp_verbind(hostport_from_resume_value)`.

4. **Supervisor / Self-Healing Layer (Actor + Flight Recorder)**
   - Build a small supervisor actor that can own islands.
   - On heartbeat loss or explicit failure signal → capture last good continuation (if the island supports it) or use the last recorded one.
   - Trigger migrate.

5. **Observability & Safety**
   - Flight recorder events for every capture/teleport/resume.
   - Optional Lamport + vector clock on continuations for ordering.
   - Timeout + max hop count on a continuation to prevent infinite migration loops.

6. **Polish & Demo**
   - Make `distributed_teleport.hel` (or a new `self_healing_demo.hel`) a first-class example.
   - Add a simple supervisor in pure .hel that migrates a crashing pipeline.
   - Measure overhead (should be near zero until a migration is requested).

7. **Advanced (later)**
   - Partial continuations / checkpointing at explicit `checkpoint` points (not only effect boundaries).
   - Encrypted continuation payloads at rest (in addition to transport encryption).
   - Capability tokens on what a resumed continuation is allowed to do.

## Open Questions & Trade-offs

- **Capture granularity**: Only at `Perform` points (current) vs. any statement (would require more invasive instrumentation of the executor loop).
- **State size**: Full `MemorySnapshot` on every migration can be large. Future: delta snapshots or explicit `checkpoint` that prunes history.
- **Exactly-once vs at-least-once**: When migrating mid-TCP, does the remote side re-send the last bytes or not? User-level protocol decision.
- **CPU-only nodes**: A continuation captured on a GPU-heavy node may need to fall back to CPU implementation of the remaining work on resume.
- **Discovery of "healthy" targets**: Use existing DiscoveryService capabilities (gpu_count, load, etc.) + a simple "willing to accept continuations" flag.

## Success Criteria

- A long-running `data_pipeline` or TCP stream can be paused on Node A (via explicit effect or simulated crash), the `SerializableContinuation` (signed) arrives on Node B, and execution continues from the exact next statement with correct memory and produces the expected final result.
- No resource leaks on the original node.
- Flight recorder shows the full migration trace.
- The mechanism works on pure CPU (old laptop) nodes.

This is the feature that makes Helheim unique: a bare-metal language where **computations themselves are mobile, signed, and self-healing citizens of the Swarm**.

Foundation is solid. This blueprint turns the existing pieces into a coherent, implementable self-healing system.

Ready when you are. We can start with Phase 1 (explicit migrate + transport) immediately.