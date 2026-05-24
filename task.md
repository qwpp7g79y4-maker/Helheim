# PEPAI MOM - Implementation Task List

## Phase 1: Foundation (Week 1-2) ✅ COMPLETED
- [x] Create PEPAI project structure (Rust in /mnt/ollama-fast/pepai)
- [x] Implement Jan's Regels as Rust module (10 axioms with verification)
- [x] Build MOM State Machine (Motive/Operation/Model)
- [x] Create Cognitive Profile Handler (Architect/Strategist/Analyst/Synthesist/Explorer)
- [x] Implement Regelaar (manager/orchestrator)
- [x] Test with live Ollama integration

## Phase 2: Model Integration ✅ COMPLETED
- [x] Connect PEPAI to Ollama API (via subprocess)
- [x] Implement Regelaar with Estafette Strategy (Sequential GPU use)
- [x] Switch to Uncensored `dolphin-llama3:8b`
- [x] Implement Error Logging (`logger.rs`)
- [x] Fix Ollama Storage & Permissions (NVMe)

## Phase 3: Knowledge & Memory (Week 3-4) - IN PROGRESS
- [x] **Phase 3A:** Short-term Memory (Context Window via `memory.json`)
- [x] **Phase 3B:** Long-term Vector Memory (RAG)
    - [x] Install Local Vector DB (Qdrant on Docker)
    - [x] Create Manual Ingestion (CLI: `pepai remember`)
    - [x] Implement Semantic Search (CLI: `pepai recall`)
    - [x] Integrate Memory into Main Chat Loop (RAG Active)
- [/] **Phase 3C:** "The Cortex" Ingest System
    - [x] Create Inbox Directory (`/mnt/ollama-fast/pepai_inbox`)
    - [x] Implement File Router (Detect .py, .sh, .txt)
    - [x] Implement Robust Chunking (Handle large files)
    - [x] Build Multi-Collection Architecture (Forked Memory)
    - [x] **God Mode Implementation**
    - [x] Add toggle switch to UI (Frontend)
    - [x] Update `regelaar.rs` to support raw prompt override
    - [x] Update `server.rs` to handle `god_mode` flag
    - [x] Verify bypass of "The Judge"
    - [x] Enforce "Truth" via Prompt Engineering (Solve Model Bias)
    - [x] Implement Interactive Chat Mode (REPL via `pepai.sh`)
    - [x] Basic Gatekeeper (Relevance Filter)
    - [x] Auto Ingest Loop ("Snake Mode")
    - [ ] **Address Technical Debt**
        - [ ] Update `qdrant-client` (Deprecation warnings)
        - [ ] Remove unused code (Compiler warnings)
    - [x] **Ingest PEPAI Philosophy:** (Framework, MOM Suite, Specs)
    - [x] **Fix Classification Bug:** Teach AI to recognize 'Policy' as [KERN] not [FICTIE].
    - [x] **Deep Memory Nuke:** Physically removed Qdrant collection files (rm -rf storage).

### Phase 4: Optimization & Robustness (Current)
- [x] **Hardware Optimization:** Upgraded to `qwen2.5-abliterated` (7B) with 4096 context.
- [x] **Refactor God Mode:** Logic moved AFTER RAG context retrieval.
  - [x] **Fix "Generic Answers":** Prompt now includes Memory [KERN]/[CODE].
  - [x] **Fix "Weak Bypass":** Prompt hardened with "Nuclear" Dutch instruction. Judge Skipped.
  - [x] **Fix "Identity Trap":** Disabled `is_identity_question` check when God Mode is ON.

### Phase 5: Operation Antigravity (Smart Knowledge)
- [x] **Specs Injection:** Create `inbox/s7_scapy_specs.txt` with correct technical definitions (TPKT/COTP).
- [x] **Ingest:** Feed knowledge to RAG.

### Phase 7: UI Redesign (Professional)
- [x] **Grok/OpenAI Style:** Refactor `index.html` to white/clean theme.
- [x] **Layout:** Center chat, clear header "PEPAI".
- [x] **Visualizer:** Maintain 3D Brain but integrate elegantly.

### Phase 8: The Black Hole Sorter (Phi-3)
- [x] **Model:** Pull `phi3:mini` (2GB) for fast classification.
- [x] **UI:** Add "Irreversible" warning under Absorb button.
- [x] **Scrolling Bug:** Fix `overflow-y` issue in main container.
- [x] **Backend:** Implement `check_memory_worth()` in Rust.
- [x] **Logic:** If Sorter says "NO", reject. If "YES", ingest.
- [x] **Future:** Archived user LoRA snippet to `inbox/black_hole_snippet.rs`.

### Phase 10: Branding & Animation
- [x] **Asset:** Create `pepai_icon.png` (Crop Brain + Transparent BG).
- [x] **Design:** Replace 'P' icon with Brain Logo.
- [x] **Animation:** Implement "Spin on Think" (HTMX event listeners).
- [x] **New Chat:** Add button to clear memory (`/new` endpoint).
- [x] **Feedback:** Add "Thinking..." bubble with pulse animation.
- [x] **Persistence:** Render history on load (Sidebar).

