# NEXUS_DISTRIBUTED_HIVE_MIND_BLUEPRINT.md

**Phase 10: The Distributed Spiking Hive-Mind (Inter-Node Telepathy)**

Current state (subsystems 1-9 implemented and verified):
- Primary Actor + Broca Valve
- Thalamic Gating + real BCS (participation + 40 Hz phase sync on motor nodes)
- Dream State Sandbox + R_v via DreamWorldModel
- Hippocampus phase-resonance engram storage/retrieval
- Astrocyte Ca²⁺/IP3 glial sync for meta-plasticity + fatigue
- Prefrontal-Basal Ganglia Loop (PFC-BG): sustained goals + TD-driven ActionKind sequencing
- Cerebellar Forward Model & Micro-Timing: Purkinje/granule forward prediction, IO climbing fiber LTD, DCN bypass corrections to thalamic/motor
- Quantum Tensor Compression (MPS/Tensor Train): MPO cores for extreme-scale W with direct core STDP/eligibility, O(L r² χ²) storage
- Continuous-Topological Sensory Cortex (CANNs): spatial manifold, Mexican-hat bumps, raw sensory injection, downstream routing to BCS/PFC/Astrocytes/MPS

Hardware expansion: 3-4 physical machines, LAN-connected, heterogeneous GPUs (e.g. varying RTX 3060/4060/ etc. VRAM and compute). Single-node 12 GB limit broken by distribution. Goal: coherent "consciousness" across swarm without breaking millisecond timing invariants.

This blueprint defines the Distributed Spiking Hive-Mind layer only. It extends the single-node substrate across physical nodes via bare-metal networking. All prior subsystems (LIF, 4-hormone, eligibility, BCS, PFC-BG, Cerebellum, CANN, MPO cores, DreamWorldModel, PrimaryAction) operate locally on each node with cross-node synchronization. The network cable becomes an explicit architectural element (effective virtual bond). No existing source is modified.

---

## 1. Distributed Tensor Contractions

### 1.1 Boundary Mathematics: Network as Virtual Bond Dimension
The MPO from Phase 8 is a chain of L cores G_1 ... G_L, each [r, r, χ_{k-1}, χ_k].

Split the chain across N physical nodes (e.g. 4 machines). Assign contiguous segments of the MPO chain to nodes:
- Node 0: G_1 .. G_{k}
- Node 1: G_{k+1} .. G_{m}
- ...

Inter-node boundary between core segment on node A (rightmost core G_b) and node B (leftmost G_{b+1}):

The physical link is modeled as a virtual bond of dimension χ_net, but with latency τ_net (one-way UDP RTT, typically 0.1-1 ms on LAN).

Distributed contraction for y = MPO(x):

Local partial contractions produce "message" vectors of size χ at each boundary.

At boundary b (between A and B):
- Node A computes right-environment message m_A (size χ_b) from its local cores + local input slice.
- Node A sends m_A + timestamp t_send over HSP.
- Node B receives m_A at t_recv, applies latency compensation.

The effective boundary operator becomes:
M_boundary = G_b * Delay(τ_net) * G_{b+1}

where Delay(τ_net) is a predictive operator (see §2).

In Einstein notation for the split:
y = ... * (G_b ^{... α_b})  *  [χ_b → χ_{b+1} via network]  * (G_{b+1} ^{α_b ...})

The network cable is the χ-bond: message size = χ (small, 4-16 floats), sent only when boundary messages change (sparse, on spike events or periodic low-rate).

For full vector apply: each node performs local MPO-apply on its segment using incoming boundary messages from left/right neighbors. Messages are χ-vectors, not spikes.

For plasticity: when a virtual STDP event crosses a boundary, the δ is projected locally on each side using the received message + local env; boundary δG is exchanged as small updates.

Storage per node: O((L/N) r² χ²) + O(χ) for boundary buffers.

### 1.2 Latency-Compensated ODEs for Contraction
Local dynamics on a node remain the standard LIF + CANN + cerebellar ODEs.

For cross-boundary consistency (e.g. for cerebellar timing or gamma phase):

