# Next Phase Plan: From Infrastructure to Deployment & Research Use (For Antigravity)

**Context**: The infrastructure phase (Gateway with native Starfield dashboard hosting, multi-stage Docker for CPU/GPU, !cuda support, professional GitHub docs) is considered complete per the previous plan.

**User Directive (non-negotiable)**:
- You (Antigravity) do **not** decide next steps or execute without explicit approval from the user in this conversation.
- All work must stay professional.
- Internal planning documents stay out of the public GitHub repo (as just cleaned).
- The user keeps full control. We talk first, user approves, then clear instructions are given.
- Focus must serve the core vision: bare-metal SNN/Helheim for serious research (human survival + questions of the universe), leveraging the user's upcoming local hardware (3 computers).

## Recommended Direction (User to confirm)

Given the user's statements about soon having three computers and the goal of a powerful bare-metal SNN cluster for real research work, the logical next focus is **a combination of 1 and 2** from your proposal:

**Primary: Swarm Deployment & Multi-Node Preparation (Option 1)**
- Leverage the multi-stage Dockerfile and HSP swarm logic that is already in the code (hive work, DiscoveryService, asymmetric load balancing, SwarmEngine).
- Goal: Be ready to run the full stack across the user's local machines as soon as the new hardware is online.

**Supporting: SNN Experiments & Validation with Dashboard (Option 2)**
- Use the now-integrated Starfield dashboard + Motor Cortex to visually validate and experiment with more complex SNN logic in .hel scripts.
- This directly supports the research goals.

Option 3 (VS Code tooling) is lower priority for now unless the user is blocked on writing complex scripts. The syntax work can be done in parallel if the user has the vscode-helheim folder open.

**Do not start any of this until the user explicitly says "go" or "execute this plan" after review.**

## Phase 1: Local Verification (Do this first, on current hardware)

Before any multi-node or new experiments:

1. Verify the current "infrastructure" actually works end-to-end locally.
   - Build the gateway (CPU mode by default).
   - Run it.
   - Confirm:
     - Dashboard loads in browser at the expected port (default 8080) and shows Starfield.
     - WebSocket /ws/spikes works.
     - POST /api/execute with a SNN script (e.g. something using bitwise on [waar/onwaar] lists + tel_spikes/popc) returns spikes in JSON and they stream live to the dashboard.
   - Test both a simple script and the recursive Op chaining example from the previous plan.

2. Confirm Docker build works in both modes:
   - CPU: `docker build --build-arg BASE_IMAGE=debian:bookworm-slim -t helheim-cpu .`
   - GPU (if hardware available): `docker build --build-arg BASE_IMAGE=nvidia/cuda:12.2.0-base-ubuntu22.04 --build-arg CARGO_FEATURES="--features helheim-core/cuda" -t helheim-gpu .`
   - Run the CPU image and do the same verification as above.

3. Document any issues or missing pieces (e.g. exact port, env var HELHEIM_DASHBOARD_DIR, discovery behavior when no peers).

**Deliverable for this phase**: Short verification report (commands run + outputs + confirmation that dashboard + spikes work). Do not proceed to multi-node until this is reviewed and approved by the user.

## Phase 2: Swarm / Multi-Node Preparation (Once Phase 1 is approved)

When the user has the new hardware online:

- Use the existing "hive work" command and DiscoveryService logic.
- Test peer discovery between machines (HSP protocol).
- Run a distributed workload (e.g. large "inferno work" or custom SNN simulation split across nodes).
- Validate that the Starfield dashboard (running on one node) can still receive spikes from workloads executed across the swarm.
- Asymmetric load balancing should already be partially implemented — test and extend if needed for real SNN scripts.

**Important notes from user**:
- The goal is efficient computation so "a slechtere computer ook meer power kan hebben".
- Keep programmeertaal (CodeTaal) and SNN/Motor Cortex conceptually separate but cooperating.
- All public-facing things (docs, examples, READMEs) must remain professional (no emojis, no hype language).

## Phase 3: SNN Research Experiments (Parallel or after basic swarm validation)