### Phase 12: Knowledge Injection (Massive Intelligence) 🧠
- [x] **Strategy:** Prioritize MathPile (Reasoning) & OpenWebMath (Logic).
- [x] **Pipeline:** Create `tools/ingest_knowledge.py` matching Rust chunking.
- [x] **Execution:** Ingest "Reasoning Booster Pack" (Synthetic/Sample).
- [x] **Optimization:** Ensure 8GB VRAM safety (Batching implemented).

### Phase 13: Memory Purge (Safety & Control) ✅ COMPLETED
- [x] **Logic:** Implement `DELETE` endpoint in `vector_store.rs` / `server.rs`.
- [x] **UI:** Add "Delete" button to visualizer & sidebar.
- [x] **Risk Check:** Implement confirmation dialog.

### Phase 14: Session Manager & Feedback (User Control) ✅ COMPLETED
- [x] **Sessions:** Implement Save/Load/List/Delete API.
- [x] **UI:** Add Sidebar Session List & Controls.
- [x] **Feedback:** implement `/api/feedback` and UI interaction.

### Phase 15: Codebase Consolidation & Logic Integration 🛠️ ✅ COMPLETED
- [x] **Vector Store:** Upgrade `qdrant-client` calls / Suppressed Deprecation Warnings.
- [x] **Judge Integration:** Wired `TheJudge` into `regelaar.rs`.
- [x] **Cleanup:** Removed unused imports in `server.rs`, `regelaar.rs`, etc.
- [x] **Verification:** Zero-warning build (via `cargo check`).

### Phase 16: Adaptive Framework (Next Level) 🚀
- [x] **Context Fix:** Verified `short_term_context` injection (CRITICAL FIX - CONFIRMED).
- [x] **Reflection Loop:** Logic in `judgement.rs` to warn on relevance drift.
- [x] **Dynamic Alignment:** Created `UserPreferences` struct & wired to System Prompt.
- [x] **Stability:** Cap `get_recent_context` (12000 chars) to prevent VRAM OOM.

### Phase 9: Hardening & Optimization (Previous)
- [ ] **Prompt Hardening:** `regelaar.rs` updated with "EXACT CODE" and "NO EXCUSES" directive.
- [ ] **RAG Optimization:** `main.rs` updated to Sliding Window Chunking (Size 512 / Overlap 128).
- [ ] **Verification:** Re-ingested effective knowledge. Verified "Golden Yaml" retrieval: 100% Match.
- [ ] **Model Upgrade (Pending):** User requested `nous-hermes2-mixtral-8x7b-q4_K_M` (Warning: VRAM Risk).
    - [x] **Identity Lockdown:** Hardened `regelaar.rs` system prompt to override AI's internal bias.
    - [x] **Final Policy Test:** PASSED. Response: "Mijn eigenaar is Bitboi." (RAG Shield active).
    - [/] **Optimization:** "Fast Lane" for Bulk Ingestion (Skip Filter for huge files).

- [ ] **Phase 5: Advanced Optimization (Speed & Scale)**
     - [ ] **Fast Lane:** Implement logic to skip LLM classification for obvious code/logs to speed up bulk ingest.
     - [/] **Hardware Optimization:** Switched to lightweight/speed focus (8B models).
     - [ ] **Conflict Detection:** The "Judge" Agent (Checking for contradictions).

- [ ] **Phase 6: Visual Interface (The Face of MOM)**
    - [ ] **Design:** Simple, Dark Mode, Terminal-aesthetic web UI.
    - [ ] **Backend:** Rust Actix-web + Audio Ingestion (Whisper/Python bridge).
    - [ ] **Frontend:** HTMX + Tailwind + Drag & Drop Ingestion.
    - [ ] **Tools:** Relocated to `/mnt/ollama-fast/pepai/tools`.
    - [ ] **Visuals:** Real-time state visualization (Brain activity graph).
      - [ ] **Sherlock Holmes**: Magic Byte detection voor correcte bestandsherkenning
      - [ ] **Vision (Llava)**: Ogen voor PEPAI (Image-to-Text ingestion)

- [ ] **Autonomous Memory Consolidation** (Phase 5)
  - [ ] AI samenvatting van eigen logs (Self-Reflection loop)
    - [ ] "Eject Button" (Privacy Safe Wipe)
- [ ] Implement persistent memory system
- [ ] Create reasoning trace storage
- [ ] Build knowledge base queries (30GB in RAM)
- [ ] Test fact verification against knowledge base

## Phase 4: Meta-Cognition (Week 5)
- [ ] Implement Observability Layer
- [ ] Create meta-logging system
- [ ] Build self-reflection prompts
- [ ] Implement Sentinel Nodes (error detection)
- [ ] Create System Health Monitor
- [ ] Test "why did you do this?" functionality

## Phase 5: Polish & Optimization (Week 6)
- [ ] Optimize RAM usage (50GB limit)
- [/] GPU allocation tuning (RTX 3060 + RTX 2060)
- [ ] CPU threading optimization (32 threads)
- [ ] Create CLI interface
- [ ] Write user documentation
- [ ] Final integration testing

## Optional: Advanced Features
- [ ] Finetuning dolphin-llama3 with PEPAI data
- [ ] Web interface (local only)
- [ ] Voice interface
- [ ] Export/import conversations
