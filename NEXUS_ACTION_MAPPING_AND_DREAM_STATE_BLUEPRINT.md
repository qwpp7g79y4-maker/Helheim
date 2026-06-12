# NEXUS — Action Mapping & Dream State (Fase 3) Blueprint

**Status bij aanvang**: Fase 2 (Thalamic Gating) voltooid.  
`burst_boost` (2.8) wordt tijdelijk geïnjecteerd in `SimParams` gedurende een expliciete decision burst window van 5 ticks.  
`build_post_burst_context` berekent de echte Burst Coherence Score (BCS) op basis van participatie-ratio + circulaire phase-synchronisatie van motor-nodes (`node_type == 3`) via `last_spike_tick`.  
Blinde motor-executie is verwijderd. De **Primary Actor** (Thalamic Gate) beslist: `ExecuteDirect` | `RequestVerbalization` | `Suppress` op basis van BCS, cortisol en veto-pressure.  
Huidige stub in `ExecuteDirect`: `ActionKind::SysCommand { cmd: "echo..." }`.

Dit document levert de **architecturale wiskundige blauwdruk** en geïsoleerde pseudo-code voor de twee gevraagde stappen (geen enkele broncode wordt gewijzigd):

1. Action Mapping — hoe post-burst spikende motor-nodes (labels zoals `"MOTOR: ACTION_SEARCH_WEB"`) correct vertaald worden naar concrete `ActionKind` executies binnen `PrimaryDecision::ExecuteDirect`.
2. Droomstaat (Fase 3 Sandbox) — veilige integratie van virtuele executie die een **Virtual Feedback** (`R_v`) genereert via de GPU, zonder enige bash / host-OS / echte Helheim-orchestrator side-effects. De feedback voedt dezelfde paden als echte executie (eligibility credit, virtuele engrammen, astrocyte Ca²⁺/IP3).

---

## 1. Action Mapping — Motor Node Labels → PrimaryAction / ActionKind

### 1.1 Principes

- De SNN motor-laag (node_type == 3) produceert **intentie** als discrete labels. Dit zijn de "winnende" motor-nodes die tijdens de decision burst coherent spikes hebben gegenereerd.
- De **Primary Actor** beslist alleen *of* er een directe actie mag komen (op basis van BCS + hormonen + veto).  
  De **vertaling** van label → concrete semantiek gebeurt daarna in een expliciete, testbare mapper.
- Nooit string-parsing in de decision core. Nooit vrije tekst uit de LLM/Broca als bron van actie-semantiek.
- Elke gemapte actie draagt `origin_burst_id` + `hormone_snapshot` mee voor correcte credit assignment later.

### 1.2 Kernstructuren (geïsoleerde definitie)

```rust
/// Unieke identificatie van de burst die tot deze beslissing leidde.
pub type BurstId = u64;

/// De vier neurochemische modulatoren (exacte volgorde en betekenis uit eerdere fases).
#[derive(Clone, Copy, Debug, Default)]
pub struct NeuroChem {
    pub dopamine: f32,
    pub adrenaline: f32,
    pub cortisol: f32,
    pub serotonin: f32,
}

/// Wat de Primary Actor uiteindelijk uitvoert (of virtueel test).
#[derive(Clone, Debug)]
pub enum ActionKind {
    // === Echte executie (na veiligheidspal) ===
    HelheimExec {
        script: String,
        args: Vec<String>,
    },
    NativeTool {
        tool: String,
        params: serde_json::Value,
    },
    InspectSandbox { path: String },
    WriteSandbox { path: String, content: String },

    // === Legacy stub — alleen voor Fase-1 cleanup ===
    SysCommand { cmd: String },   // ← dit is de huidige echo-stub; verwijderen na mapping

    // === Dream-varianten (Fase 3) — zie sectie 2 ===
    DreamHelheimExec { script: String, args: Vec<String> },
    DreamNativeTool { tool: String, params: serde_json::Value },
    // ...
}

/// Volledige actie die de Primary Actor emit.
#[derive(Clone, Debug)]
pub struct PrimaryAction {
    pub kind: ActionKind,
    pub origin_burst_id: BurstId,
    pub hormone_snapshot: NeuroChem,
    pub participating_motor_labels: Vec<String>,   // rauwe of gecanoniseerde labels uit PostBurstContext
    pub bcs_at_decision: f32,
    pub veto_pressure_at_decision: f32,
}

/// Post-burst context zoals die nu door build_post_burst_context wordt gebouwd (Fase 2).
pub struct PostBurstContext {
    pub burst_id: BurstId,
    pub bcs: f32,                                   // echte BCS uit participatie + phase coherence
    pub participating_motor_nodes: Vec<MotorNodeInfo>,
    pub avg_hormones: NeuroChem,
    pub veto_pressure: f32,
    // ... (overige velden ongewijzigd)
}

pub struct MotorNodeInfo {
    pub node_id: u32,
    pub label: String,          // bijv. "MOTOR: ACTION_SEARCH_WEB" of "MOTOR_ACTION_SEARCH_WEB"
    pub spike_count_in_window: u32,
    pub last_spike_tick: u32,
    pub phase: f32,             // 0..2π afgeleid van timing binnen window
}
```

