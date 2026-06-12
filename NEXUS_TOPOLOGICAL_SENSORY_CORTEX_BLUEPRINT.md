# NEXUS_TOPOLOGICAL_SENSORY_CORTEX_BLUEPRINT.md

**Phase 9: Continuous-Topological Sensory Cortex using Continuous Attractor Neural Networks (CANNs)**

Current state (subsystems 1-8 implemented and verified):
- Primary Actor + Broca Valve
- Thalamic Gating + real BCS (participation + 40 Hz phase sync on motor nodes)
- Dream State Sandbox + R_v via DreamWorldModel
- Hippocampus phase-resonance engram storage/retrieval
- Astrocyte Ca²⁺/IP3 glial sync for meta-plasticity + fatigue
- Prefrontal-Basal Ganglia Loop (PFC-BG): sustained goals + TD-driven ActionKind sequencing
- Cerebellar Forward Model & Micro-Timing: Purkinje/granule forward prediction, IO climbing fiber LTD, DCN bypass corrections to thalamic/motor
- Quantum Tensor Compression (MPS/Tensor Train): MPO cores for extreme-scale W with direct core STDP/eligibility, O(L r² χ²) storage

Deficit: Sensory input remains flat lexical embeddings (Oculus/Mouth vectors). No native spatial topology for raw continuous streams (tonotopic Pink Noise from PepeDSP, retinotopic 2D visual). No bump formation, no intrinsic spatial correlation for downstream compression or routing.

This blueprint defines the Continuous-Topological Sensory Cortex layer only. It organizes a subset of LIF nodes (node_type == 1) into an explicit manifold with CANN dynamics. All other subsystems continue unchanged; the topological layer feeds spatial summaries and modulated currents into existing paths (spike_current, PostBurstContext, thalamic gate, PFCState, astrocyte injection, MPS contraction). No existing source is modified.

---

## 1. Spatial Topology

### 1.1 Manifold Definition
A fixed subset of nodes (node_type == 1) forms a 2D (or 3D) continuous manifold embedded in the discrete LIF grid. Use a square or hexagonal lattice for GPU efficiency (square preferred for WGSL texture-like access; hexagonal for better isotropy).

Assign each sensory node i (node_type==1) explicit continuous coordinates:
pos_i = (x_i, y_i) ∈ [0, W] × [0, H]

Grid resolution chosen so total sensory nodes N_s = W * H (or ~0.866 W H for hex) fits within target N_nodes budget (e.g. 256×256 = 65k or 512×512 = 262k). Coordinates are stored as fixed arrays, not derived from node id.

The manifold is a subset: only node_type==1 nodes participate in CANN dynamics. Other node_types (motor=3, PFC, cerebellar, etc.) remain unstructured or use their own topologies.

### 1.2 Mathematical Formalism (PDE for Topological Integration)
Let u(x, y, t) be the continuous activation field on the manifold (approximated by discrete LIF voltages or rates of the node_type==1 subset).

The CANN dynamics (continuous limit):
τ ∂u/∂t = -u(x,y,t) + ∫∫ K(x-x', y-y') f(u(x',y',t)) dx' dy' + I_sensory(x,y,t) + I_bias + η_noise

where:
- τ : time constant (shared with or derived from LIF τ of sensory nodes)
- f(u) = max(0, u) or sigmoid (WGSL-friendly ReLU-like)
- I_sensory(x,y,t) : injected sensory drive (see §2)
- I_bias : global or PFC-modulated offset
- η_noise : additive or multiplicative noise (from astrocyte or thermal)

Discretized Euler on the grid (per tick or sub-tick):
u_new[x,y] = u[x,y] * (1 - dt/τ) + dt * (conv(K, f(u)) + I_sensory[x,y] + I_bias)

This PDE runs only on the sensory grid subset. The resulting u field drives spike probability or direct current into the corresponding LIF nodes (node_type==1), which then participate in the global spike_current and BCS calculations.

