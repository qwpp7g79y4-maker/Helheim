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

**For Antigravity**: Do not begin any work on the above until the user explicitly approves a scoped starting point and says "execute".

This document is the controlled handoff. Keep all internal discussion out of public commits.

*Prepared after user request following the previous plan execution and GitHub cleanup discussion.*