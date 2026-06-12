# NEXUS_PFC_BASAL_GANGLIA_SEQUENCING_BLUEPRINT.md

**Phase 6: Prefrontal-Basal Ganglia Loop (PFC-BG)**

Current state (subsystems 1-5 implemented and verified):
- Primary Actor + Broca Valve
- Thalamic Gating + real BCS (participation + 40 Hz phase sync on motor nodes)
- Dream State Sandbox + R_v via DreamWorldModel
- Hippocampus phase-resonance engram storage/retrieval
- Astrocyte Ca²⁺/IP3 glial sync for meta-plasticity + fatigue

Deficit: purely reactive single-burst policy. No persistent goal representation across post-burst leak. No learned chaining of ActionKind steps. Falls back to LLM verbalization.

This blueprint defines the PFC-BG extension only. Mathematical formalisms, GPU-friendly data models, and isolated pseudo-code. No existing source is modified.

---

## 1. Sustained Working Memory — Prefrontal Cortex (PFC)

### 1.1 Requirements
- Maintain a goal representation (discrete ID or low-dimensional embedding) for 10³–10⁴ GPU ticks after the initiating burst.
- Resist exponential leak and post-burst global reset.
- Bias future motor populations and BG selection toward the goal without requiring continuous high-rate spiking in the reactive layers.
- Interface with Hippocampus for goal seeding and with DreamWorldModel for offline sequence rehearsal.

### 1.2 Topology (bare-metal choice)
Discrete goal embedding + recurrent line attractor on a small fixed-capacity PFC population (N_pfc ≈ 128–512 units). 

Not a full dense cortical sheet. Capacity-limited, explicit, indexable goals. Recurrent weights form a set of stable fixed-point attractors, one per registered goal.

Alternative (higher frontier): continuous ring/line attractor for metric goal spaces. Start with discrete for immediate implementability.

### 1.3 Mathematical Formalism (PFC Attractor Dynamics)

Continuous-time (Euler discretised per GPU tick):

τ_pfc * du_i / dt = -u_i + Σ_j W_rec[i,j] * σ(u_j) + I_goal_i + I_bias_i - λ_leak * u_i + η_noise

where:
- u ∈ ℝ^{N_pfc} : PFC unit activations (maintained state)
- W_rec : N_pfc × N_pfc recurrent matrix, structured as sum of outer products for each goal prototype (W_rec = Σ_g α_g * v_g v_g^T + local excitation/inhibition for stability)
- σ(x) = tanh(β x) or ReLU-like (WGSL friendly)
- I_goal : projection of the current goal embedding e_g onto the population (I_goal = M * e_g)
- λ_leak : small (0.0001–0.001) to allow slow decay only when goal is abandoned
- τ_pfc : 200–2000 ticks (slow time constant)

To survive post-burst:
- The PFC population is **not** subject to the same strong voltage leak + refractory as node_type 0-3.
- After each burst window, PFC receives a maintenance current proportional to its own current attractor energy E = ||u||^2 / N.
- Periodic low-amplitude refresh pulses aligned to gamma when BCS > θ and goal_match > μ (see integration section).

Attractor energy (scalar goal confidence):
E_pfc(t) = (1/N) Σ u_i(t)^2
Goal match for a candidate action a: m(g, a) = cosine( e_g , readout(a) ) or dot product after learned projection.

Discrete update (WGSL shader):
```
u_new[i] = u[i] * (1.0 - dt/tau_pfc) 
         + dt * (recurrent_sum[i] + I_goal[i] + bias[i] - leak_coeff * u[i])
u[i] = clamp(u_new[i], -u_max, u_max)
```

### 1.4 Data Model (Rust + WGSL layout)

```rust
pub const PFC_DIM: usize = 256;
pub const MAX_GOALS: usize = 64;

#[repr(C)]
pub struct PFCGoal {
    pub id: u64,                    // stable goal identifier (from engram or initial injection)
    pub embedding: [f32; 32],       // low-dim goal vector (cosine friendly)
    pub prototype_idx: u32,         // which attractor basin
    pub creation_tick: u32,
}

#[repr(C)]
pub struct PFCState {
    pub current_goal: Option<PFCGoal>,
    pub u: [f32; PFC_DIM],          // persistent activations (WGSL buffer)
    pub energy: f32,
    pub sustain_ticks: u32,         // countdown; 0 = no active goal
    pub last_refresh_tick: u32,
    pub goal_match_bias: f32,       // cached scalar for motor/BG biasing
}

impl PFCState {
    pub fn decay(&mut self, dt: f32, leak: f32) {
        // called every tick or post-burst
        for v in &mut self.u {
            *v *= (1.0 - dt * leak);
        }
        self.energy = self.u.iter().map(|x| x*x).sum::<f32>() / (PFC_DIM as f32);
    }
}
```