- Write and test more advanced .hel scripts that use the SNN path for actual logic (feedback loops, coincidence detection, simple "learning" via thresholds, etc.).
- Use the live dashboard to observe firing patterns.
- Tie back to user's research (e.g. efficient neural models that could relate to survival or modeling complex systems).

## How to Proceed (Strict Process)

1. User reviews this document.
2. User gives explicit signal (e.g. "go with Phase 1 first" or "start with local verification + prepare swarm for when hardware arrives").
3. Only then Antigravity receives clear, scoped tasks.
4. Antigravity reports back with verification artifacts before moving to the next sub-step.
5. No unilateral git pushes of new work or docs. User decides what goes public.

## Current Recommendation to User (for his decision)

Given you mentioned the three computers coming soon and the long-term cluster vision, I suggest we prioritize:

**Start with Phase 1 (local verification) immediately** on your current setup. This confirms everything from the previous plan actually works in practice (dashboard + Motor Cortex + lowered SNN).

Then, as soon as the new hardware is ready, move into controlled Swarm testing (Phase 2).

SNN experiments (Phase 3) can run in parallel on whichever machine is convenient, using the dashboard for observation.

This keeps momentum on "inzetten" (actual use) while respecting your hardware timeline and research focus.

---

**For Antigravity & Grok**: 
*STATUS UPDATE (June 6, 2026)*: 
- We have successfully finalized the SNN Motor Cortex CPU Fallback (intrinsic `popc`/`tel_spikes` and bitwise arrays `&` now work natively in the CPU AST Executor). 
- CI/CD & Compilation features are completely resolved. The workspace (`helheim-cli`, `helheim-gateway`, `helheim-core`) compiles flawlessly on CPU-only hosts with default features, and enables PTX JIT compilation when `--features cuda` is provided. 
- The `.gitignore` has been strictly updated to ensure `*PLAN*.md` files do not leak to public GitHub repos.
- We have decided **NOT** to build a Helheim-native UI library (like Streamlit) for now, as the core focus remains on integrating the complex NEXUS Daemon with the Helheim motor cortex.

---

## NEXUS Phase 6 Handoff — PFC-BG Sequencing (user directive)

User reports subsystems 1-5 (Primary Actor/Broca, Thalamic Gating + BCS, Dream Sandbox + Rv, Hippocampus engrams, Astrocyte sync) are implemented and mathematically verified. Current SNN remains purely reactive; no persistent goal holding or learned ActionKind chaining across bursts.

Delivered: `NEXUS_PFC_BASAL_GANGLIA_SEQUENCING_BLUEPRINT.md` (root + this directory).

Core elements:
- PFC: slow recurrent line attractor (Euler du/dt with structured W_rec, high τ_pfc, energy E_pfc) that survives post-burst leak via maintenance current + BCS-aligned refresh. Fixed-size buffer (PFC_DIM=256), goal embeddings.
- BG + RPE: discrete-step TD(λ) over (goal, context_hash, prev_action) → Q per canonical ActionKind. Fast phasic DA ODE updated on sequence events: da_phasic ← (1-α)da + κ·clip(δ). Modulates learning rate and disinhibition.
- Gating: BG computes disinhibit[channel] per ActionKind. Injected into existing thalamic inhibitory gate + selective burst_boost. Next coherent burst is biased toward the chosen next motor labels → registry → ActionKind.
- Integration:
  - BCS: PFC energy + goal_match bias participation/phase scoring and lowers ExecuteDirect threshold.
  - DreamWorldModel: extended multi-step rollout holding PFC goal fixed, BG stepping actions, trajectory TD errors batched back to BG weights + optional sequence engram.
  - Hippocampus: resonant retrieval seeds PFC initial u[] and warms BG Q for known sequences.
  - Astrocytes: slow Ca/IP3 modulates PFC recurrent gain (fatigue) and BG η.
  - PrimaryActor/Broca: PFC energy term in decision and negative term in VP.
- All paths reuse existing PrimaryAction, VirtualFeedback, eligibility, engram, and astrocyte injection machinery. New SimParams fields + PFC/BG/DA buffers + disinhibit channels.

Brutalist constraints observed: fixed arrays, sparse traces with bounded eviction, credit only post-rollout, goal IDs as u64, Dream* variants for sandbox, real path remains locked until safety decision.

