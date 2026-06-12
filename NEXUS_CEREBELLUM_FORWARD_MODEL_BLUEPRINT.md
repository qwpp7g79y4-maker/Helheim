# NEXUS_CEREBELLUM_FORWARD_MODEL_BLUEPRINT.md

**Phase 7: Cerebellar Forward Model & Micro-Timing Circuit**

Current state (subsystems 1-6 implemented and verified):
- Primary Actor + Broca Valve
- Thalamic Gating + real BCS (participation + 40 Hz phase sync on motor nodes)
- Dream State Sandbox + R_v via DreamWorldModel
- Hippocampus phase-resonance engram storage/retrieval
- Astrocyte Ca²⁺/IP3 glial sync for meta-plasticity + fatigue
- Prefrontal-Basal Ganglia Loop (PFC-BG): sustained goals + TD-driven ActionKind sequencing

Deficit: BG selects the sequence of "what" (ActionKind), but execution remains coarse. No learned micro-timing of spikes within/ across burst windows. No internal forward prediction of immediate sensory consequence S_{t+1}. No fast error-driven correction that bypasses slow cortical/PFC-BG loops. Timing and precision still rely on reactive SNN + slow eligibility.

This blueprint defines the Cerebellar extension only. Mathematical formalisms, GPU-friendly data models, and isolated pseudo-code. No existing source is modified. All mechanisms integrate with the existing sparse CSR SNN, dual-kernel layout, SimParams, PostBurstContext, PrimaryAction, DreamWorldModel, thalamic gate, and PFC-BG disinhibition.

---

## 1. Architecture: Purkinje + Granule Topology for Micro-Timing

### 1.1 Requirements
- BG/PFC emit intended ActionKind + coarse sequence step.
- Cerebellum receives efference copy (intended action, PFC goal embedding, current motor/sensory summary, previous timing).
- Granule layer performs sparse high-dimensional expansion of the command + state for pattern separation.
- Parallel fibers (granule axons) carry timed activity to Purkinje cells.
- Purkinje population learns a forward model that outputs predicted S_{t+1} and the precise spike timing offsets required for optimal execution of that action under the current goal/context.
- Micro-timing operates at sub-burst-window resolution (phase within the 40 Hz cycle or inter-tick precision via delay kernels).
- Output via Deep Cerebellar Nuclei (DCN) provides fast corrective modulation directly to thalamic relays and/or motor spike currents, bypassing PFC-BG decision latency.

### 1.2 Bare-Metal Topology
- Granule cells: fixed high-dimensional layer (N_granule = 4096–16384). Sparse activation (few active per command). Each granule receives convergent input from: current sensory nodes, motor command embedding (from BG-selected canonical ActionKind), PFC goal vector, and a bank of delay-line taps for timing.
- Parallel fibers: effective connectivity via a sparse weight matrix PF_to_Purkinje (or CSR-like for WGSL). Different fibers carry different delay offsets (explicit tapped delay line for micro-timing).
- Purkinje cells: smaller population (N_pkj = 128–512). Each Purkinje learns a local forward model component (prediction of a sensory dimension or a timing kernel for a specific sub-action). Simple rate or voltage dynamics.
- Climbing fibers: one per Purkinje (or small groups). Driven by Inferior Olive (IO) error computation. Global or per-channel broadcast of error in practice for bare-metal.
- Deep Cerebellar Nuclei (DCN): small output layer (size matched to motor channels or ActionKind cardinality + timing dims). Receives inhibition from Purkinje. Tonic activity disinhibited when Purkinje is suppressed by LTD. DCN directly emits correction signals (phase shift, amplitude offset, spike time delta) applied to the thalamic gate or motor current injection in the main SNN shader.
- Time scale: Cerebellar pass runs at main tick rate or with a fast sub-tick loop (2-4x) for timing precision. Delay lines are fixed-length circular buffers.

No full biological mossy fiber / basket cell complexity. Purkinje + granule + CF + DCN is the minimal sufficient circuit.

---

## 2. Forward Model: Predicting S_{t+1} and Micro-Timing

### 2.1 Mathematical Formalism

State at decision point t (post-burst or intra-burst):
- s_t : compressed sensory + motor summary vector (from SNN readback or dedicated summary nodes, dim D_s ~ 32-128).
- g : PFC goal embedding (from PFCState).
- a : canonical ActionKind index (from BG selection + MotorActionRegistry).
- τ_prev : previous timing vector (phase offsets or spike time deltas from last DCN output).