WGSL side (cortex_compute.wgsl extension):
```
struct PFCBuffer {
    u: array<f32, 256>,
    energy: f32,
    sustain_ticks: u32,
    // padding
};
@group(0) @binding(N) var<storage, read_write> pfc: PFCBuffer;
@group(0) @binding(N+1) var<uniform> pfc_params: PFCParams;  // tau, leak, refresh_strength, ...
```

Recurrent weights live in a compact W_rec buffer (N_pfc * N_pfc or factored low-rank for memory).

---

## 2. Action Sequencing — Basal Ganglia (BG) + Phasic Dopamine

### 2.1 Requirements
- Learn policies that output ordered sequences of ActionKind (via the existing MotorActionRegistry) to drive the world state toward the PFC goal.
- Use fast, event-driven dopamine transients (distinct from the slow 4-hormone dopamine gain).
- Support multi-step credit assignment inside DreamWorldModel without real execution.
- Terminate or switch sequences cleanly.

### 2.2 RPE / TD Formalism

Standard TD(λ) with eligibility traces, computed on discrete sequence steps (not every LIF tick).

After each executed (real or virtual) step t:
```
s_t     = (current_sensory_summary, pfc_goal_embedding, previous_action_canonical)
a_t     = selected canonical ActionKind
r_t     = R_v (from DreamWorldModel) or real outcome reward
s_{t+1} = next state after action

δ_t = r_t + γ * V(s_{t+1}, g) - V(s_t, g)     // RPE
```

Value function (bare-metal):
V(s, g) ≈ w_v · φ(s, g)     // linear readout or small table lookup
Q(s, g, a) ≈ w_q · φ(s, g, a)   // or direct table since |A| small (registry size)

Phasic dopamine transient (fast ODE, updated only on sequence steps or post-dream rollout):

d[DA]/dt = -DA / τ_da + κ * tanh(δ / scale) + tonic_baseline

Discrete (per sequence event):
```
da_phasic = da_phasic * (1.0 - alpha_da) + kappa * clip(delta, -2.0, 2.0)
```

DA modulates:
- Learning rate of BG weights: η_bg *= (1.0 + β * da_phasic)
- Disinhibition strength (see section 3)
- Brief boost to PFC maintenance current (DA as "attention to progress" signal)

BG also receives the slow hormone dopamine as gain on all Q-values (multiplicative).

### 2.3 Sequence Representation

A sequence is an ordered list of PrimaryAction (or canonical indices) emitted by BG while a single PFC goal is active.

BG does not hold the full plan explicitly. It selects the next action at each decision point using current PFC state + sensory summary + previous step.

For long horizons the DreamWorldModel performs the rollouts and the TD errors are batched back into the BG weights after the dream episode.

### 2.4 Data Model

```rust
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub struct StateKey {
    pub goal_id: u64,
    pub context_hash: u64,          // hash of recent motor/sensory + astrocyte summary
    pub prev_action: u16,           // canonical index into registry
}

pub struct BGWeights {
    // For bare-metal: fixed-size or hashmap with bounded capacity.
    // In WGSL: two arrays (keys + values) or a small dense matrix over known actions.
    pub q: HashMap<StateKey, [f32; MAX_REGISTRY_ACTIONS]>,  // Q per action
    pub value: HashMap<StateKey, f32>,
    pub eligibility: HashMap<StateKey, f32>,                // trace
}

pub struct DopamineState {
    pub phasic_da: f32,
    pub tonic_da: f32,                // slow hormone copy
    pub last_rpe: f32,
    pub last_event_tick: u32,
}

pub struct SequenceTracker {
    pub active_goal: Option<u64>,
    pub step_index: u32,
    pub accumulated_rv: f32,
    pub actions_taken: Vec<PrimaryAction>,  // for credit + engram storage
}
```

---

## 3. Gating Mechanism — BG Disinhibition of Thalamus

Existing Thalamus already implements inhibitory gating + relaxation oscillator for 40 Hz bursts (BCS computation in build_post_burst_context).

BG adds a **disinhibitory channel** per registered ActionKind (or per motor population group).

### 3.1 Model

GPi-analog output (BG) provides tonic inhibition to specific thalamic relay populations:
```
thalamic_inhibition[channel_a] = gpi_tonic 
    - disinhibit_strength * (Q(s,g,a) - threshold) * (E_pfc > θ_energy) * (1.0 + da_phasic)
```

When disinhibition for channel_a exceeds a threshold, the corresponding motor nodes (node_type==3) that map to ActionKind a receive:
- Reduced effective inhibitory gate variable during the next decision burst window.
- Selective increase in burst_boost for those nodes (goal-directed boost on top of global 2.8).