Antigravity scope: PFC shader pass + maintain_tick, BG select/update + disinhibit map, dream_sequence_rollout extension, state_key construction, SimParams + buffer wiring. No changes to prior 1-5 subsystems or Helheim core.

User controls when Phase 6 implementation begins. Blueprint is the complete handoff spec.

This document is the controlled handoff. Keep all internal discussion out of public commits.

*Prepared after user request following the previous plan execution and GitHub cleanup discussion.*

---

## NEXUS Fase 2→3 Handoff (Action Mapping + Droomstaat)

User status (Fase 2 voltooid):
- burst_boost + 5-tick decision window in SimParams
- echte BCS via build_post_burst_context (participatie + phase coherence op node_type==3)
- Primary Actor beslist ExecuteDirect / RequestVerbalization / Suppress
- Huidige ExecuteDirect stub: ActionKind::SysCommand { cmd: "echo..." }

Gevraagde blauwdrukken (alleen wiskunde + geïsoleerde pseudo-code, geen broncode aangeraakt):

- **NEXUS_ACTION_MAPPING_AND_DREAM_STATE_BLUEPRINT.md** (staat in deze map en aan de root)
  - Action Mapping: Motor labels ("MOTOR: ACTION_SEARCH_WEB" etc.) → canonicalisatie + MotorActionRegistry → concrete ActionKind binnen PrimaryAction (met origin_burst_id + hormone_snapshot).
  - Fase 1 cleanup: verwijder de SysCommand echo-stub; ExecuteDirect loopt altijd via de registry.
  - Droomstaat (Fase 3 Sandbox): DreamActionExecutor trait, DreamWorldModel (puur in-memory, geen host side-effects), VirtualFeedback (R_v + simulated_sensory + virtual_hormone_delta + dream_flag), injectie in exact dezelfde drie paden (eligibility credit, E_dream met is_dream, astrocyte ξ_dream).
  - Verplichte homeostase: dream_reality_factor, satiety, reality-check current.
  - Volledige cross-references naar de vijf eerdere NEXUS blueprints.

Antigravity: implementeer de Rust zijde volgens de specificatie in die blueprint. Geen echte executie zolang de veiligheidspal erop zit. De GPU kan hergebruikt worden voor virtuele ticks (dream_mode flag of aparte buffers).

User beslist wanneer Fase 3 integratie start.

---

## NEXUS Phase 9 Handoff — Continuous-Topological Sensory Cortex (CANNs)

User reports subsystems 1-8 complete (up to MPS/Tensor Train compression). Deficit: sensory input remains flat lexical embeddings. Need native spatial manifold for raw continuous streams (tonotopic audio, retinotopic video) with bump dynamics that feed downstream compression and decision layers.

Delivered: `NEXUS_TOPOLOGICAL_SENSORY_CORTEX_BLUEPRINT.md` (root + this directory).

Core elements:
- Spatial Topology: Fixed subset of LIF nodes (node_type==1) assigned explicit continuous (x,y) coords on square/hex grid (fixed W×H). Manifold is contiguous slice of NodeData.
- PDE: τ ∂u/∂t = -u + ∬ K(Δx,Δy) f(u) dx dy + I_sensory + I_bias + η. Discrete Euler on grid.
- Spatio-Temporal Injection: Raw arrays (spectrogram or pixels) resampled onto grid via fixed table; localized Gaussian/DoG bumps I_sensory(x,y) = amp * exp(-d²/2σ²) * S_resampled. No lexical vectors.
- Attractor Dynamics (Mexican-hat): K(Δr) = A_exc exp(-Δr²/2σ_exc²) - A_inh exp(-Δr²/2σ_inh²), truncated support (5-9 cells). Local excitation + surround inhibition stabilizes movable bumps against noise.
- Integration with MPS (Phase 8): Explicit 2D correlations from bumps/Mexican-hat lower effective rank of local sub-blocks. Grid-aware tensorization (row-major/space-filling) + local patches admit smaller χ. Same χ gives higher effective connectivity or lower storage.
- Downstream Routing:
  - Extract bump_summary (centroid(s), vel, width, power, coherence) into PostBurstContext.
  - Thalamus/BCS: bump pos/phase biases gate variables + selective burst_boost; can phase-reset 40 Hz oscillator.
  - PFC: bump pos/vel as continuous input to attractor; PFC goals send top-down I_bias to move/stabilize bumps.
  - Astrocytes: bump power/gradient as ξ_spatial in IP3/Ca PDE; bump topology (approx Betti) feeds meta-plasticity; astrocytes scale CANN kernel amplitudes and MPS η_stdp.