Granule expansion (sparse, fixed random projection + delay taps):
Let φ(s, g, a, τ, delays) be the granule activation vector (high-dim, sparse). Delays are explicit taps: granule bank k receives input delayed by k * δt (δt = 1 tick or sub-tick).

Purkinje forward model (per cell p, or population readout):
The Purkinje population activity p encodes both the predicted sensory consequence and the timing program.

Discrete Euler (per cerebellar sub-tick or main tick):

τ_pk dV_p / dt = -V_p + Σ_k w_{p,k} * φ_k(t - d_k) + I_tonic + I_cf(t)

V_p → predicted component: for sensory prediction head, a linear readout or the V_p itself (after training) represents Ŝ_{t+1, p}.

For micro-timing: each Purkinje (or group) also maintains a short timing kernel or directly influences a per-channel timing offset θ_a (phase or delta ticks for the motor spikes of action a).

Forward prediction:
Ŝ_{t+1} = W_pred * V_pkj   (or direct population average / selected Purkinje)
θ_timing = W_time * V_pkj + bias(a, g)   // precise offsets for spike current injection or gate modulation

The model is "forward" because it is driven by the efference copy (intended a + current s + g) before the real (or simulated) consequence arrives.

In DreamWorldModel the same forward model can be queried to advance an internal predicted state for faster rollouts or to generate synthetic R_v when full simulation is expensive.

### 2.2 Data Model (Rust + WGSL)

```rust
pub const N_GRANULE: usize = 8192;
pub const N_PURKINJE: usize = 256;
pub const N_DCN: usize = 64;                 // sized to motor channels + timing dims
pub const DELAY_TAPS: usize = 8;             // micro-timing resolution

#[repr(C)]
pub struct CerebellarState {
    // Granule activations (sparse in practice; full array for WGSL simplicity)
    pub granule: [f32; N_GRANULE],
    pub granule_delay_buffers: [[f32; DELAY_TAPS]; 32], // small convergent inputs delayed

    // Purkinje
    pub purkinje_v: [f32; N_PURKINJE],       // membrane or rate
    pub purkinje_w_pf: [[f32; N_GRANULE]; N_PURKINJE], // or CSR for memory; full for simplicity if N small

    // Forward model outputs (read after cerebellar pass)
    pub predicted_s: [f32; 64],              // Ŝ_{t+1} (compressed sensory dims)
    pub timing_offsets: [f32; 32],           // per relevant motor channel or action: phase/delta in ticks

    // DCN
    pub dcn_rate: [f32; N_DCN],
    pub correction: [f32; N_DCN],            // final output: disinhibit delta + timing mod

    pub last_error_norm: f32,
    pub last_cf_burst: u32,
}

#[repr(C)]
pub struct CerebellarParams {
    pub tau_pk: f32,
    pub ltd_rate: f32,
    pub cf_gain: f32,
    pub dcn_tonic: f32,
    pub timing_scale: f32,
    pub delay_dt: f32,
    // ...
}
```

WGSL layout (add to cortex_compute or new cerebellar_compute.wgsl):
```
struct CerebellarBuffer { ... same as above ... };
@group(0) @binding(M) var<storage, read_write> cb: CerebellarBuffer;
@group(0) @binding(M+1) var<uniform> cb_params: CerebellarParams;
```

Granule input sources (efference copy): packed from PostBurstContext motor labels (canonical indices), PFCState embedding, current sensory summary, previous correction.

---

## 3. Error Correction: Inferior Olive + Climbing Fibers + LTD

### 3.1 Mathematical Formalism

After action execution (real or in DreamWorldModel step):
actual_s = sensory_summary from next PostBurstContext or DreamWorldModel observation (or full SNN readback).

Error:
e = actual_s - predicted_s   // vector in sensory space
error_norm = ||e||_2  or weighted (e.g., motor-relevant dimensions higher weight)
cf_signal_p = cf_gain * g(error_norm, e_p)   // scalar or per-Purkinje projection. g can be |e| or ReLU(e · direction_p)

Climbing fiber drives Purkinje (simplified; in biology complex spike):
During the error window (immediate post-consequence tick or after dream step):
I_cf_p(t) = cf_signal_p   // added to Purkinje ODE for one or few steps

LTD on parallel-fiber → Purkinje synapses (core supervised plasticity; anti-Hebbian under error):

For each active parallel fiber k to Purkinje p, within a short eligibility window (the delay taps provide the temporal credit):
Δw_{p,k} = - η_ltd * cf_signal_p * φ_k(t - d) * eligibility(Δt)