### 1.3 Canonicalisatie + Registry (wiskundig + pseudo)

```rust
/// Zet ruwe motor-labels om naar een stabiele canonische sleutel.
pub fn canonicalize_motor_label(raw: &str) -> String {
    raw.trim()
       .to_uppercase()
       .replace("MOTOR:", "")
       .replace("MOTOR_", "")
       .replace("ACTION_", "")
       .replace([' ', ':', '-'], "_")
       .split('_')
       .filter(|s| !s.is_empty())
       .collect::<Vec<_>>()
       .join("_")
    // Voorbeeld: "MOTOR: ACTION_SEARCH_WEB" → "SEARCH_WEB"
}

/// Template dat een canonische sleutel omzet in een ActionKind (of Dream-variant).
pub struct ActionTemplate {
    pub kind_constructor: fn(&[String], &serde_json::Value) -> ActionKind,
    pub requires_params: bool,
}

/// Registry (singleton of injected). Wordt eenmalig gevuld bij startup.
pub struct MotorActionRegistry {
    map: HashMap<String, ActionTemplate>,
}

impl MotorActionRegistry {
    pub fn new() -> Self { /* vaste mapping + eventueel config-driven extensie */ }

    pub fn resolve(
        &self,
        raw_label: &str,
        context: &PostBurstContext,   // kan later extra context (argumenten) leveren
    ) -> Option<ActionKind> {
        let key = canonicalize_motor_label(raw_label);
        let template = self.map.get(&key)?;
        // Hier kunnen we uit de context of een bijbehorende "argument node" plukken.
        // Voor minimale Fase 1: geen argumenten → pure intentie.
        Some((template.kind_constructor)(&[], &serde_json::json!({})))
    }

    pub fn resolve_many(
        &self,
        nodes: &[MotorNodeInfo],
        ctx: &PostBurstContext,
    ) -> Vec<ActionKind> {
        nodes.iter()
            .filter_map(|n| self.resolve(&n.label, ctx))
            .collect()
    }
}
```

**Voorbeeld mapping (in code te vullen door Antigravity):**

```rust
registry.register("SEARCH_WEB", ActionTemplate {
    kind_constructor: |_, _| ActionKind::NativeTool {
        tool: "web_search".into(),
        params: serde_json::json!({ "query": null }), // later verrijken
    },
    requires_params: false,
});
registry.register("WRITE_FILE", ...);
```

### 1.4 Integratiepunt in Primary Actor flow (na decide)