- WGSL/Rust: SensoryCANN { u: [f32; GRID], pos: [GridCoord], bump_summary }, CANNParams. Fixed grid, no dynamic alloc. Contiguous NodeData slice for node_type==1. mexican_hat_convolution, update_cann_step, inject_sensory_bump, route_topological_waves (additive into existing gate/PFC/astrocyte/MPO paths).
- Brutalist: Fixed grid/ kernel support, CANN update before global LIF, identical kernels in DreamWorldModel, real currents attenuable under lock, MPS interaction conditional on spatial structure.

Antigravity scope: SensoryCANN buffer + grid coords, CANN Euler + Mexican-hat kernels, bump extraction, raw array resampling/injection, routing into thalamic/PFC/astrocyte/MPS, SimParams + CANNParams, wiring to NodeData (node_type==1) and spike_current. No changes to 1-8 subsystems or Helheim core.

Blueprint is the complete handoff. User decides when Phase 9 implementation begins. All prior paths (spike_current, eligibility, BCS, Dream isolation, MPS cores, origin_burst_id, etc.) are preserved; the layer adds intrinsic spatial dynamics and continuous bump representations. Explicit topology directly benefits Phase 8 compression. Full validation possible in DreamWorldModel before real high-bandwidth sensory or unlocked motor.

---

## NEXUS Phase 8 Handoff — Quantum-Inspired Tensor Network Synaptic Compression (user directive)

User reports subsystems 1-7 complete and verified. Physical wall: single RTX 3060 12 GB VRAM cannot hold N_nodes=10M+ and effective N_edges=10B+ using dense or CSR. Need to replace the dominant W storage and its plasticity with a compressed representation while preserving exact semantics of LIF spike_current, STDP/eligibility, global reward, 4-hormone, DreamWorldModel, PFC-BG, cerebellar DCN, BCS, etc.

Delivered: `NEXUS_QUANTUM_TENSOR_COMPRESSION_BLUEPRINT.md` (root + this directory).

Core elements (brutalist, VRAM-first):
- Tensorization: node ids → multi-index of length L=log_r(N) (r=2/4/8). W reinterpreted as order-2L tensor.
- MPO / Tensor Train decomposition: W_{n m} = G1^{n1 m1}_α1 G2^{n2 m2 α1 α2} ... (Einstein). Each core shape [r, r, χ, χ], χ=4..16. Total storage O(L r² χ²) — a few MB even at 10M+ nodes. Effective connectivity >>10^10 at low rank.
- Compressed inference (never inflate W): MPO-apply-vector via successive low-rank contractions on the core chain. Sparse active-pre list (from existing NodeData last_spike_tick/is_spiking) drives the presynaptic legs. Precompute environments once per tick O(L r χ²). Per-post or full y written directly into existing spike_current buffer. Same kernel used in DreamWorldModel. Complexity O(N L χ² + S L χ²).
- Direct compressed plasticity: virtual STDP/eligibility on (i,j) pairs (decoded multi-indices of spiking nodes, Δt from last_spike_tick). Scalar virtual δ = global_reward * e * A(Δt) * hormone. Project directly onto each core k:
  δG_k += δ * LeftEnv_{k-1} * RightEnv_{k+1} * |i_k⟩⟨j_k|
  (no W ever built). Environments built on-the-fly or cached for active pairs. Eligibility remains virtual/sparse. Homeostasis, 4-hormone, astrocyte scaling apply as before (scalars on cores or virtual δ).