---

## 2. Spatio-Temporal Injection

### 2.1 Mapping Raw Arrays to Localized Bumps
Raw sensory stream S (1D spectrogram of length F or 2D image of size W_img × H_img) is resampled or projected onto the manifold grid.

For each grid location (x,y):
I_sensory(x,y,t) = Σ_k w_k * S_k(t) * G(x - μ_k, y - ν_k ; σ)

or more directly (brutalist):
- Resample S onto the grid via bilinear or nearest (fixed mapping table precomputed).
- Apply localized Gaussian (or difference-of-Gaussians) kernel per "feature channel" or directly per pixel/bin:
  I_sensory(x,y) = amp * exp( -((x - x0)^2 + (y - y0)^2) / (2 σ^2) ) * S_resampled(x,y)

Where (x0,y0) is the retinotopic/tonotopic coordinate of the sensory element, σ controls bump width (typically 1-5 grid units).

For audio (1D tonotopic): map frequency bins to 1D or 2D manifold (e.g. log-frequency along x, time or modulation along y).
For visual (2D retinotopic): direct pixel-to-grid mapping with possible foveal warping (higher resolution at center).

Injection occurs every tick (or every few ticks for slower streams) by writing into a dedicated sensory drive buffer, then added in the CANN update step. No lexical embedding; raw continuous values become spatial excitation patterns.

The resulting activity in node_type==1 nodes is their LIF voltage/rate, which then emits spikes into the global network exactly as before.

---

## 3. Attractor Dynamics: Mexican-Hat Connectivity

### 3.1 Lateral Kernel
Stabilize bumps via distance-dependent Mexican-hat connectivity on the manifold (local excitation + surround inhibition). This is implemented as lateral input in the CANN PDE above.

Kernel definition (continuous):
K(Δx, Δy) = A_exc * exp( - (Δx² + Δy²) / (2 σ_exc²) ) - A_inh * exp( - (Δx² + Δy²) / (2 σ_inh²) )

with σ_exc < σ_inh and A_exc, A_inh > 0 chosen so ∫K ≈ 0 (balanced) or slightly positive for weak global drive.

Discrete on grid (precomputed or on-the-fly in shader for small support):
For each grid cell (x,y), lateral input = Σ_{x',y' in support} K(x-x', y-y') * f(u(x',y'))

Support is truncated (e.g. 5-9 grid units radius) for performance. Hexagonal lattice uses 6-neighbor weights with distance-adjusted K.

In the full SNN, this lateral drive is added as an extra term to spike_current for the node_type==1 nodes, or maintained as a separate rate field u that modulates the LIF threshold or injected current of those nodes.

Bump stability: A localized excitation grows into a stable "bump" whose position can drift slowly under external drive (I_sensory) or internal velocity signals (from PFC or cerebellar timing). Noise is rejected by the surround inhibition.

---

## 4. Integration with MPS (Phase 8)

### 4.1 Simplification of Tensor Compression Bounds
The explicit 2D spatial topology introduces strong local correlations: nearby nodes on the manifold have highly correlated activity (bump overlap) and should have correlated effective weights.

In the MPS/MPO of Phase 8:
- Tensorize the node indices respecting the grid geometry (e.g., row-major or space-filling curve ordering of the multi-indices, or separate MPO per local patch).
- Local correlations allow lower bond dimension χ for spatially adjacent sites: the virtual bond χ can be smaller within a bump radius because the effective rank of local sub-blocks of W is low.
- Global long-range connections (to PFC, motor, etc.) retain higher χ or use a hierarchical MPO (local low-χ patches + sparse or higher-rank global MPO).
- Result: overall parameter count or effective χ can be reduced compared to unstructured topology, or the same χ yields higher effective connectivity. The spatial structure provides a natural low-rank prior that the tensor decomposition exploits automatically.