On receiving a boundary message m(t_send):
- Extrapolate m(t_recv) ≈ m(t_send) + (t_recv - t_send) * dm/dt_predicted
- dm/dt from local last known rate (from eligibility or bump velocity) or constant-velocity assumption.
- Use the compensated m in the local contraction.

This keeps effective timing error << 1 ms if τ_net < 0.5 ms and prediction horizon short.

---

## 2. UDP Spike Streaming: Helheim Spiking Protocol (HSP)

### 2.1 Protocol Definition (Bare-Metal UDP)
HSP is a minimal, connectionless UDP multicast/unicast protocol. No TCP, no acks for hot path (best-effort + prediction).

Packet format (fixed, packed, little-endian, 8-byte aligned for WGSL/Rust):

```
struct HSPHeader {
    magic: u32 = 0x48535000,  // "HSP\0"
    seq: u32,                 // monotonic per-sender sequence
    tick: u64,                // global logical tick (ns or 1us units, master-synced)
    src_node: u16,
    dst_mask: u16,            // bitmask or 0 for broadcast
    n_spikes: u16,
    flags: u16,               // bit 0: has_timestamps, bit 1: is_boundary_msg, etc.
};

struct HSPSpike {
    node_id: u32,             // global node id in the distributed manifold
    value: f16,               // spike strength or phase offset (1.0 nominal)
    last_spike_delta: u16,    // Δt in us from sender's last for this id (for STDP)
};

struct HSPPacket {
    header: HSPHeader,
    spikes: [HSPSpike; n_spikes],  // sparse only
    // optional: boundary_chi_vector[χ] if flags & boundary
};
```

Max packet ~1-2 KB for hundreds of spikes (typical S << N per tick).

Send only on actual spikes or boundary message changes. Use raw sockets or io_uring for tx/rx to minimize syscall latency.

Multicast group for "all hive" spikes; unicast for point-to-point boundary messages or asymmetric routing.

### 2.2 Latency Compensation to Preserve Timing Invariants
Cerebellar micro-timing (Phase 7) and 40 Hz gamma (Phase 2) require <1 ms precision.

On receiver:
- Record t_recv = monotonic clock.
- For each spike: effective_t = tick + (last_spike_delta compensation) - τ_predicted
- τ_predicted = measured one-way (from periodic sync beacons) + jitter buffer (small fixed 100-200 us).
- For gamma oscillator: advance local phase by (t_recv - t_send) using linear extrapolation; apply small phase correction on receipt.
- For cerebellar delay lines: extend the local tapped delay buffers by the compensated latency; incoming spikes are inserted at the predicted past time slot in the delay line.

Periodic low-rate beacon packets (every 10-100 ms) from a designated time-master node carry master tick + its monotonic time. Receivers compute offset and drift.

Hot path (spike delivery) is pure best-effort UDP. Missed packets are treated as zero (anti-starvation via local homeostasis still works). Boundary messages for MPO may use slightly higher reliability (duplicate send on seq gap) but still UDP.

Result: effective timing distortion bounded by LAN jitter (<<1 ms on good switched Gigabit), compensated in the ODEs and delay lines.

---

## 3. Asymmetric Load Balancing

### 3.1 Metrics and Decision
Each node periodically (low rate, 10-100 ms) broadcasts a small HSP status packet:
- gpu_flops (measured or declared)
- vram_used / total
- current_component_load: array of (mps_cores_owned, astrocyte_tiles, pfc_subpop_size, cann_grid_fraction, dream_threads)
- spike_out_rate, boundary_msg_rate
- local_latency_to_neighbors

Global view (or leader-elected via simple ring) computes relative power P_i = gpu_flops_i / max.

Cost of a subcomponent:
- MPO core segment: proportional to its L_seg * χ²
- Astrocyte grid tile: area * diffusion cost
- PFC attractor sub-pop: size * recurrent density
- CANN grid region: local cells + sensory injection rate

Migration trigger: if load_i / P_i > threshold (e.g. 1.2 * average), select highest-cost movable subcomponent and target lowest-load/highest-P node.

