# Changelog
All notable changes to Helheim are documented here.
Format: [Semantic Versioning](https://semver.org/)

## [0.1.0] — 2026-06-20
### Added
- Iteratieve TrampolineStack executor (geen stack overflow bij diepe nesting)
- Gas/Fuel systeem (AtomicU64, OUT_OF_GAS bescherming)
- WASM FFI sandbox via Wasmtime (vervangt libloading)
- HSP security: ECDH per-sessie + Ed25519 signatures + Lamport replay protection
- Actor supervisor met 4-level escalatie
- Continuations + gedistribueerde teleportatie (Swarm::migrate)
- PackageManager met Ed25519 package signing + path traversal bescherming
- SystemManager sandbox met symlink jail
- Flight Recorder (zero-overhead tracing)
- Panic-vrije productiecore (alle unwrap() vervangen)
- Panic-vrije parser bij malformed gebruikersinvoer
### Security
- SSRF bescherming in PackageManager
- Sandbox enforcement op inkomende continuations (is_privileged flag)
- ABI version check bij WASM module laden