In practice: when building or adapting the MPO cores, initialize or regularize cores for spatially adjacent legs with stronger local structure (smaller singular values across space). During direct core updates, the Mexican-hat lateral drive on the sensory nodes produces correlated STDP events that further reinforce the compressible structure.

The sensory grid can be treated as a "structured input layer" whose MPO sub-tensor has reduced χ by design.

---

## 5. Downstream Routing

### 5.1 Interface to Thalamic 40Hz Gating, PFC, and Astrocytes
The CANN produces a dynamic field with one or more stable moving bumps. Extract low-dimensional spatial summaries once per decision window (post CANN update, before or during burst):

- Bump position(s): centroid(s) of supra-threshold regions (argmax or center-of-mass on u field).
- Bump velocity / direction: finite difference of centroid over recent ticks.
- Total activity / width: ∫ u or number of active grid nodes.
- Phase / coherence: alignment with the 40 Hz thalamic oscillator.

These summaries are written into an extension of PostBurstContext (sensory_bump_pos, sensory_bump_vel, sensory_coherence) and into a compact vector injected as I_bias or goal_modulation.

**Thalamic 40Hz Gating (BCS)**:
- Bump position biases the inhibitory gate variables for downstream populations that are "spatially tuned" (e.g., motor channels corresponding to "attend to left" or "process high-frequency").
- Selective burst_boost for node populations whose receptive fields overlap the current bump.
- The 40 Hz relaxation oscillator can be phase-reset or amplitude-modulated by the bump's instantaneous power, improving BCS when sensory drive is spatially coherent.

**PFC Goal-State**:
- Bump position/velocity provides a continuous "where" signal that is combined with the discrete PFC goal embedding (e.g., via a learned or fixed projection into the PFC attractor input).
- PFC goal can send top-down I_bias to the CANN grid to stabilize or move bumps toward goal-relevant locations (attention).
- When a bump is stable and aligned with a PFC goal, it raises effective BCS or lowers the decision threshold for ExecuteDirect in the Primary Actor.

**Astrocytic Ca²⁺ Sync**:
- The spatial integral or gradient of the bump field acts as an additional source term ξ_sensory in the astrocyte IP3 / Ca²⁺ PDEs (from Phase 6 blueprint).
- High spatial coherence (tight bump) produces stronger localized astrocyte waves, which in turn provide global gain modulation or fatigue to the sensory grid itself and to the tensor cores (η_stdp scaling) of the MPS.
- Topological Betti numbers of the bump field (number of connected components, holes) can be approximated cheaply on the grid and used as in the astrocyte topological meta-plasticity term.

All interfaces are additive or multiplicative scalars/vectors into existing mechanisms. The topological waves do not replace any prior layer; they enrich the input representation with intrinsic spatial dynamics.

---

## 6. WGSL / Data Layouts and Isolated Pseudo-Code

### 6.1 Structural Layouts (WGSL + Rust)
```rust
#[repr(C)]
pub struct GridCoord { x: f32, y: f32 }

#[repr(C)]
pub struct SensoryCANN {
    pub u: [f32; GRID_W * GRID_H],           // activation field
    pub pos: [GridCoord; GRID_W * GRID_H],   // fixed manifold coords (or implicit)
    pub bump_summary: [f32; 8],              // pos_x, pos_y, vel_x, vel_y, width, power, coherence, pad
}

#[repr(C)]
pub struct CANNParams {
    pub tau: f32,
    pub sigma_exc: f32,
    pub sigma_inh: f32,
    pub a_exc: f32,
    pub a_inh: f32,
    pub bump_sigma: f32,
    pub grid_w: u32,
    pub grid_h: u32,
    // ...
}
```

WGSL:
```
struct GridCoord { x: f32, y: f32 };
struct SensoryCANN { u: array<f32, GRID_SIZE>, pos: array<GridCoord, GRID_SIZE>, bump_summary: array<f32, 8> };
struct CANNParams { ... };
@group(0) @binding(N) var<storage, read_write> cann: SensoryCANN;
@group(0) @binding(N+1) var<uniform> cann_params: CANNParams;
```