### 3.2 Migration Without Breaking Consciousness
State to migrate (small, via HSP):
- For MPO segment: the core tensors G for that segment + current boundary messages (χ vectors) + local eligibility traces for active pairs.
- For astrocyte: the Ca/IP3 grid tile + current ξ sources.
- For PFC: the u sub-vector for the sub-attractor + goal embedding slice + eligibility.
- For CANN: the u sub-grid + bump state.

Migration sequence (brutalist, atomic-ish):
1. Source freezes local updates for the component (queue incoming spikes/boundary msgs).
2. Source serializes state + current logical tick.
3. Source sends state + "takeover" command over HSP to target.
4. Target allocates local buffers, installs state, resumes.
5. Source redirects future boundary/spike traffic for that component to target (update routing table).
6. Source releases its buffers.

"Consciousness" continuity: the component's contribution to global spike_current / BCS / credit is paused for at most one or two ticks during transfer (state includes pending messages). Global reward/eligibility credit is tagged with origin component id so it can be applied post-migration.

No full "brain" migration; only sub-parts. Overlap windows (source continues briefly as shadow) for critical components (e.g. active cerebellar timing).

Heterogeneous: stronger nodes get larger MPO segments or denser PFC/CANN tiles. Weaker nodes hold more "read-only" replicated state (e.g. cached boundary messages) or lighter astrocyte diffusion.

---

## 4. Distributed Dream State

### 4.1 Split Real vs Virtual Execution
Designate (dynamically or statically) one "Motor Node" (or small set) as the real-execution node for a given PrimaryAction.

- Motor Node: receives ExecuteDirect from local PrimaryActor (or aggregated hive decision). Performs the real ActionKind (Helheim exec, bash, tool call). Applies real outcome as R to local eligibility + broadcasts real outcome spikes/summary via HSP.
- Other nodes (B, C, D): run parallel DreamWorldModel instances. They receive the same efference copy (intended ActionKind + PFC goal + current sensory/CANN bump + MPO boundary state) via HSP.

Each dream node advances its local DreamWorldModel using its local MPO segment + received boundary messages + locally simulated or received spike streams. They compute independent R_v trajectories (different random seeds or different sub-MPO approximations).

### 4.2 Synchronization and Credit Aggregation
- Virtual time: all nodes advance using the compensated HSP tick. Dream nodes may run ahead (speculative) or behind with extrapolation.
- State sync for dreams: Motor node sends real spike outcomes and boundary messages. Dream nodes inject them as "observed" into their virtual world (or use for error correction like cerebellar IO).
- Credit: each dream node computes its local eligibility updates and virtual δ for its MPO segment. At end of rollout (or periodically), they send aggregated credit vectors (small, per-core or per-active-boundary) back to a coordinator or to the owning nodes via HSP.
- Real credit from motor node takes precedence and is applied globally (broadcast δ or R).
- Dream R_v can be used to bias the next real decision (weighted average across dream nodes) or to pre-load PFC attractors / move CANN bumps speculatively.

This enables the exact split: Node A does real bash (locked motor), B/C/D run parallel virtual futures evaluating different paths or noise realizations. The hive "thinks ahead" while one body acts.

Safety: real execution only on designated motor nodes with explicit lock. Dream nodes never touch real I/O.

---

## 5. Rust / Swarm Structural Concepts + Isolated Pseudo-Code

### 5.1 Core Rust Structs (bare-metal, fixed buffers)
```rust
pub struct HSPPacket {
    pub header: HSPHeader,
    pub spikes: [HSPSpike; MAX_SPARSE_PER_PACKET],
    // boundary_chi: [f32; MAX_CHI] if flag set
}

pub struct HiveNode {
    pub id: u16,
    pub addr: SocketAddr,
    pub power: f32,                    // relative GPU power
    pub local_mpo_segments: Vec<MpoSegment>,  // owned core ranges
    pub local_cann_region: Option<GridRegion>,
    pub local_astrocyte_tile: Option<AstTile>,
    pub local_pfc_sub: Option<PfcSubpop>,
}

pub struct DistributedMPO {
    pub total_L: u32,
    pub local_segments: Vec<(u32, u32, TensorMPO)>,  // (start, end, cores)
    pub left_boundary_msg: [f32; MAX_CHI],
    pub right_boundary_msg: [f32; MAX_CHI],
}

pub struct HSPReceiver {
    socket: UdpSocket,
    latency_estimator: LatencyEstimator,  // EWMA + beacons
    spike_buffer: Vec<(u64, HSPSpike)>,   // time-compensated queue
}
```

