# NEXUS — Astrocyte-Driven Moods and Cognitive Fatigue Blueprint

**Current State (post Fase 5)**:  
A 2D coarse AstrocyteGrid with AstrocyteCell { c, p, ... } updated via 5-point stencil Euler integration of the Ca²⁺/IP₃ PDEs (as specified in ASTROCYTE_IMPLEMENTATION_SPEC.md).  
SNN bursts (from Motor Cortex / PFC) inject ξ_SNN (spike mass) and ρ_SNN (eligibility/IP3 drive) into overlapping grid cells via precomputed node-to-cell map.  
Currently: avg_c (mean calcium across grid) is used as a simple linear modulator for Verbalization Pressure (VP) in the Broca Valve.

**Problem**: avg_c is too fast and reactive. We need the slow diffusive waves in c and p to be the *causal generator* of persistent macro-states ("stemmingen" / moods) and long-term Cognitive Fatigue. These must:
- Persist for "hours/days" of engine runtime (independent of individual spikes or gamma bursts).
- Have biological grounding (CICR, IP3 signaling, Ca²⁺ overload → depletion/fatigue).
- Causally affect:
  - Verbalization Pressure (threshold and urgency to open Broca).
  - Existing 4-hormone snapshot (especially cortisol as stress/fatigue correlate).
  - LLM tone in the Broca output (frustrated, apathetic, hyperfocused, calm, etc.).
- Remain zero-overhead: no allocations, simple accumulators, updated at macro timescale.

This blueprint treats the astrocyte field as a true slow "neuromodulatory volume" whose spatial-temporal patterns *are* the mood, not just a trigger.

---

## 1. Slow Mood Variables — Mathematical Derivation from the Grid

The PDEs already provide the slow dynamics:
- c(x,y,t): local Ca²⁺ (fast release, slow diffusion + pump).
- p(x,y,t): IP₃ (slower, controls release sensitivity).

We do **not** add new fast variables. We extract *slow, history-dependent aggregates* from the existing fields after each diffusion step (or at even lower macro rate).

Define a small set of macro mood dimensions (inspired by astrocyte biology: Ca waves for arousal/fatigue, IP3 for signaling volume):

```math
M = (Arousal, Fatigue, Focus, Irritability, Valence)
```

### Global Aggregates (computed every macro step from the flat cell buffer)

Let N = width * height (fixed).

Mean Calcium:
C_mean = (1/N) Σ c_i

IP3 volume (signaling drive):
P_mean = (1/N) Σ p_i

Wave energy / spatial variance (coherence of the "mood field"):
E_wave = (1/N) Σ (c_i - C_mean)^2 + (p_i - P_mean)^2

Oscillation proxy (fatigue-relevant): track sign changes or use simple finite difference over last K macro steps on C_mean (high freq = agitated, damped = fatigued).

### Persistence and Hysteresis (the "hours/days" part)

Moods are **not** = current C_mean. They are slow integrators with depletion:

Introduce two persistent scalar states (updated at macro rate, double-buffered, zero alloc):

Fatigue accumulator (Ca²⁺ overload → mitochondrial depletion, homeostatic clamp):
dF/dt = α * (C_mean - C_baseline) - β * F
F = clamp(F, 0, F_max)   // biological saturation

(where α, β << 1, tuned so F integrates over 10^4–10^6 SNN ticks or real wall-time hours).

Arousal (phasic wave activity):
A = γ * E_wave + δ * (dC_mean/dt filtered)   // recent wave amplitude + direction

Focus (coherent high-Ca, low-variance state — "in the zone"):
Focus = C_mean * (1 - norm_var) * sigmoid(P_mean)

Irritability (high IP3 drive + fluctuations + low fatigue buffer):
Irrit = P_mean * (1 + |recent dC/dt|) * (1 - F/F_max)

Valence / recovery bias (slow):
V = low_pass( -F + (recent positive drive or successful dream Rv contribution) )

These five scalars (or a small vector) are the "mood state". They evolve on the timescale of the grid diffusion + the explicit integrators above.

---

## 2. Data Layout (Zero-Overhead Extension)

Extend the existing structures from ASTROCYTE_IMPLEMENTATION_SPEC.md without touching the hot PDE path.

