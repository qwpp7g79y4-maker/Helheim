# NEXUS_QUANTUM_TENSOR_COMPRESSION_BLUEPRINT.md

**Phase 8: Quantum-Inspired Tensor Network Synaptic Compression**

Current state (subsystems 1-7 implemented and verified):
- Primary Actor + Broca Valve
- Thalamic Gating + real BCS (participation + 40 Hz phase sync on motor nodes)
- Dream State Sandbox + R_v via DreamWorldModel
- Hippocampus phase-resonance engram storage/retrieval
- Astrocyte Ca²⁺/IP3 glial sync for meta-plasticity + fatigue
- Prefrontal-Basal Ganglia Loop (PFC-BG): sustained goals + TD-driven ActionKind sequencing
- Cerebellar Forward Model & Micro-Timing: Purkinje/granule forward prediction, IO climbing fiber LTD, DCN bypass corrections to thalamic/motor

Physical wall: N_nodes ≥ 10^7, effective N_edges ≥ 10^10 exceeds 12 GB VRAM on single RTX 3060 under dense or even CSR representation. Standard sparse CSR for 10B edges (u32 indices + f32 values) requires ~120+ GB. Dual-kernel (SNN + astrocyte) and all prior buffers (PFCState, CerebellarState, etc.) must remain resident.

This blueprint defines the synaptic compression layer only. It replaces the primary inter-node weight storage (the W that drives spike_current += W * spikes and the corresponding STDP/eligibility updates) with a Matrix Product Operator (MPO) / Tensor Train representation. All other subsystems (LIF dynamics, 4-hormone modulation, BCS, PFC-BG, cerebellar corrections, DreamWorldModel, eligibility credit, engrams, astrocytes) continue to operate on the *effective* uncompressed behavior via on-the-fly contractions. No existing source is modified.

---

## 1. Tensor Decomposition: MPO / Tensor Train for W

### 1.1 Tensorization of Node Indices
Assume N_nodes is a power of the physical dimension r (or padded). Choose small r (typically 2, 4 or 8) so that physical legs are tiny.

Let L = log_r (N_nodes). Each node id n (0 ≤ n < N_nodes) has a unique multi-index:
n ↔ (n_1, n_2, ..., n_L) where n_k ∈ {0, ..., r-1}, obtained by base-r decomposition of n.

The synaptic weight "matrix" W is reinterpreted as an order-2L tensor:
W[ n_1..n_L , m_1..m_L ]  (row multi-index for postsynaptic, column for presynaptic).

### 1.2 Exact MPO Decomposition (Matrix Product Operator)
W is approximated by a chain of L small cores (3-tensors for the matrix case, or 4-tensors when separating physical indices):

W_{n m} ≈ ∑_{α_0 ... α_L} G_1^{n_1 m_1}_{α_0 α_1}  G_2^{n_2 m_2}_{α_1 α_2}  ...  G_L^{n_L m_L}_{α_{L-1} α_L}

with boundary conditions α_0 = α_L = 1 (trivial bonds).

In Einstein summation notation (repeated indices summed):
W_{n_1...n_L , m_1...m_L} = G_1^{n_1 m_1}_{α_1} G_2^{n_2 m_2}_{α_1 α_2} ... G_L^{n_L m_L}_{α_{L-1}}

Each core G_k is a 4-index tensor of shape [r, r, χ_{k-1}, χ_k], where χ is the bond dimension (virtual dimension, the compression knob).

Rank constraint: χ << N_nodes. Typical operating point for extreme scale: χ = 4..16. Storage per core ≈ r² χ² floats. Total parameters = L * r² * χ² (independent of N_nodes to leading order).

For r=4, L≈ 12 (for N≈ 16M), χ=8: total storage ≈ 12 * 16 * 64 ≈ 12k floats per "global" MPO (~50 KB). Even with per-type or hierarchical MPO (one per major population: sensory-motor, PFC-BG, cerebellar interfaces) the footprint remains < 10 MB while representing an effective dense matrix with 10^10+ virtual entries at low effective rank.