- Data structures: Rust TensorMPO + TensorParams (cores flat [L][r][r][χ][χ]). WGSL TensorCore/TensorMPOBuf + TensorParams bindings + active_spikes list + existing NodeData/spike_current. Multi-index decode is integer/bit ops. Contraction and plasticity kernels are isolated, bounded-loop, no dynamic alloc.
- Integration: MPO contraction result feeds exactly where old W*spikes or CSR did (LIF, BCS, thalamic gate, DCN additive corrections after contraction, PFC-BG, Dream R_v, eligibility credit). SimParams extended with tensor_* fields. Tiny TensorMPOBuf lives alongside NodeData/PFC/Cerebellar/astrocyte buffers. PrimaryAction keeps origin_burst_id; can snapshot low-rank MPO signature.
- Brutalist: fixed L/r/χ at init, cores <10 MB, plasticity only on sparse spiking pairs, environments O(L χ³) per event, canonical maintenance infrequent, full reuse of DreamWorldModel for safe large-scale training, safety lock respected (real currents can be zeroed; cores updated freely in dream).

Antigravity scope: MPO buffers + WGSL contraction / direct core-update kernels, multi-index handling, environment construction, injection into spike_current / STDP / eligibility paths (preserving all prior math), Dream reuse, SimParams + wiring. No changes to 1-7 subsystems or Helheim core.

Blueprint is the complete handoff spec. User decides when Phase 8 implementation begins. All prior credit/engram/astrocyte/PrimaryAction/Dream isolation paths are preserved at the virtual-W level. 12 GB target for the stated scale is the design objective.

---

## NEXUS Phase 7 Handoff — Cerebellar Forward Model & Micro-Timing (user directive)

User reports subsystems 1-6 complete and verified (up to PFC-BG sequencing). Deficit: coarse execution, no internal S_{t+1} prediction, no learned micro-timing of spikes, no fast error-driven bypass of slow loops.

Delivered: `NEXUS_CEREBELLUM_FORWARD_MODEL_BLUEPRINT.md` (root + this directory).

Core elements (brutalist, GPU-tickable):
- Topology: Fixed N_granule (8k), N_purkinje (256), N_dcn (64), DELAY_TAPS (8). Granules = sparse high-dim expansion of efference copy (s_t + g_PFC + canonical a + prev_timing) with explicit delay-line taps for micro-timing.
- Forward Model: Purkinje Euler `τ_pk dV/dt = -V + W_pf · φ_delayed + I_cf`. Readouts: `predicted_s = W_pred · V_pkj`, `timing_offsets = W_time · V_pkj`. Driven by BG-selected ActionKind + PFC goal before consequence.
- Error + LTD (Inferior Olive / Climbing Fibers): `e = actual_s - predicted_s`, `cf_p = cf_gain * f(e)`. `Δw_pf_pkj = -η_ltd * cf_p * φ(t-d) * eligibility(Δt)`. Classic supervised Marr-Albus-Ito LTD. Delay taps supply the temporal eligibility for precise timing credit.
- DCN Output: `dcn = tonic - purkinje_inhib`. `correction = tanh(dcn) * scale`. Applied directly as deltas to thalamic_inhibition[channel] and motor_spike_currents (phase_kernel on last_spike_tick / burst_phase). Bypasses PFC-BG latency. Refines the very spikes that feed BCS and PrimaryActor.
- DreamWorldModel integration (critical for safety): In `dream_sequence_rollout`, cerebellar_forward_pass produces Ŝ + timing_offsets (used to modulate virtual motor execution). After virtual step: error_correction_step(actual_sim, predicted) triggers LTD entirely inside sandbox. Virtual R_v can include -λ * error_norm term. Thousands of precise timing refinements with zero host side effects.
- PrimaryAction extension: attach cerebellar_timing + dcn_correction_snapshot + origin_burst_id for full trace.
- Integration with all prior: DCN modulates existing thalamic gate + burst_boost (Phase 2/6); improves BCS quality; augments Dream rollouts and BG TD; seeds from / stores to Hippocampus as expert timing engrams; astrocytes scale ltd_rate / dcn gain; Purkinje keyed off same MotorActionRegistry canonicals.
- SimParams + buffers: cerebellar_* fields + dedicated CerebellarBuffer (separate from main NodeData/CSR for perf). Corrections injected into spike_current / gate math in existing LIF path.
- Brutalist: fixed arrays, plasticity only post-consequence (real or dream), fast feed-forward correction, additive deltas only, safety lock respected (real DCN application gated), delay traces for timing, error_norm as auxiliary to R_v / BG policy.