Where eligibility(Δt) = exp(-|Δt|/τ_el) for timing precision (higher LTD when parallel fiber spike and error are temporally aligned).

This is the Marr-Albus-Ito supervised LTD. No LTP in the minimal model (or very slow homeostatic LTP to prevent total silencing).

The effect: repeated errors for a given command + state pattern depress the PF weights that were active during the wrong timing, causing Purkinje V_p to drop for that pattern. Lower Purkinje → higher DCN → corrective output that adjusts future timing so the executed spikes produce better S_{t+1} (lower future error).

In discrete post-event update (CPU or post-GPU readback for stability):
```
for p in 0..N_PURKINJE {
    let cf = compute_cf(p, e);
    for k in active_granules_for_this_command {
        let trace = delay_line_trace[p][k];  // recent φ at different delays
        cb.purkinje_w_pf[p][k] -= cb_params.ltd_rate * cf * trace;
        cb.purkinje_w_pf[p][k] = clamp(cb.purkinje_w_pf[p][k], w_min, w_max);
    }
}
```

### 3.2 Integration with DreamWorldModel for Safe Training
Inside dream_sequence_rollout (from Phase 6):
- BG emits next PrimaryAction.
- Cerebellum receives efference copy, runs forward_predict → produces Ŝ and timing_offsets.
- DreamWorldModel applies the action (with the cerebellar timing_offsets modulating the virtual motor execution if supported).
- After virtual step, actual simulated sensory is observed.
- IO error computed on (actual_sim - predicted).
- LTD applied immediately in the dream episode (or batched).
- This allows the forward model + timing to be refined thousands of times safely without host side effects.
- Virtual R_v can be augmented by -λ * error_norm (prediction accuracy itself becomes part of the reward for the BG policy).

---

## 4. DCN Output: Fast Corrective Modulation to Thalamus / Motor

### 4.1 Formalism

Purkinje inhibition to DCN:
dcn_p = dcn_tonic - Σ (inhibition_from_purkinje_p)   // or max(0, tonic - purkinje_rate_p)

DCN population (or per-channel) produces correction:
correction_c = tanh( dcn_c * gain ) * timing_scale

Application points (direct bypass of slow loops):
1. Thalamic gate modulation (extends Phase 2/6 disinhibition):
   effective_inhibition[channel] -= correction_c   // or specific timing channel
   This allows the next burst to have precisely shifted participation or phase for the motor nodes of the current ActionKind.

2. Direct motor spike current or phase injection (in main LIF shader, inside the decision burst window):
   for motor nodes belonging to the selected action:
       spike_current += correction_c * phase_factor(last_spike_tick, target_phase)
       // or adjust the relaxation oscillator phase of the thalamic gate for that subpopulation

3. Timing offset to the burst window itself:
   The 5-tick decision burst start or the gamma oscillator phase for goal-relevant motor nodes is advanced/delayed by the DCN timing_offsets.

Because DCN acts on the existing thalamic/motor machinery at high temporal resolution (within the burst or between ticks), it provides micro-adjustments that the PFC-BG (which operates at sequence-step / burst granularity) cannot.

DCN corrections are also written back into the efference copy for the next cerebellar input, closing the fast loop.

### 4.2 Pseudo-Code for Core Loop

```pseudo
// After BG selection of next ActionKind a, before or during burst computation
cerebellar_forward_pass(efference_copy = {s_t, g_from_pfc, a, prev_timing}) {
    // 1. Granule expansion with delays
    phi = expand_granule(s_t, g, a, prev_timing, delay_taps);
    update_delay_buffers(phi);

    // 2. Purkinje dynamics + forward prediction
    for p {
        rec = dot(cb.purkinje_w_pf[p], phi);
        cb.purkinje_v[p] = cb.purkinje_v[p] * (1 - dt/tau_pk) + dt * (rec + tonic);
    }
    cb.predicted_s = W_pred * cb.purkinje_v;
    cb.timing_offsets = W_time * cb.purkinje_v;

    // 3. DCN
    for c {
        inhib = sum_purkinje_inhibition_to_dcn(c);
        cb.dcn_rate[c] = cb_params.dcn_tonic - inhib;
        cb.correction[c] = tanh(cb.dcn_rate[c]) * cb_params.timing_scale;
    }
}

// Apply correction (fast path, inside thalamic/motor shader or post-burst prep)
apply_cerebellar_correction(correction, &mut thalamic_inhibition, &mut motor_spike_currents, burst_phase) {
    for relevant motor channel c {
        thalamic_inhibition[c] -= correction[c] * 0.5;   // disinhibit boost
        motor_spike_currents[c] += correction[c] * phase_kernel(burst_phase, target_from_timing_offsets);
    }
}

// Post-consequence (after real step or dream step, using next PostBurstContext or dream obs)
error_correction_step(actual_s, predicted_s) {
    e = actual_s - predicted_s;
    cb.last_error_norm = norm(e);

    for p {
        cf = cb_params.cf_gain * relu(e · dir_p + error_norm);  // or simpler norm-based
        apply_ltd_to_parallel_fibers(p, cf, recent_granule_traces);
    }
    // Optional: small eligibility credit from cerebellar error to higher loops if desired
}
```