```rust
#[repr(C)]
pub struct AstrocyteMoodState {
    // Persistent slow states (updated at macro rate)
    pub fatigue: f32,
    pub arousal: f32,
    pub focus: f32,
    pub irritability: f32,
    pub valence: f32,

    // For hysteresis and slow filtering
    pub c_mean_smooth: f32,
    pub p_mean_smooth: f32,
    pub energy_smooth: f32,
    pub last_c_mean: f32,
    pub integration_time: u64,
}

#[repr(C)]
pub struct AstrocyteGrid {
    // ... all existing fields (width, height, cells, maps) unchanged ...
    pub mood: AstrocyteMoodState,
}

#[repr(C)]
pub struct AstrocyteParams {
    // ... existing PDE params ...
    // New slow mood parameters (tuned once, zero cost)
    pub fatigue_alpha: f32,
    pub fatigue_beta: f32,
    pub fatigue_max: f32,
    pub arousal_gamma: f32,
    pub focus_coherence_weight: f32,
    pub irritability_p_weight: f32,
    pub mood_update_rate: u32,
    pub c_baseline: f32,
    // ...
}
```

Mood state is ~10 floats. Read-mostly from the rest of the engine (VP calculation, hormone snapshot, Broca prompt builder). Updated only in the slow path after PDE.

No per-cell mood. The waves live in the dense grid; moods are the global "volume transmission" readout.

---

## 3. Logical Flow & Isolated Pseudo-Code (Brutalist)

After the existing `run_astro_diffusion_step(grid, params, snn_xi_rho)`:

```pseudo
// Called from the same slow macro orchestration that already exists
// (post-burst or every N diffusion steps)
fn update_astrocyte_moods(grid: &mut AstrocyteGrid, params: &AstrocyteParams, current_macro_tick: u64) {
    if (current_macro_tick % params.mood_update_rate) != 0 { return; }

    let cells = ...; // flat access, no alloc
    let n = grid.cell_count as f32;

    // 1. Cheap aggregates (single pass, cache friendly)
    let mut sum_c = 0.0f32;
    let mut sum_p = 0.0f32;
    let mut sum_sq = 0.0f32;
    for i in 0..grid.cell_count {
        let cell = unsafe { &*grid.cells.add(i as usize) };
        sum_c += cell.c;
        sum_p += cell.p;
        let dev = cell.c - grid.mood.c_mean_slow;
        sum_sq += dev * dev;
    }
    let c_mean = sum_c / n;
    let p_mean = sum_p / n;
    let variance = sum_sq / n;

    // 2. Slow exponential smoothers (persistence)
    let alpha = 0.05; // very slow for "hours" feel; tune with real wall time if needed
    grid.mood.c_mean_slow = grid.mood.c_mean_slow * (1.0 - alpha) + c_mean * alpha;
    grid.mood.p_mean_slow = grid.mood.p_mean_slow * (1.0 - alpha) + p_mean * alpha;
    grid.mood.energy_slow = grid.mood.energy_slow * (1.0 - alpha) + variance * alpha;

    // 3. Fatigue integrator (the key long-term memory of overload)
    let delta = grid.mood.c_mean_slow - params.c_baseline;
    grid.mood.fatigue += params.fatigue_alpha * delta - params.fatigue_beta * grid.mood.fatigue;
    grid.mood.fatigue = grid.mood.fatigue.clamp(0.0, params.fatigue_max);

    // 4. Instantaneous mood dimensions (wave pattern driven)
    let d_c = grid.mood.c_mean_slow - grid.mood.last_c_mean;
    grid.mood.arousal     = params.arousal_gamma * (grid.mood.energy_slow + d_c.abs().min(2.0));
    grid.mood.focus       = grid.mood.c_mean_slow * (1.0 - (variance / (variance + 1.0))) * (1.0 + grid.mood.p_mean_slow * 0.1);
    grid.mood.irritability = grid.mood.p_mean_slow * (1.0 + d_c.abs()) * (1.0 - grid.mood.fatigue / params.fatigue_max);
    grid.mood.valence     = -grid.mood.fatigue * 0.8 + (recent_successful_dream_rv_or_positive_xi * 0.2);

    grid.mood.last_c_mean = grid.mood.c_mean_slow;
}

// Called from existing VP calculation site (Broca Valve)
fn modulate_verbalization_pressure(base_vp: f32, mood: &AstrocyteMoodState, current_cortisol: f32) -> f32 {
    let fatigue_term     = mood.fatigue * 1.8;
    let irrit_term       = mood.irritability * 0.9;
    let arousal_mod      = (mood.arousal - 1.0).clamp(-0.5, 1.5);
    let calm_buffer      = (mood.valence + 0.5).max(0.0) * 0.6;

    let vp = base_vp
           + fatigue_term
           + irrit_term
           + arousal_mod
           - calm_buffer
           + (current_cortisol - 0.5) * 0.4;

    vp.clamp(0.0, 10.0)
}

// Called when building the prompt / tone for the LLM (Broca)
fn get_broca_tone_bias(mood: &AstrocyteMoodState) -> MoodTone {
    if mood.fatigue > 0.65 {
        MoodTone { prompt_addition: "The system is cognitively fatigued and low-energy. Use short, direct, slightly curt language. Avoid enthusiasm or elaboration unless explicitly required.", strength: 0.75 }
    } else if mood.irritability > 1.2 && mood.arousal > 1.4 {
        MoodTone { prompt_addition: "The system is overstimulated and irritated. Use sharper, more critical or impatient phrasing. Short sentences preferred.", strength: 0.65 }
    } else if mood.focus > 1.7 && mood.fatigue < 0.25 {
        MoodTone { prompt_addition: "The system is in a state of high focus and precision. Be technical, concise, and direct. Longer coherent chains are ok.", strength: 0.55 }
    } else if mood.valence > 0.35 && mood.arousal < 0.9 {
        MoodTone { prompt_addition: "The system is in a calm, recovered state. Respond measured and slightly positive but never effusive.", strength: 0.45 }
    } else {
        MoodTone { prompt_addition: "", strength: 0.0 }
    }
}
```