```pseudo
// In de beslissingslus (nexus_daemon of equivalent) na Fase 2:
let decision = primary_actor.decide_after_burst(&post_burst_ctx);

match decision {
    PrimaryDecision::ExecuteDirect => {
        let kinds = registry.resolve_many(
            &post_burst_ctx.participating_motor_nodes,
            &post_burst_ctx
        );

        if kinds.is_empty() {
            // fallback: RequestVerbalization of Suppress
            return;
        }

        // Voor minimale start: neem de eerste (of hoogste coherentie) als primaire intentie.
        let primary_kind = kinds.into_iter().next().unwrap();

        let action = PrimaryAction {
            kind: primary_kind,
            origin_burst_id: post_burst_ctx.burst_id,
            hormone_snapshot: post_burst_ctx.avg_hormones,
            participating_motor_labels: post_burst_ctx.participating_motor_nodes
                                            .iter().map(|n| n.label.clone()).collect(),
            bcs_at_decision: post_burst_ctx.bcs,
            veto_pressure_at_decision: post_burst_ctx.veto_pressure,
        };

        // Nu pas: echte executie OF droom-routering (zie sectie 2)
        route_action(action, post_burst_ctx);
    }
    PrimaryDecision::RequestVerbalization => { broca.open_with_pressure(vp); }
    PrimaryDecision::Suppress => { /* stilte + eventueel homeostase straf */ }
}
```

### 1.5 Fase 1 Cleanup richtlijnen (geen code, alleen richtlijn)

- Verwijder de hardcoded `SysCommand { cmd: "echo..." }` stub volledig.
- Maak `ExecuteDirect` zonder een succesvolle registry-hit onmogelijk (of forceer `Suppress` + lichte negatieve eligibility).
- Test: geef de motor-laag een label dat wél in de registry staat → `PrimaryAction` bevat het juiste `ActionKind`.
- De `origin_burst_id` is heilig: alle latere credit (eligibility × R of Rv) en engrammen moeten hiernaar verwijzen.

---

## 2. De Droomstaat (Fase 3) — Veilige Sandbox + Virtual Feedback Rv

### 2.1 Doel

Terwijl de **echte motor** (bash, FS, netwerk, Helheim-orchestrator calls) achter een veiligheidspal zit, moet de SNN toch kunnen leren van de *consequenties* van zijn eigen motor-intenties.

We voeren de actie **virtueel** uit in een geïsoleerde wereld, genereren een scalar virtual reward `R_v`, en injecteren die exact via dezelfde drie leerpaden als echte executie:

- SNN eligibility credit assignment (STDP + global reward × eligibility)
- Hippocampus: virtuele engrammen (`E_dream`)
- Astrocyte Ca²⁺/IP3 reactie-diffusie (met extra `ξ_dream` term)

### 2.2 VirtualFeedback (wiskundig contract)

```rust
#[derive(Clone, Debug)]
pub struct VirtualFeedback {
    pub rv: f32,                           // R_v ∈ [-1, +1] typisch; sterker signaal dan echte R mogelijk
    pub simulated_sensory: Vec<f32>,       // wat de "zintuigen" zouden hebben waargenomen
    pub virtual_hormone_delta: NeuroChem,  // hoe de virtuele uitkomst de interne staat zou moduleren
    pub dream_flag: bool,                  // altijd true voor dit pad
    pub origin_burst_id: BurstId,
    pub satiety_cost: f32,                 // >0 → verhoogt droom-verzadiging
    pub steps_simulated: u32,              // hoe lang de virtuele wereld is doorgedraaid
}
```

**Belangrijke invariant**: `R_v` mag de SNN nooit "echte" beloning geven in de zin van permanente wereldverandering. Het is een **interne simulatie reward** voor het trainen van de policy.

### 2.3 DreamActionExecutor trait (geïsoleerd contract)

```rust
pub trait DreamActionExecutor {
    /// Voert de PrimaryAction virtueel uit en retourneert de feedback.
    /// Mag intern GPU-ticks gebruiken (zelfde of afgeleide van cortex_compute).
    /// Mag NOOIT echte syscalls, FS-writes buiten de sandbox, of Helheim-orchestrator aanroepen.
    fn execute_virtual(
        &mut self,
        action: &PrimaryAction,
        ctx: &PostBurstContext,
    ) -> VirtualFeedback;

    /// Optioneel: reset de interne droom-wereld (bijv. na reality-check).
    fn reset_world(&mut self);
}
```

### 2.4 DreamWorldModel (interne simulatie)

- **Geen** host OS resources.
- In-memory virtueel bestandssysteem, virtuele web responses (stubbed of via een beperkt, veilig model), virtuele command outputs.
- Kan een **interne SNN-tick** draaien (of een lichte "world model" SNN) om temporele consequenties te simuleren.
- Voor maximale consistentie: hergebruik de bestaande GPU dual-kernel structuur met een `dream` bit of aparte buffer set (SNN spikes + astrocyte grid). Dezelfde `cortex_compute` shader kan met aangepaste `SimParams` (lagere `burst_boost`, of expliciete `dream_mode` flag) worden aangeroepen voor N interne stappen.

