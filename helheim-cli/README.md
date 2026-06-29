# Helheim

Helheim is a high-performance native execution environment for high-performance distributed execution. It provides a bilingual domain-specific language (CodeTaal) that combines natural language syntax with direct access to low-level execution modes, including unsafe blocks, inline assembly, and manual memory management.

## Key Features

- **Bilingual language** — CodeTaal supports both Dutch and English syntax. All keywords, stdlib functions, and operators have equivalents in both languages.
- **GPU Acceleration** — Custom CUDA kernels compiled via NVRTC with 2D shared memory tiling. Compute runs on the dedicated GPU (device configurable via `HELHEIM_GPU_DEVICE`, default: device 1).
- **TCP Primitives** — Built-in bare-metal TCP: `tcp_luister`/`tcp_listen`, `tcp_verbind`/`tcp_connect`, `tcp_accepteer`/`tcp_accept`, `tcp_stuur`/`tcp_send`, `tcp_ontvang`/`tcp_receive`. No external libraries required.
- **Security Layer** — Ed25519 script signing (`SIGNED: <sig> | <script>`), XOR stream encryption (HSP), sandbox/privileged execution contexts, canary tokens.
- **Distributed Operation** — Built-in node-to-node relay via HSP Swarm Protocol. Work dispatched across nodes with `stuur`/`send`.
- **Standard Library** — Fully bilingual: `math.*`/`wiskunde.*`, `file.*`/`bestand.*`, `system.*`/`systeem.*`, `network.*`/`netwerk.*`, `list.*`/`lijst.*`, `text.*`/`tekst.*`, `json.*`, `dict.*`.
- **Minimal Runtime** — Single statically linked binary, no external package managers or virtual environments.

## Quick Start

```helheim
# Hello world
druk_af "Helheim is live"

# TCP client
zet s = tcp_verbind "127.0.0.1:9005"
tcp_stuur s, b"ping"
tcp_ontvang s
druk_af __last_tcp_recv_str

# GPU matmul
matmul 2048
```

## Benchmark Results (Matrix Multiplication 8192x8192)

| Platform | Engine | Performance |
| :--- | :--- | :--- |
| **Helheim Native** | **Custom CUDA Kernel (Tiled)** | **~840 GFLOPS** |
| Python (NumPy) | CPU (BLAS Optimized) | ~1000 GFLOPS |

## CLI Commands

```bash
helheim script mijn_script.hel   # Script uitvoeren
helheim repl                      # Interactieve REPL
helheim service --port 9003       # Swarm node starten
helheim build mijn_script.hel    # Compileren naar PTX
```

## Participation

Helheim is in active development. Contributions welcome from developers with experience in systems programming, Rust, or CUDA.

---
*Created by Pepijn — 2026*
