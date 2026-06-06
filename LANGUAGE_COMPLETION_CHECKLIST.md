# Helheim CodeTaal - Language Completion Checklist

**Strict Rules (from user):**
- Focus ONLY on completing the programming language (Helheim / CodeTaal).
- SNN / Motor Cortex is SEPARATE. Do not mix or prioritize SNN-specific work in the language core.
- Swarm/HSP already existed and worked before. Recent changes ("veel gebouwd") may have broken integration — verify/fix as secondary, using the language.
- No scope creep: no 3 computers, no new clusters, no "further" projects until language is solid.
- Professional only: formal tone, no emojis, no hype in all docs, code comments, plans.
- Talk first. No execution without explicit user "go".
- Antigravity: execution, git hygiene, testing, docs professionalization, pushes only after approval.
- Grok: language implementation, analysis, proposing focused changes to parser/lowering/semantic/tests for general CodeTaal.
- Internal plans/checklists stay out of public GitHub.

## Current State Assessment (as of now)
- **Binary**: Exists (`helheim-cli` with run, repl, service).
- **Library**: `helheim-lang` is the core language crate (parser, ast, semantic, synthesis/lowering).
- **Language features** (bilingual):
  - Variables (zet/let), control flow (als/if/dan/anders, zolang/while, voor/for), functions (functie/fn, roep_aan/call, retourneer/return), try/catch, expressions (Op), lists/matrices, gebruik/use (partial modules), basic I/O, host ops.
  - Lowering: `lower_general` + context binding exists. Handles blocks, some control flow, expressions. FunctionDef treated as block in places. Return supported in translate_op.
  - Parser: Supports most keywords, precedence for Ops, lists/matrices.
- **Functions**: Syntax + AST + basic lowering exist and are used in examples. Not fully complete for general-purpose use (lowering often falls to block/PTX or interpreter; limited tests for pure language functions; no strong evidence of full lexical scoping/closures in general path).
- **Gaps vs other programming languages** (to be a "complete" general-purpose bilingual language):
  - Full, reliable lowering/execution for *all* core constructs in general (non-SNN) code without heavy fallbacks.
  - Robust module system (`gebruik` needs full support for separate files, namespaces, clean import in general programs).
  - Better diagnostics and error messages (parser has basic "Fout op regel", needs columns, context, better messages).
  - Dedicated unit tests for the language core (parser roundtrips, semantic errors, lowering fidelity for pure CodeTaal — existing tests lean SNN/kernel).
  - Complete function support: proper general lowering (not just block), return values in CPU path, examples/tests for general use.
  - Language-first documentation (LANGUAGE_SPEC.md should prioritize general CodeTaal; SNN as example only).
  - Tooling (VS Code syntax for CodeTaal to make language development practical).
  - Clean separation: core language should not be polluted with GPU/SNN specifics in AST/syntax/lowering.
- **Swarm**: Pre-existed (swarm.rs, SwarmEngine, hive work in executor, service command). User had it working. Needs verification after recent changes.
- **Overall**: Foundation is real (binary + lang lib + many features + lowering). Not "empty". But not yet a polished, self-contained general programming language. Functions are "enough to use" in examples but need completion for reliability. Lots of SNN focus in lowering/docs — this must be separated.

**Binary + Library status**: Yes, present and buildable. The work is to make the language complete and usable as a proper programming language.

## Prioritized Checklist (what must happen)

### Phase 0: Audit & Baseline (DO FIRST — no changes until complete)
- [ ] Full audit of current language state (parser coverage of all AST nodes, lowering paths for general vs SNN, function completeness, module support).
- [ ] Inventory: what constructs lower reliably to PTX for general code? What falls back?
- [ ] Document gaps in a clean, professional way (update LANGUAGE_SPEC.md with "Current Status" section).
- [ ] Identify minimal set of features for "complete basic programming language" (compare to simple general langs: variables, expressions, control flow, functions, basic modules, I/O, error handling).
- **Owner**: Grok (analysis + report). Antigravity: review, run builds/tests for audit, ensure professional output.
- **Deliverable**: Updated LANGUAGE_SPEC.md + short internal audit note (not public).

### Phase 1: Core Language Completion (focus here until solid)
Priority order (do in sequence, one at a time, with explicit go between):

