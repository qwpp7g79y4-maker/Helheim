# First-Class Namespaces & StdLib Integration Blueprint

**Project:** Helheim CodeTaal  
**Version:** 1.1 (Nulmeting + Refactor)  
**Date:** 2026-06  
**Status:** Actionable Implementation Spec  
**Owner:** Helheim Core Team  
**References:** 
- stdlib_architecture_blueprint.md (core philosophy)
- tcp_primitives_blueprint.md
- HANDOFF.md (current debt)
- ffi_module_system_blueprint.md

## Goal
Make namespaces **first-class** in the language and runtime so that:
- `gebruik "stdlib/pure/net.hel" als Net`
- `perform Net.luister "0.0.0.0:8080"`
- `Net.http_get ...`
- Qualified stdlib calls (`http::get`, `sqlite::open`) work with proper semantic analysis, arity checking, and effect dispatch.
- No more string-prefix hacks that break analysis and distributed execution.

This must align 100% with the "Primitives over Built-ins" and external StdLib design.

## Current State (Nulmeting - June 2026)
- `Gebruik` + `als` currently does a shallow flatten: turns into `"Net::luister"` string in `memory.ast_funcs`.
- This bypasses `SemanticAnalyzer` for qualified names.
- Effect dispatch (`Perform`) and `FunctionCall` rely on string matching → brittle.
- `webserver_demo.hel` works by accident via the hack.
- Violates the design in `stdlib_architecture_blueprint.md` sections 4 (Namespace Injection) and 5 (Current State & Refactoring Path).
- The architecture debt is real and documented.

## Core Requirements
1. Namespaces are compile-time + runtime constructs (not string munging).
2. StdLib modules (pure .hel + FFI) are discovered, linked, and injected with proper qualified names.
3. Semantic analysis validates arity and types for `ns::name`.
4. Executor / effect system does O(1) or fast qualified lookup.
5. Distributed / persistence / `ast_json` remains correct (qualified names travel safely).
6. Backward compat for a short transition if needed, then remove the old hack.

## Architecture (aligned with existing blueprint)

### 1. AST Changes
- Keep `CodeTaal::Gebruik { path, namespace: Option<String> }`.
- Optionally introduce a `CodeTaal::QualifiedCall { ns: String, name: String, args }` or let the linker produce properly scoped nodes.
- `Perform { effect, operation, args }` should carry the namespace if the effect is namespaced.

### 2. Parser
- When seeing `gebruik "foo.hel" als Net` record the alias.
- Support both `Net.foo` and `foo` (with alias in scope) in expressions and `perform`.

### 3. SemanticAnalyzer (Compile-time)
- Add `register_qualified(ns: &str, name: &str, ty: TypeInfo)` to the symbol table.
- On `Gebruik`, load the module (via linker), register all its public symbols under the namespace.
- On use of `Net.bar(...)`, resolve as qualified and do arity/type check.
- Same for `perform Net.bar ...` (effects can be namespaced).

### 4. ModuleLinker + StdLibManager
- `ExpandedStdModule` already has `namespace`.
- Stop the "prefix everything into one flat string" path.
- During bootstrap (in `Orchestrator::bootstrap` or dedicated `StdLibManager::bootstrap`):
  - For each pure module: expand, then register under its namespace in a nested structure or `DashMap<String, DashMap<String, ...>>`.
  - For native FFI: register the table under the ns prefix.

### 5. MemoryManager & Executor (Runtime)
- New or extended storage:
  ```rust
  // Example structure
  pub struct StdFunctions {
      pure: DashMap<String, DashMap<String, (Vec<String>, CodeTaal)>>, // ns -> name -> (params, body)
      native: DashMap<String, DashMap<String, HelFunctionCall>>,
  }
  ```
- In `execute_ast` for `FunctionCall` or `Perform`:
  - If the call is qualified (`ns::name`), do direct lookup in the ns map.
  - Fall back to current global for non-namespaced legacy during transition.
- For effects: the effect name itself can be namespaced (`perform Net.luister`).

### 6. Resource & Security
- Namespaced stdlib functions inherit the caller's `ExecutionContext` (sandbox/privileged).
- FFI native modules keep their existing sandboxing.

## Implementation Phases (do in this order)

**Phase 0 — Preparation (now)**
- This blueprint is the source of truth.
- Update HANDOFF.md to point here.
- Add a small test .hel that uses proper qualified stdlib once implemented.

**Phase 1 — Semantic & Symbol Table**
- Implement `register_qualified` + qualified resolution in SemanticAnalyzer.
- Make `Gebruik als Ns` register the module's symbols qualified.
- Update parser if needed for `Ns.name` syntax in calls.

**Phase 2 — Linker & Bootstrap**
- Evolve ModuleLinker to preserve module boundaries instead of flattening.
- Wire StdLibManager (or equivalent) into bootstrap so pure + native modules are pre-registered under their ns.

**Phase 3 — Executor & Memory**
- Add the ns-aware function tables.
- Update the match arms for `FunctionCall`, `Perform`, and any direct `roep_aan` / effect paths.
- Remove or gate the old string-prefix hack behind a flag or deprecation.

**Phase 4 — Effects + Distributed**
- Ensure `perform Net.foo` resolves correctly.
- Make sure qualified names survive `ast_json` serialization for Concurrent/distributed islands.
- Test cross-island calls that use stdlib (should be local or explicitly shipped).

**Phase 5 — Cleanup & Docs**
- Remove the old hack completely.
- Update CHEATSHEET.md, LANGUAGE_SPEC.md, README.
- Add examples using real namespaces (`gebruik "stdlib/pure/net.hel" als Net`).
- Mark the tcp_primitives and stdlib architecture blueprints as "implemented per this namespace spec".

## Success Criteria
- `gebruik "..." als Net; perform Net.luister ...` passes semantic analysis and runs.
- Arity errors are reported at compile time for namespaced calls.
- No string hacks left in executor for stdlib dispatch.
- Existing pure stdlib (net, http) and FFI plugins work without change to user code.
- Distributed execution and persistence continue to work.

## Open Items (for later)
- Hot reload of stdlib modules.
- Capability restrictions per namespace.
- Auto-prelude with safe core namespaces.

This blueprint closes the gap between the high-level architecture vision and the current implementation debt. Once Phase 3 is done, higher-level work (full TCP/HTTP stdlib demos, webserver on effects, etc.) can resume safely on a solid foundation.

Ready for implementation. Give the signal and we start Phase 1.