Result: the next coherent burst is biased to produce motor spikes whose labels resolve (via MotorActionRegistry) to the BG-chosen next ActionKind.

Disinhibition is transient (one burst window) and must be re-earned by BG selection on the subsequent decision point.

### 3.2 Pseudo integration with existing gate

In the thalamic gating shader / build_post_burst_context path:
```
for each motor channel {
    effective_gate = base_inhibition[channel] - bg_disinhibit[channel]
    spike_current += ... * (1.0 - effective_gate) * goal_bias[channel]
}
```

Only when PFC energy high + BG has selected a step for the active goal is bg_disinhibit non-zero for the chosen channel(s).

---

## 4. Integration with Existing Subsystems

### 4.1 With Burst Coherence Score (BCS)
- PFC energy and goal_match act as multiplicative bias inside the BCS calculation or the post-burst motor participation scoring.
- High PFC energy + high goal-action alignment lowers the BCS threshold for ExecuteDirect (Primary Actor becomes more willing to act when it has a plan).
- Sequence steps are only advanced on bursts that achieve sufficient BCS under the current goal bias.

### 4.2 With DreamWorldModel (Rv generation for sequences)
Extension of Fase 3 sandbox:

```
fn dream_sequence_rollout(pfc: &PFCState, bg: &mut BGWeights, dream: &mut DreamWorldModel, steps: u32) -> Vec<VirtualFeedback> {
    let mut trajectory = vec![];
    let mut seq = SequenceTracker { active_goal: pfc.current_goal.as_ref().map(|g| g.id), .. };
    
    for _ in 0..steps {
        if pfc.energy < PFC_ENERGY_MIN { break; }
        
        let state_key = build_state_key(pfc, dream.current_observation(), seq.last_action);
        let a = bg.select_action(state_key, temperature);           // argmax or softmax over Q
        let primary_action = registry.resolve_canonical(a).into_primary_action(pfc.current_goal);
        
        let vf = dream.execute_virtual(&primary_action, ...);       // existing DreamWorldModel
        trajectory.push(vf);
        
        let delta = compute_td_error(vf.rv, &state_key, next_state_key, bg);
        bg.update_with_trace(state_key, a, delta, da_phasic);
        da_phasic = update_phasic_da(da_phasic, delta);
        
        seq.actions_taken.push(primary_action);
        seq.accumulated_rv += vf.rv;
    }
    
    // batch credit + optional sequence engram write
    hippocampus.store_sequence_engram(pfc.current_goal, seq.actions_taken, seq.accumulated_rv);
    trajectory
}
```

Dream rollouts are the primary training signal while real motor is locked.