The MoodTone (or equivalent struct) is injected into the existing Broca/VerbalizationRequest path — either as extra context in the snapshot or as a pre-prompt modifier. Because the LLM is only the transducer, the "mood" is applied as a hard bias on top of the raw SNN snapshot.

---

## 4. Integration Points & Causal Chain (Zero-Overhead)

1. SNN burst → accumulate mass into AstrocyteGrid (existing).
2. run_astro_diffusion_step() (existing 5-point stencil).
3. update_astrocyte_moods() (new, cheap single-pass + a few mul/add, called at low rate).
4. When Broca Valve checks pressure:
   - Read the small AstrocyteMoodState (cache-friendly).
   - modulate_verbalization_pressure(base_from_bcs + hormones + uncertainty, mood, current_cortisol).
   - If pressure > threshold → open valve.
5. When building the actual LLM call / snapshot:
   - Attach or pre-apply get_broca_tone_bias(mood).
   - Also expose mood fields in the MinimalSnnSnapshot so the transducer sees "fatigue=0.82, irritability=1.3" etc.
6. Feedback loop (existing astrocyte modulation paths): high fatigue can further damp eligibility traces or STDP amplitudes → future bursts become "lazier" or more fragmented → more fatigue. Classic vicious/virtuous cycle.

Cortisol is no longer the primary driver; it becomes partly downstream of the astrocyte fatigue integrator (biologically plausible).

The mood persists because F and the slow smoothers have time constants much larger than any single burst or even a full gamma cycle. Once the waves have diffused and the integrator has charged, individual spikes have almost no immediate effect.

---

## 5. Brutalist Implementation Notes (Rust Engine)

- MoodState lives in AstrocyteGrid as a single small struct — one cache line.
- update_astrocyte_moods is a single linear pass over the cells (same as the existing accumulation). Fuse it with any post-diffusion work if you want.
- All heavy lifting stays in the existing PDE solver. Mood extraction is O(N) but called infrequently.
- For real "hours/days" in a long-running daemon: either use a wall-time accumulator in the macro_tick or keep the constants extremely small and rely on the integrator. Both work; the PDE smoothness + the explicit fatigue integrator give the hysteresis for free.
- When serializing state (engrams, checkpoints, Broca snapshot) always include the full AstrocyteMoodState. It is now part of the system's "mind".

This makes the "humeur" a direct, slow, causally upstream consequence of the same physical process (Ca²⁺/IP₃ waves) that is already running for meta-plasticity. The fast SNN provides the forcing; the slow field integrates it into something that can dominate tone and pressure long after the spikes are gone.

The rest of the architecture (4-hormone snapshot, eligibility modulation, Primary Actor decision, Broca transducer) simply reads the new persistent mood fields. No magic, no extra state machines. 

---

**End of blueprint.**

This is the causal story the user asked for. The astrocytes are no longer a "trigger" — their wave dynamics *are* the long-term mood substrate. Everything else (VP, cortisol, Broca tone, future eligibility) reads from it.

Antigravity can drop the two new functions into the existing slow orchestration path with minimal diff. All prior PDE, mapping, and SNN modulation code remains untouched.