# Helheim

> **Status:** Bare-Metal, CLI-First, CUDA-Accelerated.
> **Language:** Bilingual DSL (English & Dutch) — CodeTaal.

Helheim is a high-performance native execution engine designed for low-latency computation and direct hardware control. It runs independently of web frameworks or HTTP overhead, executing AST natively on CPU and GPU.

## Design Principles

1. **No abstraction overhead:** No REST APIs, no wrappers. A Rust binary executing AST natively on CPU and GPU.
2. **Double-Buffered CUDA:** Maximizes VRAM throughput by asynchronously overlapping host-to-device memory copies with active tensor computation.
3. **Asymmetric Load Balancing (Inferno Mode):** Distributes workloads across all available Nvidia GPUs (NVRTC/PTX) and multi-core CPUs via Rayon.
4. **HSP Swarm Protocol:** Native TCP node distribution with lock-free connection pooling (`dashmap`) and XOR stream encryption.

## SNN Support (Motor Cortex)

Helheim provides a zero-overhead path for Spiking Neural Network workloads without Python interpreter or PyTorch tensor overhead.

Spikes are bit-packed as `u32` masks and lowered directly to PTX via `popc.b32` thresholding. Context binding allows host variables (e.g., `zet x=...`) to flow into GPU kernels as `.param` inputs. Results are bit-cast into a VRAM ringbuffer and unpacked to `waar/onwaar` lists on the host.

## Architecture

### 1. CodeTaal (DSL)

Helheim uses a bilingual (English/Dutch) DSL compiled to an Abstract Syntax Tree (AST).

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

### 2. PTX JIT Lowering

CodeTaal blocks are lowered directly to PTX instructions for Nvidia GPUs, with no interpreter overhead for conditionals, loops, or operations.

### 3. CLI (`helheim-cli`)

Scripts are executed from the terminal. The orchestrator dynamically evaluates the AST or JIT-compiles to PTX.

```bash
helheim script examples/snn/03_snn_cortex.hel
```

## Usage

**Interactive REPL:**
```bash
helheim repl
```

**Swarm Node:**
Start a headless worker node listening for encrypted commands via the HSP protocol.
```bash
helheim service --port 9003
```

## Documentation

- [Language Specification](docs/LANGUAGE_SPEC.md)
- [Motor Cortex — SNN JIT Engine](docs/MOTOR_CORTEX.md)