**Voorbeeld interne lus (pseudo):**

```pseudo
fn execute_virtual(action: &PrimaryAction, ctx: &PostBurstContext) -> VirtualFeedback {
    let mut world = self.dream_world.clone();   // pure data, geen side effects
    let mut total_rv = 0.0;
    let mut sensory_accum = vec![0.0; sensory_dim];

    for step in 0..self.max_dream_steps {
        // 1. "Voer" de actie virtueel uit in het world model
        let outcome = world.apply_action(&action.kind);   // bijv. simulated search result, file write in ram-fs

        // 2. Genereer simulated sensory consequence
        let sensory = world.observe();
        sensory_accum = sensory_accum.iter().zip(&sensory).map(|(a,b)| a + b).collect();

        // 3. Optioneel: tick een kleine interne SNN / world-model voor predictie van volgende staat
        if self.use_gpu_dream_tick {
            let dream_params = SimParams { burst_boost: 0.0, dream_mode: true, .. };
            let _ = gpu_cortex.execute_tick(&mut world.internal_snn_state, dream_params);
        }

        // 4. Lokale reward voor deze micro-stap
        let step_r = self.reward_fn(&outcome, &ctx.avg_hormones, &action);
        total_rv += step_r * self.discount.powf(step as f32);
    }

    let rv = (total_rv / self.max_dream_steps as f32).clamp(-1.0, 1.0);

    VirtualFeedback {
        rv,
        simulated_sensory: sensory_accum,
        virtual_hormone_delta: self.estimate_hormone_impact(rv, &ctx.avg_hormones),
        dream_flag: true,
        origin_burst_id: action.origin_burst_id,
        satiety_cost: self.satiety_per_step * self.max_dream_steps as f32,
        steps_simulated: self.max_dream_steps,
    }
}
```

### 2.5 Injectie van Virtual Feedback (exact dezelfde paden als real)

Na `execute_virtual`:

1. **SNN credit assignment** (identiek aan echte R):
   ```pseudo
   let scaled_rv = vf.rv * dream_reward_scale;   // vaak lager dan echte R (0.3..0.7)
   apply_eligibility_credit(scaled_rv, eligibility_traces, &mut weights);
   // STDP, homeostase, missed_spike penalty blijven exact werken
   ```

2. **Virtueel engram (Hippocampus)**:
   ```pseudo
   let e_dream = Engram {
       content: simulated_sensory + action.participating_motor_labels,
       origin_burst_id: vf.origin_burst_id,
       hormone_context: ctx.avg_hormones + vf.virtual_hormone_delta,
       is_dream: true,
       resonance_potential: vf.rv.abs(),
       dream_reality_factor: current_dream_reality_factor,
   };
   hippocampus.store(e_dream);
   ```

3. **Astrocyte modulatie** (zie NEXUS_ASTROCYTE_TOPOLOGICAL_BLUEPRINT):
   ```pseudo
   let xi_dream = astrocyte_coupling * vf.rv;
   // injecteer als extra bronterm in de IP3 / Ca²⁺ PDEs voor de droom-duur
   astrocyte_grid.inject_dream_source(xi_dream, duration = vf.steps_simulated);
   ```

### 2.6 Veiligheids- en homeostase-mechanismen (verplicht)

- **dream_reality_factor** (0..1): daalt bij elke pure droom-lus. Wordt langzaam hersteld door echte executie of expliciete reality-checks. Als te laag → forceer `Suppress` of `RequestVerbalization`.
- **satiety**: elke droom-actie verhoogt een interne counter. Bij hoge waarde daalt de effectieve |R_v| en stijgt de drempel voor `ExecuteDirect` in droom-modus.
- **reality-check current**: een langzaam accumulator die na N droom-stappen een "verlangen naar gronding" opbouwt. Kan een lichte negatieve eligibility geven als er te lang puur gedroomd wordt.
- **Geen persistentie**: droom-engrammen krijgen een lagere retrieval prioriteit of een expliciete `is_dream` tag die tijdens latere retrieval wordt meegewogen.
- **No side effects**: de `DreamWorldModel` mag nooit de echte `Orchestrator`, echte sockets, echte FS of echte Helheim-executor aanraken. Alle I/O is ofwel stubbed of volledig gesimuleerd.