Grid nodes are a contiguous block of NodeData entries with node_type==1. Their indices map 1:1 to grid linear index.

### 6.2 Isolated Pseudo-Code

```pseudo
// Per-tick or per sub-tick (inside main cortex tick, before global LIF voltage update)
fn update_cann_step(dt: f32, sensory_drive: array<f32>, pfc_mod: f32) {
    for each grid cell g {
        let lateral = mexican_hat_convolution(g, cann.u);  // truncated support, precomputed or local loop
        let drive = sensory_drive[g] + pfc_mod * goal_projection[g];
        cann.u[g] = cann.u[g] * (1 - dt / cann_params.tau) 
                  + dt * (lateral + drive + noise());
    }
    extract_bump_summary();  // center of mass on supra-threshold u, velocity via history
    write_bump_to_postburst_context(cann.bump_summary);
}

// Injection (called from sensory ingest path, e.g. PepeDSP or video frame)
fn inject_sensory_bump(raw_array: array<f32>, mapping: fixed_resample_table) {
    for each grid g {
        let s = resample(raw_array, mapping[g]);
        sensory_drive[g] = amp * gaussian_bump(g.pos, source_pos_of(s), cann_params.bump_sigma) * s;
    }
}

// Mexican-hat kernel (precomputed weights or inline)
fn mexican_hat_convolution(g, u_field) -> f32 {
    var sum = 0.0;
    for neighbor in local_support(g) {
        let d2 = dist2(g.pos, neighbor.pos);
        let k = a_exc * exp(-d2 / (2*sigma_exc*sigma_exc)) 
              - a_inh * exp(-d2 / (2*sigma_inh*sigma_inh));
        sum += k * f(u_field[neighbor]);
    }
    return sum;
}

// Downstream routing (after cann update, before or inside thalamic/BCS)
fn route_topological_waves() {
    let bump = cann.bump_summary;
    // Thalamic bias
    apply_selective_burst_boost_and_gate(bump.pos, bump.power, bump.coherence);
    // PFC
    pfc_state.add_input( project_bump_to_goal_space(bump) );
    // Astrocyte
    let xi_spatial = bump.power * (1.0 + bump.coherence);
    astrocyte_grid.inject_source(xi_spatial, duration = 1);  // or gradient term
    // MPS interaction (Phase 8)
    if spatial_structure_enabled {
        reduce_effective_chi_for_local_sensory_subtensor(bump.width);
    }
}
```

The cann.u field can also directly modulate the LIF voltage or spike_current of the corresponding node_type==1 nodes, closing the loop into the global SNN.

---

## 7. Brutalist Implementation Notes

- Fixed grid size (compile-time or init-time constant in WGSL). No dynamic resizing.
- Sensory grid is a contiguous slice of the overall NodeData array (node_type==1). Their LIF dynamics remain the standard equations; CANN provides additional structured drive and lateral terms.
- Mexican-hat support is small and fixed (e.g. 7×7 kernel); convolution is a simple unrolled loop or precomputed sparse weights per cell.
- All new state lives in SensoryCANN buffer (size O(grid) floats) + tiny params. Integrates with existing NodeData, spike_current, PostBurstContext, SimParams, TensorMPO, PFCState, CerebellarState, astrocyte grid.
- Injection and CANN update run before the main LIF voltage step so that the resulting spikes participate in the current burst window and BCS.
- DreamWorldModel re-uses the identical CANN kernels and bump extraction; raw sensory in dream is synthetic or forward-model predicted.
- MPS interaction is optional/conditional: the grid geometry can be used to order the tensorization of sensory-related legs for better χ or to apply spatially varying rank.
- Safety: while motor lock active, full CANN dynamics and plasticity (if added later) run in Dream; real-world currents from sensory bumps can be attenuated.
- Debug surface: expose u field (downsampled), bump_summary, current I_sensory, lateral contribution, and effect on BCS / thalamic gate.