1. **Diagnostics & Errors** (quick win, high value) - [COMPLETED]
   - [x] Improve parser + semantic errors: add column info, consistent professional messages, context.
   - [x] Test with bad input.
   - **Owner**: Grok implements. Antigravity: tests, docs update.
   - **Done when**: Errors are clear and include line+column; no more raw "Fout op regel".

2. **Functions — Make them complete for general use**
   - Ensure FunctionDef/Call/Return work end-to-end in general (non-SNN) lowering and CPU path.
   - Proper return values, scoping in pure language programs.
   - Add language-specific tests and examples (pure CodeTaal functions, not SNN).
   - **Owner**: Grok (lowering/parser/semantic work). Antigravity: test binary, update examples/docs professionally.
   - **Done when**: Functions are reliable for general programs; tests pass; examples demonstrate them cleanly.

3. **General Lowering Fidelity (separate from SNN)**
   - Strengthen lower_general / translate for all core constructs (blocks, if, loop, for, vars, lists, expressions) without SNN/bitpack assumptions.
   - Reduce interpreter fallbacks for pure language code.
   - **Owner**: Grok. Antigravity: verification with non-SNN scripts.
   - **Done when**: A pure general CodeTaal program lowers and runs reliably via PTX/CPU without heavy SNN code paths.

4. **Modules (`gebruik` / use)**
   - Make import system work for general language programs (file loading, basic namespacing).
   - **Owner**: Grok. Antigravity: tests + docs.
   - **Done when**: Can cleanly import and use code from other .hel files in general programs.

5. **Tests for the Language Core**
   - Dedicated tests in helheim-lang for parser, semantic, lowering of general CodeTaal (not SNN tests).
   - Cover functions, control flow, errors, modules.
   - **Owner**: Grok writes tests. Antigravity runs in CI/builds.
   - **Done when**: Good coverage; `cargo test -p helheim-lang` shows real language tests.

6. **Documentation — Language First**
   - LANGUAGE_SPEC.md: authoritative, professional, focused on CodeTaal as general programming language.
   - Clear section: "SNN/Motor Cortex is a separate layer that uses the language".
   - Examples: mix of general programs + SNN as usage example only.
   - **Owner**: Antigravity (professional writing + updates). Grok: technical accuracy.
   - **Done when**: Spec is complete, professional, language-primary.

7. **Tooling (VS Code for the language)**
   - Complete syntax highlighting, basic support for CodeTaal.
   - **Owner**: Antigravity (if language priority). Grok: provides grammar if needed.
   - **Done when**: Usable in VS Code for writing language code.

### Phase 2: Separation & Polish (after core is solid)
- Audit and separate any SNN/GPU-specific syntax or lowering from core language AST/keywords.
- Ensure SNN examples are "users of the language", not definers.
- Full audit that binary + library support a complete language experience.

### Phase 3: Swarm Verification (secondary, because pre-existing)
- Verify existing swarm works with current language + lowered SNN + dashboard.
- Fix integration breakage from recent builds.
- Professional docs on "using the language with swarm".
- **Owner**: Antigravity leads verification + fixes (after language phases). Grok reviews language side.
- Do **not** expand swarm as new primary work.

**Overall Done Criteria for "Language Af"**:
- Binary runs general CodeTaal programs reliably.
- Library exposes clean parser/lowering for general use.
- Functions, control flow, modules, expressions all solid in general path.
- Good tests + professional language-first docs.
- Clear separation from SNN.
- No critical gaps vs basic general-purpose language.

## Process Checklist (always)
- [ ] User reviews and gives explicit "go" before any phase or task.
- [ ] Changes are small, reviewable.
- [ ] Antigravity: build, test (language + swarm verification), professional docs, git only approved work.
- [ ] Grok: proposes language-only changes, keeps focus.
- [ ] All public output professional (no emojis, formal).
- [ ] Report back with facts after each step.
- [ ] Revisit this checklist after each phase.

**First thing that must happen**:
Phase 0 — Full audit + baseline in LANGUAGE_SPEC.md. This gives us the real checklist of gaps. Do not start implementing features until audit is done and user approves the gaps list.

This is the focused, language-only checklist. Send to Antigravity only after your review. No other plans or directions.

What is your explicit "go" and first task? (E.g., "go, start Phase 0 audit".)