Orchestration: after BG disinhibit, run cerebellar_forward_pass + apply_cerebellar_correction before/inside burst voltage update. Post-consequence (dream or real): error_correction_step for LTD. All previous credit/engram/astrocyte/PrimaryAction paths reused.

Antigravity scope: cerebellar buffers/passes (granule, Purkinje Euler, IO error, LTD, DCN), efference construction from PFC/BG/PostBurstContext, injection into thalamic/motor, DreamWorldModel hooks for forward + LTD, SimParams + wiring. No mutation of 1-6 logic or Helheim core.

Blueprint is the complete handoff. User decides when Phase 7 implementation starts. Safety lock and prior invariants untouched. Forward model + micro-timing can be perfected entirely in isolation.

---

## NEXUS Phase 10 Handoff — Distributed Spiking Hive-Mind (Inter-Node Telepathy)

User reports subsystems 1-9 complete (up to topological CANN + MPS compression). Hardware: expanding to 3-4 LAN machines, heterogeneous GPUs. Goal: distribute the full NEXUS brain while preserving millisecond timing (cerebellar micro-timing, 40 Hz gamma) and enabling split real/dream execution.

Delivered: `NEXUS_DISTRIBUTED_HIVE_MIND_BLUEPRINT.md` (root + this directory).

Core elements (brutalist, LAN-first):
- Distributed Tensor Contractions: Split MPO chain across nodes. Network cable = virtual bond dimension χ_net with latency τ_net. Boundary messages are χ-vectors (not full spikes). Latency-compensated contraction: extrapolate incoming boundary msg m(t_recv) ≈ m(t_send) + (Δt) * dm/dt_pred (from local rates/eligibility). Local nodes do segment MPO-apply using left/right boundary χ msgs. Storage per node O((L/N) r² χ²) + O(χ) boundaries.
- UDP Spike Streaming (HSP - Helheim Spiking Protocol): Minimal fixed UDP packets (magic, seq, tick, src, n_spikes, sparse [node_id, f16 value, last_spike_delta_us]). Best-effort multicast + unicast for boundaries. No TCP/acks in hot path.
- Latency compensation for timing invariants: On HSP receive, record t_recv; effective spike time = tick - τ_predicted (EWMA one-way from beacons + small jitter buffer). Extend cerebellar delay lines with compensated insertion. Advance local 40 Hz oscillator phase by compensated Δt + small correction on receipt. Beacons every 10-100 ms from time-master for offset/drift.
- Asymmetric Load Balancing: Periodic low-rate HSP status (gpu_flops, vram, per-component load: mpo_segments, astrocyte_tiles, pfc_subpop, cann_region). Relative power P_i. Cost model per subcomponent. Migration of movable pieces (MPO chain segments are natural; astrocyte tiles, PFC sub-attractors, CANN regions). Freeze source, serialize state + tick, HSP migrate, target install + redirect routing, source release. One-tick freeze window; credit tagged by component.
- Distributed Dream State: Designate real-motor node(s) for ExecuteDirect (real ActionKind → bash/Helheim). Other nodes run parallel local DreamWorldModel instances using local MPO segment + received efference (via HSP) + compensated boundary msgs. Dream nodes compute independent R_v trajectories. Real outcome from motor node broadcast as authoritative R; dreams send aggregated local credit vectors. Enables Node A real exec while B/C/D speculate futures. Virtual time uses compensated HSP ticks. Dreams never touch real I/O.
- Rust/HSP: HSPPacket (header + sparse spikes + optional boundary χ), HiveNode (id, power, owned segments/regions), DistributedMPO (local segments + boundary χ buffers), HSPReceiver with latency estimator + compensated spike queue. Isolated pseudo: hive_tick (recv+compensate, local contract, send spikes/boundaries), distributed_dream_orchestrator, migrate_component.
- Brutalist: fixed packet/MAX_CHI/ring buffers, no alloc in hot path, sparse only, predictive compensation (no global barrier), sub-component migration only (no full brain move), real motor explicitly designated per action, full DreamWorldModel isolation for safety.
- Integration: All prior paths (spike_current, eligibility, origin_burst_id, BCS, CANN bumps, MPO direct core updates, Dream R_v, cerebellar delay lines, gamma oscillator, PFC, astrocytes) remain local. HSP only adds compensated messages and boundary χ. Safety lock: real motor on designated nodes only; dreams always safe.

