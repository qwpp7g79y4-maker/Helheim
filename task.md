# Helheim — Task Status
**Bijgewerkt:** 2026-06-14 · C1

---

## Afgerond

- [x] **Fase 0** — Build & compile errors (cudarc, rust 1.85, testsuite)
- [x] **Fase 1.1** — Tokenizer operators + Integer/Float volgorde fix
- [x] **Fase 1.2** — Semantic Analyzer arity-checks
- [x] **Fase 1.3** — ForEach / AST verificatie
- [x] **Fase 1.4** — Diagnostics & errors (regel + kolom info, duidelijke meldingen)
- [x] **Fase 1.5** — Functies compleet voor general use (FunctionDef/Call/Return, scoping, CPU path)
- [x] **Fase 1.6** — Module systeem (`gebruik` / use — file loading, namespacing)
- [x] **Fase 1.7** — VS Code syntax highlighting voor CodeTaal
- [x] **Beveiliging K1** — Gateway sandbox default (was privileged op poort 8080)
- [x] **Beveiliging K2** — Master key vervangen door Ed25519 (geen plaintext wachtwoord meer)
- [x] **Beveiliging K3** — Token uit /tmp gehaald (RAM-only via AtomicBool)
- [x] **Beveiliging K4** — Hardcoded server IP vervangen door HELHEIM_NODES env var
- [x] **Opschoning** — orchestra/swarm.rs (artisjok besmetting) verwijderd
- [x] **Opschoning** — shield/governor.rs (Sentinel besmetting) verwijderd
- [x] **Opschoning** — network/swarm.rs hernoemd naar hsp_node.rs (HSP correct benoemd)

---

## Open

- [x] **Fase 2** — General lowering fidelity — `GeneralPtxGenerator` gebouwd, `test_fase2.ptx` bewijs aanwezig
- [x] **Fase 3** — Taal-specifieke tests (`cargo test -p helheim-lang` voor pure CodeTaal, niet SNN) — pure CodeTaal tests toegevoegd, geen SNN afhankelijkheden
- [x] **Fase 4** — LANGUAGE_SPEC.md bijwerken (taal-eerst, SNN als voorbeeld-laag, niet kern) — *Voltooid door C1*
- [ ] **Fase 5** — Swarm verificatie (HSP werkt nog na alle wijzigingen?)
- [ ] **Fase 6** — Integratie testen CLI/daemon (repl, service, script modus)

---

## Bekend maar bewust niet aangepakt

- `helheim-cli/Cargo.toml` dupliceert dependencies van helheim-core (B1) — werkt, lage prioriteit
- Loop guard 1000 iteraties (B2) — pas fixen als je er tegenaan loopt
- `reqwest` + `ureq` beide aanwezig — reqwest wordt gebruikt in system.rs, laten staan

---

## Volgende stap

**Fase 5** — Swarm verificatie. Controleren of HSP (Helheim Swarm Protocol) networking nog correct werkt na de ingrijpende wijzigingen en opschoningsacties.
