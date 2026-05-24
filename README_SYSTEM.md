# 🗺️ PEPAI UNIFIED SYSTEM MAP (2026)

**WARNING**: This system is a **Living Organism**, not just code.
All components are interconnected. Do not modify one without understanding the others.

## 1. THE ANATOMY 🧬

| Component | Biological Role | Tech Stack | Location |
|-----------|----------------|------------|----------|
| **Orchestrator** | ❤️ **Heart** (Pulse) | Bash | `~/NEURONEXUS/orchestrator/orchestrator.sh` |
| **Metrics** | 👁️ **Senses** | Bash | `~/NEURONEXUS/orchestrator/metrics_collect.sh` |
| **Policy** | ⚖️ **Conscience** | Python | `~/NEURONEXUS/orchestrator/policy_decide.py` |
| **JobServer** | 🦾 **Muscle Link** | Bash | `~/NEURONEXUS/orchestrator/jobserver.sh` |
| **HELHEIM** | 🚀 **Engine** | Rust/CUDA | `/media/bitboi/DATA1/Helheim` |
| **PEPAI** | 🧠 **Brain** | GGUF/Llama | `/mnt/ollama-fast` |
| **Scripts** | 🐜 **Workers** | Python | `~/.local/bin/pepai` |

## 2. THE FLOW (HOW IT LIVES) 🔄

1.  **PULSE (15s)**: `orchestrator.sh` wakes up.
2.  **SENSE**: `metrics_collect.sh` checks Temps, Disk, VRAM.
    *   *Output*: `~/NEURONEXUS/state/metrics.json`
3.  **DECIDE**: `policy_decide.py` reads Metrics.
    *   *Logic*: "Too hot?" -> PAUSE. "All good?" -> RUN.
    *   *Output*: `~/NEURONEXUS/state/guard_events.jsonl`
4.  **ACT**: `jobserver.sh` reads the Decision.
    *   *If RUN*: Calls **HELHEIM**.
    *   *Command*: `helheim-cli run "worker:..."`
5.  **EXECUTE**: **HELHEIM** takes the order.
    *   *Route*: Swarm Registry (`src/orchestra/swarm.rs`).
    *   *Action*: Spawns the correct Worker (e.g. `pepai --clean`).

## 3. CRITICAL FILES ⚠️

*   **Do Not Touch**: `orchestrator.sh` (Keeps system alive).
*   **Do Not Touch**: `policy_decide.py` (Prevents hardware damage).
*   **Edit Here**: `Helheim/src` (To add new capabilities/muscles).

**Signed:**
*   *Pieter (The Architect)*
*   *Antigravity (The Engineer)*