---

## 5. Full Integration Points

- PFC-BG (Phase 6): BG's selected ActionKind + state_key provides the command to cerebellum. PFC goal embedding is part of the efference copy. DCN corrections can trigger PFC "progress" refresh or slight energy boost when error_norm is decreasing.
- Thalamic Gating + BCS (Phase 2): DCN correction directly alters the inhibitory gate variables and/or selective burst_boost for the current motor subpopulation. This refines the spikes that will be used for the BCS calculation and the PrimaryActor decision. The micro-timing improves the quality of the motor nodes that participate in the coherent burst.
- DreamWorldModel + VirtualFeedback (Phase 3/6): Forward model is queried inside dream rollouts. Predicted S can be used for internal R_v shaping (-prediction_error term). After each virtual action application, error_correction_step is called with the simulated actual. LTD happens entirely inside the sandbox. This is the primary training regime while safety lock is active.
- PrimaryAction: The timing_offsets and correction can be attached to PrimaryAction (new fields: cerebellar_timing, dcn_correction_snapshot) so that origin_burst_id traces the precise execution parameters for later credit assignment or sequence engram storage.
- Hippocampus: Successful low-error executions (low final error_norm after DCN corrections) can be stored with the cerebellar weights snapshot or as "expert timing" engrams. Retrieval can preload Purkinje weights or seed initial timing_offsets for a known sequence.
- Astrocytes: Slow Ca²⁺ waves can globally scale cerebellar ltd_rate (fatigue reduces fine learning) or dcn_tonic (global gain on corrections).
- Main SNN LIF + SimParams: Add cerebellar sub-pass. The granule/Purkinje/DCN can be additional node groups or completely separate buffers (recommended for performance: cerebellar is not full LIF, it is a specialized forward + supervised module). Corrections are injected into the existing spike_current and gate math before the voltage integration step.
- PostBurstContext: Extend with cerebellar fields (predicted_s used, actual_error_norm, dcn_corrections_applied) so that PrimaryActor and BG can see the quality of micro-execution.
- ActionKind registry: Cerebellum is indexed by the same canonical action keys. Per-action or per-action+goal specialized Purkinje subpopulations.

Orchestration sketch (extension of Phase 6 pseudo):

```pseudo
pfc.maintain_tick(...);
bg.prepare(...);
disinhibit = bg.compute_disinhibition(...);

cerebellar_forward_pass( build_efference(pfc, bg.last_action, sensory, disinhibit) );
apply_cerebellar_correction(cb.correction, &mut thalamic_inhibition, &mut motor_currents, current_phase);

let decision = primary_actor.decide_after_burst(&post_burst_ctx_with_cb);

if ExecuteDirect {
    // motor nodes now carry the refined timing from DCN
    if dream { 
        vf = dream.execute_with_cb_timing(...);
        error_correction_step(vf.simulated_sensory, cb.predicted_s);  // LTD inside dream
        // also do BG TD update as before
    } else {
        real_execute();
        // later, when sensory feedback arrives: error_correction_step(actual, predicted)
    }
}
```

---

## 6. WGSL / SimParams Extensions

Add to SimParams:
```
cerebellar_tau_pk: f32,
cerebellar_ltd_rate: f32,
cerebellar_cf_gain: f32,
cerebellar_dcn_tonic: f32,
cerebellar_timing_scale: f32,
cerebellar_delay_dt: f32,
```

New buffers (separate from main NodeData/CSR for clarity and performance):
- CerebellarBuffer (granule, purkinje_v, w_pf (sparse or dense small), predicted, timing, dcn, correction)
- CerebellarParams
- Optional: small efference summary buffer written from main SNN/PFC/BG after each decision.

Cerebellar pass can be a distinct compute shader or a set of functions called from the main cortex tick (after BG disinhibit computation, before or inside the burst voltage update).