### 4.3 With Hippocampus (phase-resonance engrams)
- Successful sequences (high cumulative Rv, goal reached in dream or reality) are stored as macro-engrams containing the ordered list of ActionKind + the originating PFC goal embedding.
- On goal injection (from Broca, external, or internal drive): hippocampus performs resonance retrieval. The best-matching past sequence seeds:
  - PFC attractor initial condition (preload first step's motor bias)
  - BG Q-value warm-start for the early steps of the sequence
- Retrieval is phase-locked to the gamma burst window (consistent with existing engram mechanism).

### 4.4 With Astrocyte Glial Sync
- Astrocyte Ca²⁺ waves (slow) modulate:
  - PFC recurrent gain (fatigue reduces W_rec strength → forces goal abandonment after too long without progress)
  - BG learning rate (high astrocyte IP3 → lower η_bg, prevents over-learning during high metabolic load)
- Topological Betti numbers (from prior astrocyte blueprint) can increase the "cost" of sustaining a PFC goal if global complexity is already high.

### 4.5 With Primary Actor / Broca Valve
- When PFC energy > θ and BG has a high-Q next step ready, PrimaryDecision::ExecuteDirect is strongly preferred over RequestVerbalization.
- Verbalization Pressure (VP) formula gains a negative term proportional to PFC energy (the system prefers to act on its plan rather than talk).
- If sequence stalls (multiple low-Rv steps or PFC energy collapse), Broca is allowed to verbalize status or request clarification.

### 4.6 With ActionKind Registry (from Fase 3 blueprint)
BG and PFC operate exclusively over canonical action indices produced by the MotorActionRegistry. Dream* variants are used inside DreamWorldModel; real variants only when safety lock is lifted and disinhibition actually reaches the real motor path.

---

## 5. Orchestration (Isolated Pseudo-Code)

Main decision loop extension (post build_post_burst_context):

```pseudo
pfc.maintain_tick(dt, current_hormones);           // always runs

if let Some(goal) = pfc.current_goal {
    bg.prepare_for_goal(goal);
    
    let state_key = build_state_key(pfc, sensory_summary, last_action);
    let disinhibit_map = bg.compute_disinhibition(state_key, pfc.energy, da_phasic);
    
    // feed into existing thalamic gate
    apply_bg_disinhibition_to_thalamus(disinhibit_map);
}

let decision = primary_actor.decide_after_burst(&post_burst_ctx, pfc.energy, bg.confidence);

match decision {
    PrimaryDecision::ExecuteDirect => {
        let next_action = bg.select_and_commit(state_key);   // or from disinhibited motor labels
        if in_dream_mode || safety_lock {
            let vfs = dream_sequence_rollout(...);           // may execute 1 or N steps
            apply_virtual_feedback_batch(vfs);
        } else {
            real_executor.execute(next_action);
        }
        pfc.maybe_refresh_on_progress(next_action, last_rv);
    }
    PrimaryDecision::RequestVerbalization => { ... }
    PrimaryDecision::Suppress => { pfc.decay_extra(); }
}

if pfc.energy < ABANDON_THRESHOLD {
    pfc.clear_goal();
    bg.clear_sequence();
    // optional: store partial sequence as negative engram
}
```

PFC goal injection entry points (any of):
- Hippocampus resonant retrieval on high-certainty internal drive
- Explicit motor node that means "set new goal X" (during burst)
- External command (debug / user) that writes directly into PFC buffer (bypasses for testing)

---

## 6. WGSL / SimParams Extensions (minimal)

Add to SimParams:
```
pfc_tau: f32,
pfc_leak: f32,
pfc_refresh_strength: f32,
bg_da_tau: f32,
bg_da_kappa: f32,
bg_learning_rate: f32,
goal_bias_scale: f32,
sequence_gamma: f32,
```

New bindings:
- pfc_state buffer (read_write)
- bg_q_table or compact key/value arrays (if pure WGSL policy)
- da_state scalar or small struct
- disinhibit_channels array (size = registry cardinality)

For performance: keep BG policy update on CPU side after GPU readback of post-burst state + dream rollouts. Only the selection + disinhibition signals need to be fast and GPU-resident.

---

## 7. Brutalist Implementation Notes

- Fixed-size arrays everywhere in hot paths. No Vec in the tick or shader.
- PFC population is a single storage buffer; recurrent matrix can be low-rank factored (two small matrices U,V) to keep memory < 1 MiB.
- Sequence eligibility traces are sparse (only active (goal, context, prev_a) entries). Bound the trace map; evict oldest on overflow.
- All credit assignment for sequences happens after dream rollouts or after real step feedback arrives. Never inside the LIF inner loop.
- Goal IDs are 64-bit stable hashes or explicit registry indices. No strings in the loop.
- Termination signals: special canonical action "GOAL_COMPLETE" or energy collapse or explicit "ABORT" motor label.
- Safety: while real motor lock is active, disinhibition signals only affect the DreamWorldModel path. Real thalamic relays remain fully inhibited for motor execution.
- Debug surface: expose current PFC energy, active goal ID, last δ, current phasic_da, and the disinhibit vector for the next burst.

---

## 8. Cross-References to Prior Blueprints

- NEXUS_ACTION_MAPPING_AND_DREAM_STATE_BLUEPRINT.md — ActionKind, PrimaryAction, MotorActionRegistry, DreamWorldModel, VirtualFeedback, route_action.
- NEXUS_PRIMARY_ACTOR_BROCA_VALVE_BLUEPRINT.md — decide_after_burst, VP formula, ExecuteDirect path.
- NEXUS_THALAMIC_GAMMA_GATING_BLUEPRINT.md — burst_boost, SimParams, build_post_burst_context, BCS, thalamic inhibitory gate.
- NEXUS_HIPPOCAMPUS_ENGRAM_BLUEPRINT.md — phase-resonance retrieval, E_dream, is_dream flag.
- NEXUS_ASTROCYTE_TOPOLOGICAL_BLUEPRINT.md — Ca²⁺/IP3 PDEs, ξ terms, Betti-driven meta-plasticity, dual-kernel layout.
- NEXUS_SNN_MATHEMATICAL_FOUNDATION_BLUEPRINT.md — base LIF, 4-hormone modulation, eligibility, STDP, CSR layout.

All new mechanisms reuse the same credit assignment, engram, astrocyte, and PrimaryAction pathways. Only PFC-BG adds the persistence and sequencing layer on top.

---

**End of blueprint.**

This completes the formal definition for Phase 6. Antigravity implements the Rust/WGSL structures, the PFC shader pass, the BG selection + TD update, the disinhibition injection into the existing gate, and the dream rollout loop. The safety lock and all prior invariants remain untouched.