---

## 8. Cross-References to Prior Blueprints

- NEXUS_QUANTUM_TENSOR_COMPRESSION_BLUEPRINT.md — Explicit 2D manifold reduces effective rank of local sub-blocks in the MPO; local correlations from Mexican-hat bumps allow smaller χ for sensory-related tensor legs. Sensory grid provides structured ordering for tensorization.
- NEXUS_CEREBELLUM_FORWARD_MODEL_BLUEPRINT.md — Bump velocity/position can be used as continuous command for cerebellar forward model; DCN micro-timing corrections can be spatially modulated by bump location.
- NEXUS_PFC_BASAL_GANGLIA_SEQUENCING_BLUEPRINT.md — Bump summaries provide continuous "where" input to PFC attractor and BG state_key. PFC goals send top-down bias to move or stabilize bumps. Sequence actions can be gated by bump alignment.
- NEXUS_ACTION_MAPPING_AND_DREAM_STATE_BLUEPRINT.md — ActionKind / PrimaryAction / MotorActionRegistry / DreamWorldModel / VirtualFeedback. Sensory bumps feed the same PostBurstContext and dream rollouts. PrimaryAction can carry bump state at decision time.
- NEXUS_PRIMARY_ACTOR_BROCA_VALVE_BLUEPRINT.md — Bump coherence and alignment with goal raises effective BCS or biases PrimaryDecision toward ExecuteDirect. High spatial noise (unstable bumps) can increase Verbalization Pressure.
- NEXUS_THALAMIC_GAMMA_GATING_BLUEPRINT.md — burst_boost, SimParams, build_post_burst_context, BCS, thalamic inhibitory gate. Bump position/phase directly biases gate variables and selective boost for the current 5-tick window. Improves participation and phase sync when sensory drive is topologically coherent.
- NEXUS_HIPPOCAMPUS_ENGRAM_BLUEPRINT.md — Phase-resonance retrieval can preload or bias bump positions on the grid for recalled spatial memories. Stable bump trajectories stored as engrams with spatial metadata.
- NEXUS_ASTROCYTE_TOPOLOGICAL_BLUEPRINT.md — Ca²⁺/IP3 PDEs receive ξ_spatial from bump power/gradient. Betti numbers of the bump field (components, holes) feed the topological meta-plasticity term. Astrocytes provide slow gain/fatigue back to the CANN kernel amplitudes and to MPS η.
- NEXUS_SNN_MATHEMATICAL_FOUNDATION_BLUEPRINT.md — Base LIF node dynamics for the sensory subset (node_type==1), 4-hormone modulation (applied to CANN drive and lateral weights), eligibility (sensory spikes carry eligibility as before), CSR layout (MPS still handles long-range; local grid uses explicit Mexican-hat or small dense lateral), homeostasis, global reward × eligibility credit assignment.

All prior mechanisms continue to operate on the spikes and summaries produced by the topological layer. The CANN adds intrinsic spatial dynamics and continuous bump representations on top of the existing discrete LIF substrate.

---

**End of blueprint.**

This completes the formal definition for Phase 9. Antigravity implements the SensoryCANN buffer, the grid coordinate layout, the CANN Euler + Mexican-hat convolution kernels, the bump extraction and summary writing, the sensory array resampling/injection, the routing functions into thalamic/PFC/astrocyte/MPS paths, the SimParams + CANNParams extensions, and the integration with existing NodeData (node_type==1) and spike_current. The layer is additive and re-uses every prior data path and invariant exactly.

No Helheim core files and no previously implemented NEXUS subsystems are altered. The full topological dynamics, bump formation, and downstream routing can be exercised and validated at scale inside DreamWorldModel (with synthetic or forward-predicted raw sensory) before any real high-bandwidth sensory ingest or unlocked motor execution. The explicit spatial structure directly benefits the Phase 8 MPS compression.