For memory: if N_GRANULE * N_PURKINJE is too large, store w_pf in CSR format (row = Purkinje, columns = active granules per command) or use a factored/low-rank approximation per action.

---

## 7. Brutalist Implementation Notes

- Fixed-size arrays only. Granule layer can be treated as dense for WGSL simplicity or maintained as per-command sparse masks.
- Plasticity (LTD) happens after consequence (real sensory readback or dream observation), never inside the inner LIF voltage loop. Use traces/delay buffers for the temporal window.
- Forward prediction and DCN correction are fast feed-forward paths (O(N_pkj * active_granule) per step). Keep active_granule small.
- All corrections are additive deltas to existing mechanisms (thalamic inhibition, spike_current). No replacement of BG/PFC/thalamic logic.
- Error signals and LTD use the same compressed sensory summaries already present for PostBurstContext and DreamWorldModel.
- While safety lock active: only DreamWorldModel + forward model + LTD are exercised. Real DCN corrections can be logged but not applied to physical motor until user explicitly lifts the lock for that channel.
- Goal/action specific specialization: Purkinje can be partitioned or use a goal/action embedding to gate which subset of weights is active (avoids catastrophic interference across different sequences).
- Credit assignment: cerebellar error_norm can be used as an auxiliary signal to scale the R_v that reaches the BG TD update (better timing → higher effective reward for the sequence policy).
- Debug: expose predicted vs actual, current cf_signal, average |Δw| from last LTD, DCN correction vector, and the timing_offsets that were applied to the last burst.

---

## 8. Cross-References to Prior Blueprints

- NEXUS_PFC_BASAL_GANGLIA_SEQUENCING_BLUEPRINT.md — BG supplies the ActionKind sequence steps and state_keys; PFC goal embedding is part of cerebellar efference copy; DCN corrections can influence PFC progress/refresh. Sequence rollouts in DreamWorldModel now augmented by cerebellar forward model + internal LTD.
- NEXUS_ACTION_MAPPING_AND_DREAM_STATE_BLUEPRINT.md — ActionKind / PrimaryAction / MotorActionRegistry / DreamWorldModel / VirtualFeedback. Cerebellum keys off the same canonical indices. Timing corrections and predicted_s can be attached to PrimaryAction. Dream rollouts are the training arena for the forward model.
- NEXUS_PRIMARY_ACTOR_BROCA_VALVE_BLUEPRINT.md — PrimaryDecision and ExecuteDirect path now see refined motor participation thanks to DCN. Cerebellar error can bias Verbalization Pressure (high error → more likely to request verbalization or suppress).
- NEXUS_THALAMIC_GAMMA_GATING_BLUEPRINT.md — burst_boost, SimParams, build_post_burst_context, BCS, thalamic inhibitory gate. DCN correction is injected directly into the gate variables and motor spike currents inside the 5-tick decision burst window. Improves the quality of spikes used for real BCS computation.
- NEXUS_HIPPOCAMPUS_ENGRAM_BLUEPRINT.md — Phase-resonance retrieval can preload cerebellar timing programs or Purkinje weight snapshots for known low-error sequences. Successful micro-timed executions stored with cerebellar metadata.
- NEXUS_ASTROCYTE_TOPOLOGICAL_BLUEPRINT.md — Ca²⁺/IP3 waves and Betti numbers globally modulate cerebellar ltd_rate and dcn gain (fatigue and complexity cost on fine timing).
- NEXUS_SNN_MATHEMATICAL_FOUNDATION_BLUEPRINT.md — Base LIF, 4-hormone modulation (slow dopamine as gain on cerebellar rates too), eligibility (cerebellar uses supervised LTD variant; eligibility traces appear as the delay taps), CSR layout (cerebellar PF weights can reuse sparse representation).

All new machinery reuses the same credit paths, engram storage, astrocyte modulation, PrimaryAction tracing (via origin_burst_id), and thalamic/motor substrate. The cerebellum adds the fast, supervised, timing-precise forward model and bypass correction layer on top of the slower PFC-BG actor.

---

**End of blueprint.**

This completes the formal definition for Phase 7. Antigravity implements the cerebellar buffers and passes (granule expansion, Purkinje Euler + forward readouts, IO error + LTD, DCN), the efference copy construction, the injection points into thalamic gate and motor currents, the integration inside DreamWorldModel rollouts, and the SimParams/buffer wiring. No changes to prior 1-6 subsystems or any Helheim core logic. The safety lock remains fully respected.

The forward model + LTD loop can be trained to extreme precision entirely inside the isolated sandbox before any real motor execution is ever permitted.