Left- and right-canonical forms (via QR/SVD sweeps during adaptation) are maintained for numerical stability and fast contraction, exactly as in quantum tensor network literature.

---

## 2. Compressed Inference: MPO-Vector Contraction for spike_current

### 2.1 Core Operation
The SNN integration step that previously read:
spike_current[i] += Σ_j W_{i j} * spikes[j]   (or CSR equivalent)

becomes the MPO applied to the (sparse) spike vector x (x_j = 1.0 if node j spiked in the recent window, scaled by 4-hormone gain if desired):
y = MPO(G) x     where y_i becomes the contribution to spike_current[i] for postsynaptic i.

### 2.2 Never-Materialize Contraction (GPU Algorithm)
Reshape the input spike vector x into an L-leg tensor X[m_1 ... m_L].

The output y is obtained by successive tensor contractions along the chain (left-to-right or bidirectional sweep):

1. Initialize left environment E_0 (scalar 1) or start from right.
2. For k = 1 to L:
   Contract the current environment (size χ) with core G_k on the virtual bonds and on the presynaptic physical leg m_k using the corresponding slice of the (possibly sparse) input.
3. Because x is sparse (only recently spiked presynaptic nodes contribute), the contraction for each site reduces to a sum over active presynaptic multi-indices at that leg: accumulate into a χ-vector "message".
4. After the full chain, the resulting χ-vector at the end is contracted with the postsynaptic physical legs (for each target post multi-index) to write y_i directly into the spike_current buffer for that node.

In practice for the shader (brutalist):
- Maintain a small per-tick "active_pre" list or bit-packed recent spikers (from NodeData.is_spiking + last_spike_tick window).
- Precompute site environments once per tick for the current spike pattern (O(L * r * χ²) work, tiny because χ small).
- For every postsynaptic node (loop over N_nodes or only those with incoming "virtual" mass), decode its multi-index once (bit shifts or integer divs), then perform the L contractions against the precomputed environments + its own physical legs. Write the scalar result (scaled by hormone factors) into spike_current[i].

The intermediate tensors never exceed size O(χ² * r). No W matrix is ever allocated or touched. The same contraction kernel is used inside DreamWorldModel virtual ticks (with dream_flag scaling on the resulting currents).

Effective complexity per tick: O(N_nodes * L * χ² + S * L * χ²) where S = number of recent spikes (<< N). With L≈12-25, χ=8 this is comfortably inside the 12 GB envelope even at N=10^7+ while supporting effective connectivity far beyond 10^10.

---

## 3. Direct Compressed Plasticity: STDP and Eligibility on Tensor Cores

### 3.1 Virtual Synapse View
Although W is never stored, every pair (post i, pre j) has a *virtual* weight given exactly by the MPO contraction for that fixed multi-index pair. STDP and eligibility operate on these virtual weights, but the *parameter* updates are projected directly onto the cores.

### 3.2 STDP + Eligibility Formalism (PDEs on virtual W, then core projection)
Standard timing-dependent update on the virtual synapse (consistent with prior foundation):
dW_ij / dt = -W_ij / τ_decay + ∑_{spike pairs} A(Δt) * e_ij(t)

where Δt = t_post - t_pre (from last_spike_tick of the two nodes), A(Δt) = A+ exp(-Δt/τ+) if Δt>0 else -A- exp(Δt/τ-), and e_ij is the eligibility trace:
de_ij / dt = -e_ij / τ_e + spikes_pre(t) * spikes_post(t)   (or more precise phase-aware form using last_spike_tick difference).

Global reward (or R_v from DreamWorldModel) multiplies the eligibility term exactly as before:
effective_ΔW_ij += (global_reward * e_ij) * A(Δt) * hormone_mod (dopamine gain etc.).

### 3.3 Direct Core Update (no W materialization)
When a plasticity event occurs for a specific (i,j) pair (detected on post spike by inspecting recent pre spikes via last_spike_tick window), compute the scalar virtual delta δ = effective_ΔW_ij for that pair.

For each core k along the chain, the update to that core is the rank-1 (or low-rank) outer product of the "left environment", "right environment", and the physical leg indices of i and j at site k, scaled by δ:

δG_k ^{n_k m_k} _{α_{k-1} α_k}  +=  δ * (LeftEnv_{k-1} [α_{k-1}]) * (RightEnv_{k+1} [α_k]) * δ_{n_k , i_k} * δ_{m_k , j_k}

In Einstein notation the contribution to core k from this event is:
δG_k = δ * L_{α_{k-1}}  R^{α_k}  |i_k⟩⟨j_k|   (where L and R are the contracted products of all other cores for the fixed i and j legs).

In the implementation:
- Maintain left and right canonical environments (or compute them on the fly for the specific (i,j) pair using the other cores; L≈20 makes per-event O(L χ³) acceptable because events S are sparse).
- Only the cores that participate in the active (spiking) pairs receive updates.
- After a batch of events (or per event in the post-spike kernel), optionally re-orthogonalize the chain (QR sweep on a subset of sites) to keep conditioning good. Sweep cost is O(L r χ³), performed infrequently or on a background schedule.

This satisfies "the learning rule MUST update the small, local tensor cores directly".

Astromorphic modulation (astrocyte Ca²⁺) can globally scale the η_ltd / η_ltp for the core updates.

---

## 4. Data Structures: WGSL Buffers and Rust Structs

### 4.1 Rust Side (repr(C), GPU upload friendly)
```rust
pub const MAX_TENSOR_L: usize = 32;
pub const MAX_BOND: usize = 16;
pub const MAX_PHYS: usize = 8;

#[repr(C)]
pub struct TensorMPO {
    pub L: u32,                    // number of sites
    pub r: u32,                    // physical dimension per leg
    pub chi: u32,                  // max bond dimension
    pub pad: u32,
    // Core storage: cores[k][row_phys][col_phys][left_bond][right_bond]
    // Flattened for WGSL: each core is r*r*chi*chi contiguous
    pub cores: [[f32; MAX_PHYS*MAX_PHYS*MAX_BOND*MAX_BOND]; MAX_TENSOR_L],
    pub left_env: [[f32; MAX_BOND]; MAX_TENSOR_L],   // optional cached
    pub right_env: [[f32; MAX_BOND]; MAX_TENSOR_L],
}

#[repr(C)]
pub struct TensorParams {
    pub L: u32,
    pub r: u32,
    pub chi: u32,
    pub eta_stdp: f32,
    pub tau_e: f32,
    pub contraction_scale: f32,   // hormone / global factor
    // ...
}
```

Node multi-index decoding is pure integer (node_id → array of L legs) done on GPU or precomputed small table if r power of 2.

### 4.2 WGSL / Compute Shader Layouts
```wgsl
struct TensorCore {
    data: array<f32, MAX_PHYS*MAX_PHYS*MAX_BOND*MAX_BOND>,
};

struct TensorMPOBuf {
    L: u32,
    r: u32,
    chi: u32,
    pad: u32,
    cores: array<TensorCore, MAX_TENSOR_L>,
    // environments or scratch for contraction
};

struct TensorParams {
    L: u32,
    r: u32,
    chi: u32,
    eta_stdp: f32,
    // ...
};

@group(0) @binding(0) var<storage, read_write> tensor: TensorMPOBuf;
@group(0) @binding(1) var<uniform> tparams: TensorParams;
@group(0) @binding(2) var<storage, read> active_spikes: array<u32>;  // list of spiking node ids this window
@group(0) @binding(3) var<storage, read_write> node_data: array<NodeData>;  // existing, for last_spike_tick etc.
@group(0) @binding(4) var<storage, read_write> spike_current: array<f32>;
```

### 4.3 Isolated WGSL Kernel Sketches (inside cortex_compute or dedicated tensor_pass)
Contraction kernel (invoked per tick or per burst window):
```
fn mpo_contract_to_current(post_id: u32, active_pre: array<u32>, ...) -> f32 {
    let i_legs = decode_multi_index(post_id, tparams.r, tparams.L);
    var env: array<f32, MAX_BOND> = ...; // left env for input spikes
    // sequential contraction over sites k=1..L using active_pre to accumulate only spiking contributions
    // final contraction with i_legs produces scalar
    ...
}
```