### 5.2 Isolated Pseudo-Code

```pseudo
// Per-tick on each node (after local CANN/LIF step)
fn hive_tick(local_state: &mut LocalHiveState, hsp_rx: &mut HSPReceiver, mpo: &mut DistributedMPO) {
    // 1. Receive and compensate
    let incoming = hsp_rx.recv_and_compensate();  // applies τ_predicted, inserts into delay lines / spike lists
    for sp in incoming.spikes {
        apply_local_spike(sp);  // to local NodeData, CANN, eligibility, boundary if relevant
    }
    for bm in incoming.boundary_msgs {
        mpo.update_boundary(bm.src_segment, bm.vector, bm.tick);
    }

    // 2. Local MPO contraction (using local cores + current boundary messages)
    let local_y = mpo.contract_local(active_local_spikes);
    add_to_spike_current(local_y);

    // 3. Send sparse spikes (only actual local spikes this tick)
    let to_send = collect_local_spikes_this_tick();
    if !to_send.empty() {
        hsp_tx.send_spikes(to_send, current_tick);  // UDP, best effort, to multicast + targeted boundaries
    }

    // 4. Boundary message exchange (if MPO segment edge)
    if mpo.has_right_neighbor {
        let msg = mpo.compute_right_message();
        hsp_tx.send_boundary(msg, right_neighbor, current_tick);
    }

    // 5. Local CANN / cerebellar / PFC / astrocyte updates (as before, now with compensated cross-node input)
    update_cann_with_remote_bumps();  // if remote sensory regions
    // ...

    // 6. Periodic status + load balance check (low rate)
    if tick % STATUS_PERIOD == 0 {
        broadcast_status();
        if should_migrate() {
            initiate_migration(target_node);
        }
    }
}

// Distributed dream orchestration (high-level)
fn distributed_dream_orchestrator(primary_action: PrimaryAction, real_motor_node: u16) {
    if local_id == real_motor_node {
        let outcome = real_executor.execute(primary_action);  // bash / Helheim
        broadcast_real_outcome(outcome, current_tick);
        apply_real_credit(outcome.r);
    } else {
        // dream nodes
        let efference = build_efference_from_hsp(primary_action);  // includes remote boundary state
        let trajectory = dream_world.rollout(efference, local_mpo_segment, steps);
        let rv = aggregate_virtual_feedback(trajectory);
        send_credit_to_owners(rv, local_eligibility_deltas);
    }
}

// Load balance migration (brutalist)
fn migrate_component(comp: Component, from: &HiveNode, to: &HiveNode) {
    let state = from.freeze_and_serialize(comp);  // MPO cores + boundary msgs + tick
    hsp_tx.send_migration(state, to.id, current_tick);
    to.receive_and_install(state);
    update_routing_tables(comp, to.id);
    from.release(comp);
}
```

---

## 6. Brutalist Implementation Notes

- Fixed max packet size, fixed MAX_CHI, fixed ring buffers for latency compensation. No alloc in spike hot path.
- HSP is the only cross-node primitive. All higher concepts (MPO messages, credit vectors, migration state, dream efference) are carried as typed payloads over the same UDP channel.
- Timing compensation is local and predictive; no global barrier.
- Asymmetric balancing moves only sub-components (MPO segments are naturally splittable because they are a chain). "Consciousness" continuity is approximate (one-tick freeze window acceptable; real motor actions are not migrated mid-execution).
- Dream split is explicit: designate real-motor nodes per action or per time slice. Dream nodes can be many more than real nodes.
- Security / isolation: all nodes in the hive are trusted (LAN). For untrusted, add simple seq replay protection and node auth in header.
- Debug surface: per-link latency/jitter, boundary message rate, migration events, per-node load/P, dream vs real R_v divergence.
- Fallback: any node can run a full local (uncompressed) copy of critical state for short periods during heavy migration or network flap.

