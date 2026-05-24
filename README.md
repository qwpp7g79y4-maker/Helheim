# 🐺 HELHEIM: The Native Ascension

> **Status:** Bare-Metal, CLI-First, CUDA-Accelerated.
> **Language:** Dutch-Based Abstract Syntax (CodeTaal).

Helheim is a high-performance, strictly native inference and execution engine designed for extreme latency optimization and bare-metal hardware control. It operates completely independent of bloated web frameworks or HTTP overhead. It is the "Body" to the AI "Brain".

## 🚀 The Philosophy
1. **Zero Bloat:** No REST APIs, no heavy wrappers. Just a pure Rust binary executing AST natively on CPU and GPU.
2. **Double-Buffered CUDA:** Maximizes VRAM throughput by asynchronously overlapping host-to-device memory copies with active tensor computation.
3. **Asymmetric Load Balancing ("Inferno Mode"):** Automatically distributes workloads across all available Nvidia GPUs (NVRTC/PTX) and multi-core CPUs via Rayon.
4. **HSP Swarm Protocol:** Native TCP node distribution utilizing lock-free connection pooling (`dashmap`) and XOR stream encryption.

## ⚡ The Architecture

### 1. CodeTaal (The DSL)
Helheim understands a custom, Dutch-based syntax that is compiled directly to an Abstract Syntax Tree (AST).
*Example (`test_logic.hel`):*
```helheim
zet a = 10;
als a > 5 dan {
    voer uit "echo 'Helheim is wakker!'";
}
```

### 2. The Engine (`helheim-cli`)
You execute scripts directly from the terminal. The Orchestrator (`src/orchestra/mod.rs`) dynamically evaluates the AST.
```bash
helheim run test_logic.hel
```

### 3. GPU "Inferno" Mode
Run multi-device matrix multiplications directly. Helheim dynamically detects Blackwell (sm_100+) vs Turing architectures and compiles C++ PTX kernels on the fly.
```bash
helheim run "gpu work 4096"
```

## 🛠️ Usage

**Interactive REPL:**
```bash
helheim repl
```

**Deploy as Swarm Node:**
Start a headless worker node that listens for encrypted commands via the HSP protocol (No TCP Handshake latency).
```bash
helheim service --port 9003
```

**Dispatch to Swarm:**
```bash
helheim run "stuur 'voer uit ls -la' naar 192.168.1.100:9003"
```

---
*Built for the Antigravity Standard. Speed, Truth, and Absolute Control.*