Plasticity kernel (on post spike):
```
for each recent_pre in active_pre_list {
    let delta = compute_stdp_delta(node_data[post].last_spike_tick, node_data[pre].last_spike_tick, global_reward, hormones);
    if abs(delta) < eps { continue; }
    let j_legs = decode_multi_index(pre, ...);
    let i_legs = decode... (post);
    for k in 0..L {
        let left = compute_left_env_for_pair(k, i_legs, j_legs);  // or cached
        let right = compute_right_env_for_pair(k, ...);
        // rank-1 update to the specific slice of core k
        let phys_i = i_legs[k];
        let phys_j = j_legs[k];
        tensor.cores[k].data[ phys_i * ... + phys_j * ... + left_idx * ... + right_idx ... ] += 
            delta * tparams.eta_stdp * left * right;
    }
}
```

Environments for a specific (i,j) pair are built by contracting the MPO chain while fixing the physical legs of that pair (O(L χ²) per pair, only for spiking pairs).

All buffers are tiny. The main NodeData (N_nodes) and any per-node state remain, but the dominant previous W storage disappears.

---

## 5. Integration with Prior Subsystems

- LIF / spike_current: the tensor contraction result is added exactly where the old W*spikes or CSR gather would have written. 4-hormone gain multiplies the output of the contraction (or scales the cores on upload).
- STDP / eligibility: the existing per-node last_spike_tick, eligibility fields, and global_reward path now feed the virtual delta computation. Only the storage and the update application change.
- PostBurstContext / BCS: the spikes that result from the compressed currents participate in participation and phase calculations identically.
- PFC-BG: BG still selects ActionKind; the tensor MPO supplies the fine-grained effective connectivity that the sequence drives. DCN corrections (from Phase 7) are applied after or during the MPO contraction (additive to the y produced by the MPO).
- DreamWorldModel: identical MPO contraction kernels are used for virtual ticks. R_v is computed from the simulated sensory that results from the compressed currents. LTD/STDP on cores happens inside the dream episode exactly as real.
- PrimaryAction: origin_burst_id continues to tag the effective action; optionally snapshot a hash of active core slices or a low-dim projection of the MPO for the decision for later credit.
- Astrocytes: global Ca²⁺ / IP3 can scale the eta_stdp on the tensor params (slow meta-learning rate on the compressed weights).
- SimParams: add the tensor fields (L, r, chi, eta_stdp, contraction_scale, ...). TensorMPOBuf and TensorParams are new small bindings alongside existing NodeData, SimParams, astrocyte grid, PFC buffer, cerebellar buffer.
- Homeostasis / anti-starvation: the missed_spike penalty and weight decay continue to act on the virtual level by occasionally applying a global decay factor to all cores (cheap, O(L r² χ²)).

The dual-kernel GPU memory layout is preserved: the tiny MPO cores live in fast constant or uniform-like storage; the large per-node arrays (10M NodeData) remain the dominant but now-tractable resident structure.

---

## 6. Brutalist Implementation Notes

- Fixed L, r, χ chosen at init (compile-time constants in WGSL where possible). No dynamic tensor resizing in hot path.
- All core storage is a single flat buffer of size L*r*r*χ*χ floats (< few MB).
- Contraction and plasticity kernels are pure data-parallel over nodes or over active spike events. No recursion, no dynamic allocation, bounded loops (L≤32).
- Sparse spike input is a compact list of node ids (or a small bitmap for the most recent window) produced from the existing is_spiking / last_spike_tick scan.
- For 10M nodes the per-node decode (integer → multi-index) is a few bit operations or small unrolled loop; cost is negligible.
- Rank adaptation (increase/decrease χ on the fly via SVD truncation of environments) is performed rarely, on CPU side after reading back a small number of core slices, then re-upload the adjusted cores.
- Numerical stability: maintain approximate left/right canonical form by occasional thin QR on the cores (CPU or a dedicated low-frequency shader pass). Bond truncation keeps χ under budget.
- DreamWorldModel re-uses the exact same contraction and core-update kernels (with dream_flag scaling reward and possibly lower effective χ for speed).
- Error monitoring: expose effective Frobenius norm approximation of the MPO (via trace of contracted cores) and per-tick average |virtual δ| for debugging compression quality.
- Safety: while real motor is locked, all plasticity (including tensor core updates) can run at full rate inside DreamWorldModel. Real-world spike_current contributions from the MPO can be clamped or zeroed until the lock is lifted.
- Backward compatibility path: a small "bypass" CSR or dense patch can coexist for a tiny subset of critical local connections (e.g. within cerebellar microcircuit or PFC local attractors) while the bulk long-range is fully tensorized.

