# 🐺 HELHEIM: The Native Ascension

> **Status:** Bare-Metal, CLI-First, CUDA-Accelerated.
> **Language:** Native Bilingual (English & Dutch) — CodeTaal.

Helheim is a high-performance, strictly native inference and execution engine designed for extreme latency optimization and bare-metal hardware control. It operates completely independent of bloated web frameworks or HTTP overhead. It is the "Body" to the AI "Brain".

## 🚀 The Philosophy
1. **Zero Bloat:** No REST APIs, no heavy wrappers. Just a pure Rust binary executing AST natively on CPU and GPU.
2. **Double-Buffered CUDA:** Maximizes VRAM throughput by asynchronously overlapping host-to-device memory copies with active tensor computation.
3. **Asymmetric Load Balancing ("Inferno Mode"):** Automatically distributes workloads across all available Nvidia GPUs (NVRTC/PTX) and multi-core CPUs via Rayon.
4. **HSP Swarm Protocol:** Native TCP node distribution utilizing lock-free connection pooling (`dashmap`) and XOR stream encryption.

## 🧠 Zero-Overhead SNN on Bare Metal (Motor Cortex)
Helheim makes Python + PyTorch obsolete for specific high-performance workloads.
Spikes are bit-packed as `u32` masks and directly lowered to PTX with `popc.b32` thresholding. This executes via a true JIT `hel_lowered` entry on CUDA without Python interpreter overhead or PyTorch tensor bloat. 
Context binding allows host variables (e.g., `zet x=...`) to seamlessly flow into GPU code. Results are bit-cast into a VRAM ringbuffer and unpacked by the host to `waar/onwaar` lists.

## ⚡ The Architecture

### 1. CodeTaal (The DSL)
Helheim understands a custom, natively bilingual (English/Dutch) syntax that is compiled directly to an Abstract Syntax Tree (AST).
*Example:*
```helheim
zet input_spikes = [waar, onwaar, waar, waar];
zet gewichten = [waar, waar, onwaar, waar];
zet overlap = input_spikes & gewichten;
zet fire_count = tel_spikes(overlap);

als fire_count >= 2 dan {
    druk_af "Neuron fired!";
} anders {
    druk_af "Misfire";
}
```

### 2. Lowered Blocks & Real PTX JIT
No interpreter overhead for Block/If/Loop/Op. CodeTaal is lowered directly into highly optimized PTX instructions for Nvidia GPUs.

### 3. The Engine (`helheim-cli`)
You execute scripts directly from the terminal. The Orchestrator dynamically evaluates the AST or JIT-compiles it.
```bash
helheim run examples/snn/03_snn_cortex.hel
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

---
*Built for the Antigravity Standard. Speed, Truth, and Absolute Control.*
