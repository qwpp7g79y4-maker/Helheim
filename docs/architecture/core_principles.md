# Helheim: Architectural Core Principles

This document defines the invariant constraints and technical identity of the Helheim system. These guidelines govern all contributions and architectural decisions.

## 1. System Identity
Helheim is a native executable framework designed as a low-level, high-performance execution engine. It operates directly on bare-metal hardware and GPUs to process distributed workloads, utilizing a specialized Dutch-based Domain Specific Language (DSL).

## 2. Invariant Architectural Constraints
- **Hardware Agnosticism:** The core execution engine must compile and run on environments lacking NVIDIA GPUs (e.g., macOS, generic Linux/Windows environments). 
- **Isolated GPU Dependencies:** CUDA and associated PTX backends must remain strictly gated behind the `--features cuda` flag. The default compilation path must rely on the CPU fallback backend (e.g., Rayon).
- **Dependency Minimization:** The integration of monolithic mathematical libraries (such as `openblas` or `ndarray-linalg`) in the core engine is prohibited to prevent linking failures across cross-platform toolchains.
- **PTX Primacy:** GPU operations must target NVIDIA PTX code directly via the internal `synthesis.rs` engine. WebGPU is strictly reserved for client-side visualizations (e.g., the dashboard) and must not be integrated into the core execution engine.

## 3. Modular Workspace Structure
- `helheim-core`: The foundational engine. Contains the Abstract Syntax Tree (AST), the execution engine, state management (incorporating RAII `ScopeGuard`), and the PTX JIT compiler.
- `helheim-lang`: The lexical analyzer, parser, and semantic validation layer. Translates the DSL into the AST structure.
- `helheim-gateway`: The REST/WebSocket API layer (Axum). Exposes the execution endpoints and streams real-time data to connected clients.
- `helheim-cli`: The command-line executable acting as the primary entry point for executing scripts and standalone nodes.
- `helheim-dashboard`: The visual client interface for monitoring internal engine state.

## 4. Distributed Execution Protocol
Helheim relies on the Helheim Swarm Protocol (HSP) for distributed execution. Workloads can be asynchronously delegated across peer nodes. The internal data model prioritizes high-speed, binary state representations to accurately orchestrate complex workloads across the cluster.