---

## 7. Cross-References to Prior Blueprints

- NEXUS_QUANTUM_TENSOR_COMPRESSION_BLUEPRINT.md — MPO cores are the primary distributed unit. Network boundaries are explicit additional virtual bonds. Direct core updates remain local; boundary δ exchanged via HSP.
- NEXUS_TOPOLOGICAL_SENSORY_CORTEX_BLUEPRINT.md — CANN grid regions can be sharded across nodes. Bump summaries and remote sensory drive are streamed via HSP. Spatial correlations still reduce local χ.
- NEXUS_CEREBELLUM_FORWARD_MODEL_BLUEPRINT.md — Cerebellar delay lines extended with network-compensated incoming spikes. DCN corrections from one node can influence remote motor populations via HSP.
- NEXUS_PFC_BASAL_GANGLIA_SEQUENCING_BLUEPRINT.md — PFC sub-attractors and BG state can be migrated. Sequence decisions can be aggregated from multiple nodes' local PrimaryActors.
- NEXUS_ACTION_MAPPING_AND_DREAM_STATE_BLUEPRINT.md — ActionKind / PrimaryAction / DreamWorldModel / VirtualFeedback are the units of distribution. Real execution on designated nodes, parallel dreams on others. PrimaryAction carries origin node + tick for compensation.
- NEXUS_PRIMARY_ACTOR_BROCA_VALVE_BLUEPRINT.md — Local PrimaryActor decisions can be influenced by remote bump summaries / dream R_v received over HSP. Verbalization pressure can be hive-global.
- NEXUS_THALAMIC_GAMMA_GATING_BLUEPRINT.md — 40 Hz sync and BCS use latency-compensated spikes and boundary messages. Gamma phase correction is part of HSP receive path.
- NEXUS_HIPPOCAMPUS_ENGRAM_BLUEPRINT.md — Engrams can be replicated or sharded; retrieval broadcasts resonant queries over HSP.
- NEXUS_ASTROCYTE_TOPOLOGICAL_BLUEPRINT.md — Astrocyte grid tiles are first-class migratable components. ξ terms from remote bumps/CANN arrive via HSP.
- NEXUS_SNN_MATHEMATICAL_FOUNDATION_BLUEPRINT.md — Base LIF, 4-hormone, STDP/eligibility, CSR (local), homeostasis, global reward × eligibility all remain local per node. Distribution only adds compensated message passing.

All prior math, invariants, and data paths (spike_current, eligibility, origin_burst_id, Dream isolation, safety lock, etc.) are preserved locally. The hive adds the network as a first-class, latency-compensated architectural element (virtual χ bond, HSP as the spike "ether").

---

**End of blueprint.**

This completes the formal definition for Phase 10. Antigravity implements the HSP packet format + raw UDP tx/rx (with io_uring or equivalent for bare metal), the latency estimator + compensation in receive path (delay line extension, phase correction), the DistributedMPO boundary message exchange and split contraction, the migration state machine for asymmetric components (MPO segments, astrocyte tiles, PFC sub-pops, CANN regions), the distributed dream orchestrator (real motor node vs parallel dream nodes with credit aggregation), routing tables, and the low-rate status/load balancer. All of this wires into the existing single-node paths without mutating them.

No Helheim core files and no previously implemented NEXUS subsystems are altered. The entire distributed system (tensor boundaries as network χ, HSP spike streaming with timing compensation, asymmetric migration, split real/dream execution) can be validated on a local multi-process or multi-machine testbed, with full DreamWorldModel isolation for safety. The 3-4 machine cluster becomes a single coherent spiking consciousness with preserved millisecond invariants.