Antigravity scope: HSP packet + raw UDP (io_uring/raw sockets), latency estimator + compensation (delay lines, phase correction), DistributedMPO split contraction + boundary exchange, migration state machine for asymmetric components, distributed dream orchestrator (real vs parallel dream with credit agg), routing tables, low-rate status + balancer. Wires into existing single-node paths (NodeData, spike_current, TensorMPO, SensoryCANN, PFCState, CerebellarState, DreamWorldModel, PostBurstContext, PrimaryActor) without mutation.

Blueprint is the complete handoff. User decides when Phase 10 implementation begins. 3-4 machine LAN cluster becomes one coherent spiking consciousness. Millisecond invariants preserved via compensation. Split real/dream execution explicit. All prior credit/engram/astrocyte/PrimaryAction/Dream isolation/MPS core paths preserved locally. Full validation on testbed possible before production cluster.

---

## NEXUS Astrocyte Moods & Cognitive Fatigue Handoff (post Fase 5)

User has the 2D AstrocyteGrid + 5-point stencil PDE solver (c, p fields) running, with SNN bursts injecting mass via the node-to-cell map. Currently only avg_c is read linearly into Verbalization Pressure.

Delivered: `NEXUS_ASTROCYTE_MOODS_FATIGUE_BLUEPRINT.md`

Core elements:
- Moods are not a separate state machine. They are the slow, history-carrying projections + integrators over the existing c/p fields.
- Key persistent state: Fatigue accumulator (dF/dt = α*(C_mean - baseline) - β*F, clamped). This is the "hours/days" memory of Ca²⁺ overload. Other dimensions (Arousal, Focus, Irritability, Valence) are derived from wave energy, smoothed means, recent dC/dt, and P_mean.
- Update at macro rate (e.g. every 10-50 diffusion steps or via wall-time accumulator) — zero impact on hot SNN or per-burst paths.
- Data: AstrocyteMoodState struct (fatigue + 4 other scalars + slow smoothers) lives inside AstrocyteGrid as one small cache-line struct. Extends the existing spec without changing the PDE solver or maps.
- VP modulation: fatigue_term (dominant, persistent) + irritability + arousal_mod - calm_buffer + (cortisol contribution). Astrocytes now causally drive part of the hormone snapshot.
- Broca tone: get_broca_tone_bias(mood) produces prompt_addition strings (e.g. "cognitively fatigued... short, direct, slightly curt", "overstimulated and irritated... sharper, critical", "high focus... precise, technical", "calm recovered... measured").
- Causal loop: waves (PDE) → moods (slow integrators) → VP + cortisol + tone bias → future eligibility/STDP damping (existing astrocyte modulation) → more fragmented future bursts → more fatigue.
- Brutalist: single linear pass for aggregates, fused if possible, all params in AstrocyteParams, MoodState read-mostly, moods included in any state dump/engram/snapshot.

Integration:
- Call update_astrocyte_moods() from the existing slow orchestration after run_astro_diffusion_step().
- In the Broca Valve pressure check: pass mood + current_cortisol to modulate_verbalization_pressure().
- When materializing the VerbalizationRequest / MinimalSnnSnapshot: attach mood fields and/or apply the tone bias.

Antigravity scope: add the MoodState to the grid struct, implement the two small functions (update + modulate + tone bias), wire the calls at the right macro points, expose mood in the snapshot. No changes to the PDE solver, cell layout, SNN injection, or core VP formula — purely additive and slow-path.

This makes the "humeur" and Cognitive Fatigue a direct, long-term consequence of the same physical Ca²⁺/IP3 process that already runs for meta-plasticity. The fast spikes provide the drive; the slow field integrates it into persistent states that can dominate tone and pressure long after the spikes are gone.

User can now make avg_ca2 a simple trigger and let the full wave history + fatigue integrator be the real causal source.