### 2.7 Routering (orchestratie punt)

```pseudo
fn route_action(action: PrimaryAction, ctx: &PostBurstContext) {
    if safety_lock_active || current_mode == DreamMode {
        let vf = dream_executor.execute_virtual(&action, ctx);
        apply_virtual_feedback(vf);           // de drie injectie-paden hierboven
        update_dream_homeostasis(vf.satiety_cost);
    } else {
        // echte executie (pas later, na expliciete vrijgave van de pal)
        let real_outcome = real_executor.execute(action);
        apply_real_feedback(real_outcome);
    }
}
```

### 2.8 Relatie met eerdere fases

- Bouwt direct voort op **Thalamic Gating** (BCS, burst window, `build_post_burst_context`, `PrimaryActor::decide_after_burst`).
- Gebruikt dezelfde `PrimaryAction` struct met `origin_burst_id` voor traceable credit.
- Voedt exact dezelfde **eligibility**, **Hippocampus engram** en **Astrocyte** mechanismen (met `dream_flag` / `is_dream` / `ξ_dream`).
- `Broca Valve` (Verbalization Pressure) blijft werken; in droom-modus kan een lage BCS nog steeds verbalisatie triggeren in plaats van virtuele actie.
- Later (na Hippocampus + Astrocyte integratie) kan de droom-wereld ook topologische Betti-informatie gebruiken voor rijkere gesimuleerde consequenties.

---

## 3. Volgorde van implementatie (aanbeveling)

1. **Fase 1 cleanup** (direct):
   - Registry + canonicalisatie implementeren.
   - Stub `SysCommand` verwijderen.
   - `ExecuteDirect` altijd via mapper laten lopen (testbaar).
   - `origin_burst_id` + `hormone_snapshot` overal meenemen.

2. **Droomstaat kern** (Fase 3):
   - `DreamWorldModel` + `DreamActionExecutor` trait + minimale `VirtualFeedback`.
   - `route_to_dream_state` + `apply_virtual_feedback` (de drie injectiepaden).
   - Homeostase: `dream_reality_factor`, `satiety`, reality-check.

3. **Verrijking** (later):
   - GPU-accelerated dream ticks (dual-kernel hergebruik).
   - Argument-resolutie uit extra motor/argument nodes.
   - Droom-engram retrieval tijdens latere BCS-beslissingen (Hippocampus resonantie).
   - Astrocyte topologische modulatie op droom-wereld dynamiek.

---

## 4. Cross-references naar eerdere blueprints

- `NEXUS_SNN_MATHEMATICAL_FOUNDATION_BLUEPRINT.md` — LIF, 4-hormoon STDP, eligibility, CSR, homeostasis.
- `NEXUS_THALAMIC_GAMMA_GATING_BLUEPRINT.md` — dual memory layout, burst_boost, BCS = 0.65·participation + 0.35·phase_coherence.
- `NEXUS_PRIMARY_ACTOR_BROCA_VALVE_BLUEPRINT.md` — BCS drempel, VP formule, PrimaryDecision enum.
- `NEXUS_HIPPOCAMPUS_ENGRAM_BLUEPRINT.md` — E_dream met `is_dream` vlag, resonantie-retrieval.
- `NEXUS_ASTROCYTE_TOPOLOGICAL_BLUEPRINT.md` — Ca²⁺ PDEs, ξ_dream / ρ_SNN, Betti-gedreven IP3.

---

**Einde van de blauwdruk.**

Antigravity en gebruiker kunnen nu de Rust-integratie doen. Alle wiskunde, invarianten, structuren en injectie-paden zijn hier formeel vastgelegd. Geen enkele bestaande Helheim- of NEXUS-broncode is aangeraakt.

De veiligheidspal blijft 100% intact totdat expliciet besloten wordt hem (gedeeltelijk) te lichten.