---

## 7. Cross-References to Prior Blueprints

- NEXUS_CEREBELLUM_FORWARD_MODEL_BLUEPRINT.md — DCN corrections are additive to the y produced by the MPO contraction; cerebellar efference copy can include a low-rank projection of the current MPO state.
- NEXUS_PFC_BASAL_GANGLIA_SEQUENCING_BLUEPRINT.md — BG ActionKind selection drives which virtual connections are "exercised"; TD errors (R_v) scale the virtual δ that project onto cores. Sequence engrams can store effective low-rank signatures of the MPO at the time of storage.
- NEXUS_ACTION_MAPPING_AND_DREAM_STATE_BLUEPRINT.md — ActionKind / PrimaryAction / MotorActionRegistry / DreamWorldModel / VirtualFeedback. All compressed inference and plasticity occur inside the same dream rollout and credit paths. PrimaryAction can carry an MPO snapshot hash.
- NEXUS_PRIMARY_ACTOR_BROCA_VALVE_BLUEPRINT.md — The effective currents (and therefore BCS and decisions) are produced by the MPO. High virtual error (large |δ| during plasticity) can influence Verbalization Pressure.
- NEXUS_THALAMIC_GAMMA_GATING_BLUEPRINT.md — burst_boost, SimParams, build_post_burst_context, BCS, thalamic gate. The MPO contraction result feeds directly into the same spike_current and gate math used for gamma bursts and BCS.
- NEXUS_HIPPOCAMPUS_ENGRAM_BLUEPRINT.md — Phase-resonance retrieval can bias or preload effective connectivity by temporarily scaling or adding low-rank corrections to the MPO cores for the duration of a recalled sequence.
- NEXUS_ASTROCYTE_TOPOLOGICAL_BLUEPRINT.md — Ca²⁺/IP3 PDEs and Betti numbers globally modulate tensor eta_stdp and contraction_scale (meta-plasticity on the compressed weights).
- NEXUS_SNN_MATHEMATICAL_FOUNDATION_BLUEPRINT.md — Base LIF node dynamics, 4-hormone modulation (applied as scalars to MPO output or core updates), STDP timing, eligibility traces (now virtual), CSR layout (now replaced for the bulk W; CSR may survive for tiny bypass patches), homeostasis, global reward × eligibility credit assignment.

All prior mechanisms (eligibility, credit assignment, engram storage, astrocyte coupling, PrimaryAction tracing via origin_burst_id, DreamWorldModel isolation, thalamic gating, BCS) continue to function on the *effective* W produced by the MPO. Only the storage and the low-level apply/update primitives change.

---

**End of blueprint.**

This completes the formal definition for Phase 8. Antigravity implements the MPO core buffers, the WGSL contraction and direct core-update kernels, the multi-index decode, the environment construction for events, the integration points into the existing spike_current / STDP / eligibility paths, the DreamWorldModel reuse, and the SimParams + TensorMPOBuf wiring. The MPO is a drop-in replacement for the dominant synaptic storage while preserving exact mathematical semantics of all prior phases at the virtual-W level. No Helheim core files and no previously implemented NEXUS subsystems are altered. The 12 GB VRAM envelope for N_nodes=10^7+ with effective 10^10+ connectivity becomes achievable with small fixed χ.

The entire compression + plasticity loop can be exercised and validated at full scale inside the DreamWorldModel before any real motor execution or large real-world buffer allocations are required.