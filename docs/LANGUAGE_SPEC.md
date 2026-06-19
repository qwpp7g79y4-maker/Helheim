# CodeTaal — Language Specification v2.0
**[A·C1·AF]**
**Auteur:** Duurt Jan Pepijn de Jonge
**Bijgewerkt:** 2026-06-14 door C1 — gebaseerd op werkelijke broncode in `helheim-lang/src/`

---

## 1. Wat is CodeTaal

CodeTaal is een tweetalige (Nederlands/Engels) programmeertaal gebouwd op het Helheim runtime.  
De taal compileert via een Abstract Syntax Tree (AST) naar:

- **PTX** — NVIDIA GPU assembly (sm_80, Ampere)
- **CPU executor** — directe interpretatie voor host-side operaties

CodeTaal is een **algemene taal**. GPU-compute en tensors zijn toepassingslagen — geen kern.

---

## 2. Compilatiepijplijn

```
Broncode (.hel)
    ↓
Tokenizer         — karakterstroom → token stroom (Ident, Number, Op, ...)
    ↓
Parser (HelParser) — Pratt parser — token stroom → CodeTaal AST
    ↓
Resolver          — module expansie (gebruik/use → code inladen)
    ↓
Semantic Analyzer — arity checks, scope validatie, diagnostics
    ↓
Synthesis Engine  — AST → PTX of CPU executor pad
    ├── KernelSynthesisEngine  — GPU kernel pad (PtxGenerator, sm_80)
    └── GeneralPtxGenerator    — Algemeen lowering pad (Fase 2, .func blokken)
```

---

## 3. Taalconstructies

### 3.1 Variabelen

```
zet naam = waarde;
let naam = waarde;
set naam = waarde;
```

Geen typeannotatie vereist — type is sterk getypeerd in de AST (`Int`, `Float`, `String`, `Bool`, `List`).

### 3.2 Functies

**Definitie:**
```
functie naam(param1, param2) {
    ...
    geef_terug waarde;
}
```
Aliassen definitie: `fn`, `func`, `function`, `met`, `with`.  
Aliassen terugkeer: `retourneer`, `return`.  
Functieparameters leven in eigen scope. Geneste functies worden geëxtraheerd vóór lowering.

**Aanroep:**
```
roep_aan naam(arg1, arg2);
call naam(arg1, arg2);
```

**PTX output:** `.func (.reg .f64 %ret) naam (%f0, %f1) { ... }` (GeneralPtxGenerator)

### 3.3 Conditionals

```
als conditie dan {
    ...
}
anders {
    ...
}
```

Aliassen: `if` / `else`. `dan` / `then` zijn optioneel als er direct een `{` volgt.  
**PTX output:** `setp.ne.f64 %p0, ...; @%p0 bra then_0; bra else_0;`

### 3.4 Lussen

**While loop:**
```
zolang conditie dan {
    ...
}
```
Aliassen: `while`, `repeat`.  
**PTX output:** label + `setp.eq.f64 %p, cond, 0; @%p bra loop_end; bra loop_start;`

**For-each loop:**
```
voor elke item in lijst {
    ...
}
```
Alias: `for each item in list`.  
AST: `ForEach { iterator, iterable, body }` — iterable is een volledige expressie.

### 3.5 Modules & First-Class Namespaces

```
gebruik "bestandsnaam";
use "bestandsnaam";
import "bestandsnaam";
```

Compile-time module expansie via `Resolver`. Het bestand wordt ingeladen en zijn AST wordt samengevoegd vóór semantic analyse.  
Na de import zijn de functies en variabelen uit die module beschikbaar via **gekwalificeerde namen** (namespaces).
Voorbeeld voor functies: `roep_aan Std::IO::Tcp::stuur(s, "data")`. Voor effecten: `perform Swarm::migrate("ip", 80)`. De oude string hacks (`gebruik "x" als Y`) zijn verwijderd.

### 3.6 Foutafhandeling

```
probeer {
    ...
} vang fout {
    ...
}
```
Alias: `try` / `catch`. Optionele foutvariabele (`error_var`).

### 3.7 Concurrent blokken

```
concurrent {
    stap_a;
    stap_b;
}
```
AST: `Concurrent { statements }` — parallelle uitvoering, scheduling is runtime-verantwoordelijkheid.

### 3.8 Daemon

```
daemon {
    ...
}
```
AST: `Daemon { body }` — achtergrondproces, uitvoering buiten de hoofdstroom.

### 3.9 Hel-blokken (bare metal)

```
hel {
    // Ruwe CUDA/PTX code hier
}
```
AST: `HelBlock { raw_code }` — wordt rechtstreeks doorgegeven aan NVRTC/executor. Geen parsing.

### 3.10 Inline Assembly & CPU Fallback

Volledige integratie van de hardware assembly pipeline met optionele CPU fallback:
```
asm ptx in(a=a, b=b) out(c) clobber("memory") {
    "add.u32 %0, %1, %2;"
} fallback {
    zet c = a + b;
    druk_af "[CPU Fallback] Geen GPU gevonden, software emulatie gebruikt!";
};
```
Ondersteunt `in(..)`, `out(..)` en `clobber(..)`. Deze inputs en outputs worden tijdens de semantic analysis gevalideerd en veilig gekoppeld aan de locale variabelen.
Het optionele `fallback { ... }` of `terugval { ... }` blok wordt automatisch door de executor uitgevoerd als de GPU niet beschikbaar is, of als het gevraagde target (bijv. `x86`) niet ondersteund wordt op de huidige architectuur. Dit garandeert hardware-onafhankelijke code-uitvoering en voorkomt "no-op" data corruptie of crashes.

### 3.11 Effecten en Continuations (Algebraic Effects)

Helheim ondersteunt first-class algebraic effects (het scheiden van *wat* je wilt doen van *hoe* het uitgevoerd wordt).

**Definitie & Handler:**
```
effect Migratie {
    voor_vertrek,
    na_aankomst
}

handle Migratie {
    voor_vertrek => {
        // ... resource cleanup
        hervat("ok");
    }
} in {
    // Perform een effect. Ondersteunt nu ook namespaced effecten!
    zet result = perform Swarm::migrate("ip", 80);
    druk_af "Hervat met waarde: " + result;
}
```

**Continuations & Resume Value:**
Een `hervat` (resume) call binnen een handler geeft niet alleen de controle terug aan het originele programma, maar kan ook een waarde doorgeven (de `resume_value`). In het voorbeeld hierboven zal de variabele `result` de waarde `"ok"` krijgen nadat de handler klaar is. Dit mechanisme maakt asynchrone patronen, state injectie en foutafhandeling uiterst flexibel.

**Migratie (Swarm::migrate) & Resource Re-acquisition:**
De `Swarm::migrate` aanroep triggert een netwerk-teleportatie via continuations. De executie op de huidige node pauzeert, en pakt zijn complete callstack en de `MemorySnapshot` in JSON/Base64.
Voordat hij over het netwerk gaat, roept hij de `voor_vertrek` effect handler aan (indien gedefinieerd). Hier kan men expliciet open sockets/files sluiten. 
Op de doellocatie (na deserialisatie) roept de Helheim executor eerst `na_aankomst` aan. Hier herstelt de script zijn resources (bijv. opnieuw verbinden met DB) vóórdat de normale uitvoering via `hervat` of `resume` doorgaat.

**Migratie Contract (Resource Validatie):** Het is de expliciete verantwoordelijkheid van de `na_aankomst` handler om alle verbonden state (zoals open sockets via `Std::IO::Tcp`) te heropenen in een equivalente logische toestand als voor vertrek. Bij het ontbreken van correcte re-acquisitie zal de executor falen op de eerstvolgende netwerk I/O die vereist is, wat de verdere state van de code onvoorspelbaar en de node attributie onveilig maakt. Als `voor_vertrek` crasht, wordt de migratie volledig afgebroken en blijft de code veilig op de oorspronkelijke node draaien zonder memory leaks. Een crash in `voor_vertrek` blokkeert de migratie.

*(Zie ook `examples/reacq_full.hel` voor een compleet uitvoerbaar bestand.)*

**Volledig voorbeeld met qualified calls en resource re-acq:**
```
gebruik "Std::IO::Tcp";

effect Migratie { voor_vertrek, na_aankomst }

handle Migratie {
    voor_vertrek => {
        roep_aan Std::IO::Tcp::sluit(s);
        hervat("ok");
    },
    na_aankomst => {
        zet s = roep_aan Std::IO::Tcp::verbind("192.168.1.100");
        hervat("ok");
    }
} in {
    // We gebruiken hier een gekwalificeerde aanroep voor de functie
    roep_aan Std::IO::Tcp::stuur(s, "Hallo wereld!");
    // We gebruiken perform voor het effect
    perform Swarm::migrate("10.0.0.5", 9003);
    roep_aan Std::IO::Tcp::stuur(s, "Ik ben nu op node 2!");
}
```

### 3.12 Actor Supervisor (Crash Escalation)

Helheim ondersteunt gedistribueerde en lokaal-geïsoleerde actors via `Actor.spawn`. Een actor draait in zijn eigen `daemon_memory` stack en kan niet direct de globale state muteren. Als een actor crasht (bijv. via `gooi`), wordt er gekeken naar zijn **SupervisionStrategy**.

**Spawn Syntax:**
```
// Standaard ("Stop" strategie)
perform Actor.spawn("{ ... }");

// Met escalatie ("Escalate" strategie)
perform Actor.spawn("{ ... }", "Escalate");

// Met herstart ("Restart" strategie)
perform Actor.spawn("{ ... }", "Restart");
```

**Escalatie:**
Als een child-actor wordt gestart met de `"Escalate"` strategie en hij crasht, pakt de Helheim executor dit op en stuurt hij een foutbericht terug naar de inbox (mailbox) van de parent-actor die hem heeft gestart. Het bericht begint altijd met `"ESCALATION_ERROR van <child_id>:"`.
De parent kan dit in zijn `ontvang { ... }` blok afvangen. Dit patroon stelt Helheim in staat om diepe falende takken in asynchrone executie bomen veilig naar boven te borrelen. Als een parent de error op zijn beurt weer `gooi`t, ontstaat er een multi-level escalation chain (bijv. `ESCALATION_ERROR van 2: ESCALATION_ERROR van 3: ...`).

*(Zie test `test_actor_supervisor_escalate_deep` in `tests/integration_tests.rs` voor een bewezen 4-level escalatie chain).*

### 3.13 FFI en Package Manager (Ed25519 Signatures)

Helheim kan native C/Rust bibliotheken (`.so` of `.dll`) zero-overhead inladen via de Package Manager. Omdat we bare-metal draaien, eist de `PackageManifest` een cryptografische Ed25519 handtekening (`.sig`) naast de module. Het laden van ongesigneerde code of het gebruik van paden met directory traversal (`../`) is fundamenteel onmogelijk. Dit beveiligt het `Swarm` netwerk tegen malafide externe plugins. *(Zie `helheim-core/src/bin/signer.rs` en `helheim-core/tests/test_ffi_stress.rs`).*

### 3.14 Flight Recorder (Observability)

In plaats van langzame log-statements, weeft Helheim een hardware-accelerated "Flight Recorder" direct in de AST-executie lus. Elke `MigrateCapture`, `GpuLaunch` of `ErrorPropagated` slaat een microseconde timestamp (TSC) plus payload in een memory-mapped ringbuffer. Met `helheim-cli audit trace.json` kan een ontwikkelaar deze datastroom direct terugleiden naar de specifieke `lijn:kolom` in de CodeTaal broncode, compleet met kleurcodes voor faalpaden.

**Voorbeeld (2-level):**
```
perform Actor.spawn("{
    perform Actor.spawn(\"{ gooi \\\"ChildFout\\\"; }\", \"Escalate\");
    ontvang msg { druk_af \"Fout opgevangen: \" + msg; }
}");
```

---

## 4. Types & Literals

| Type | Voorbeeld | AST representatie |
|---|---|---|
| `Int` | `42`, `-5` | `LiteralValue::Int(i64)` |
| `Float` | `3.14`, `-0.5` | `LiteralValue::Float(f64)` |
| `String` | `"hallo"` | `LiteralValue::String(String)` |
| `Bool` | `waar`, `onwaar` / `true`, `false` | `LiteralValue::Bool(bool)` |
| `List` | `[1, 2, 3]`, `[waar, onwaar]` | `LiteralValue::List(Vec<LiteralValue>)` |

Alle types zijn sterk getypeerd in de AST. Geen runtime type-guessing.  
Strings zijn host-side — geen directe PTX register representatie.  
Lists zijn algemeen bruikbaar.

---

## 5. Trefwoordtabel (tweetalig)

| Nederlands | Engels | Betekenis |
|---|---|---|
| `zet` | `let`, `set` | Variabele definitie |
| `als` … `dan` | `if` … `then` | Conditionale vertakking |
| `anders` | `else` | Fallback blok |
| `zolang` | `while`, `repeat` | While-lus |
| `voor elke` | `for each` | Iteratie |
| `functie` / `met` | `function`, `fn`, `func`, `with` | Functiedefinitie |
| `roep_aan` | `call`, `invoke` | Functieaanroep |
| `geef_terug` / `retourneer` | `return` | Terugkeerwaarde |
| `waar` / `onwaar` | `true` / `false` | Boolean literals |
| `gebruik` | `use`, `import` | Module import |
| `probeer` / `vang` | `try` / `catch` | Foutafhandeling |
| `druk_af` | `print`, `log` | Standaarduitvoer |
| `lees` / `schrijf` | `read` / `write` | Bestand I/O |
| `stuur` … `naar` | `send` … `to` | HSP netwerk verzending |
| `haal` | `fetch` | HTTP GET |
| `voer uit` | `execute` | Shell commando (Motor Cortex) |
| `gedeeld` | `shared` | GPU shared memory allocatie |

---

## 6. Operatoren & Prioriteit

De parser gebruikt een **Top-Down Operator Precedence (Pratt) Parser**.

| Prioriteit | Operator | Type | PTX instructie |
|---|---|---|---|
| 20 | `*`, `/`, `%` | Rekenkundig | `mul.f64`, `div.f64`, `mul.lo.u32` |
| 10 | `+`, `-` | Rekenkundig | `add.f64`, `sub.f64`, `add.u32` |
| 7 | `<<`, `>>` | Bitshift (integer) | `shl.b32`, `shr.b32` |
| 6 | `&`, `\|`, `^` | Bitwise (integer) | `and.b32`, `or.b32`, `xor.b32` |
| 5 | `==`, `!=`, `<`, `>`, `<=`, `>=` | Vergelijking | `setp.eq/ne/lt/gt/le/ge.f64` |
| 3 | `&&` / `en` | Logisch AND | `and.pred` |
| 2 | `\|\|` / `of` | Logisch OR | `or.pred` |

**Typen in PTX:**
- Float/Int expressies → `.f64` registers in GeneralPtxGenerator
- Integer-only (bitwise) → `.b32` / `.u32` registers in PtxGenerator
- Predicaten → `.pred` registers

---

## 7. Host-side Operaties

Deze nodes worden uitgevoerd op de CPU, niet op de GPU.

| Syntax | Keyword | Functie |
|---|---|---|
| `druk_af "tekst"` | `print`, `log` | Standaarduitvoer |
| `lees "pad"` | `read` | Bestand lezen |
| `schrijf "pad" inhoud` | `write` | Bestand schrijven |
| `stuur data naar doel` | `send`, `to` | HSP netwerk verzending |
| `haal url` | `fetch` | HTTP GET |
| `voer uit commando` | `execute` | Shell commando (Motor Cortex) |
| `trace_event!` | | Interne macro voor Flight Recorder observability |

---

## 8. Observability & Flight Recorder

De Helheim runtime (executor) heeft een ingebouwde, zero-overhead tracer (de **Flight Recorder**). 
Alle nodes in the AST genereren cryptografische trace events als dit gecompileerd is met `#[cfg(feature = "flight_recorder")]`.
Trace events worden visueel aan de CLI teruggegeven met kleurcodering voor live monitoring van swarm teleports, handler errors, resource blocks, e.d.

**Trace Effect:**
De taal biedt het `Trace` effect aan om handmatig custom events te loggen.
```
perform Trace::record("Event details");
```

Event types:
- `MigrateCapture` (wanneer een continuation ingepakt wordt)
- `MigrateTeleport` (wanneer verzonden via netwerk)
- `MigrateResume` (wanneer de state op een nieuw node landt)
- ... en standaard runtime events.

---

## 9. GPU Kernel Pad (PtxGenerator)

**Doel:** Expliciete GPU kernels met gecontroleerde register allocatie.  
**Activatie:** `CodeTaal::GpuKernel(GpuKernelDef)` in de AST.  
**Target:** sm_80 (Ampere), PTX versie 8.0.

### Register typen

| Register | Gebruik |
|---|---|
| `%r<n>` (.b32) | Integers, bitmasks |
| `%rd<n>` (.b64) | Pointers, adressen |
| `%f<n>` (.f32) | Floating point compute |
| `%h<n>` (.f16x2) | Half precision (tensor cores) |
| `%p<n>` (.pred) | Conditie predicaten |

### Ondersteunde GPU operaties

- `GpuOperation::SubgroupSync` → `bar.sync 0`
- `GpuOperation::MatrixMultiplyAccumulate` → WMMA `mma.sync.aligned` (m16n8k16, m32n8k16)
- `GpuOperation::SharedLoad/SharedStore` → `ldmatrix.sync`, shared memory
- `MatMul` → FP16 WMMA kernel met async copy (cp.async), double buffering
- `TensorAdd`, `TensorRelu`, `VectorAdd` → standaard CUDA C kernels

### Context binding

`synthesize_lowered_with_context(code, context)` — host-scope variabelen worden als `.param` inputs doorgegeven aan het PTX kernel. Vrije variabelen worden automatisch gedetecteerd via `collect_free_variables()`.

---

## 9. Algemeen Lowering Pad (GeneralPtxGenerator)

**Doel:** Pure CodeTaal constructies (geen GpuKernel AST-nodes) loweren naar geldig PTX.  
**Activatie:** `CodeTaal::Block` of `CodeTaal::FunctionDef` — automatisch.  
**Target:** sm_80, PTX versie 7.0.

### Wat het doet

1. Extraheert functies (`extract_functions`) → aparte `.func` blokken
2. Genereert `extern "C" .entry main()` voor top-level blokken
3. Elke variabele krijgt een eigen `.f64` register
4. Functiescope is geïsoleerd (eigen register teller, eigen var_map)

### PTX output patroon

```ptx
.version 7.0
.target sm_80
.address_size 64

.func (.reg .f64 %ret) bereken (.reg .f64 %f0, .reg .f64 %f1) {
    .reg .f64 %f<1024>;
    .reg .pred %p<1024>;
    mul.f64 %f2, %f0, %f1;
    setp.gt.f64 %p0, %f2, 0f4059000000000000;
    @%p0 bra then_0;
    bra else_0;
then_0:
    mov.f64 %f3, %f2;
    mov.f64 %ret, %f3;
    ret;
else_0:
endif_1:
    mov.f64 %ret, 0f0000000000000000;
    ret;
}
```

### Beperkingen (bewust open gelaten)

- Strings zijn niet in PTX registers — host-side only by design
- `ForEach` is geparkeerd voor Phase 2 PTX lowering
- Integer-specifiek PTX (bitwise, popc) verloopt via PtxGenerator, niet GeneralPtxGenerator
- Caching uitgeschakeld — `CodeTaal` mist `Eq + Hash` door `LiteralValue::Float`

---


## 10. Bekende Beperkingen & Open Werk

| Item | Status | Toelichting |
|---|---|---|
| Kernel caching | Uitgeschakeld | `CodeTaal` mist `Eq + Hash` door `Float` |
| `ForEach` PTX lowering | Open | Fase 3+ |
| String in PTX | Niet ondersteund | Host-side only by design |
| `Chaos` kernel | Stub | Placeholder, oneindige loops / registerdruk |

---

*Dit document beschrijft de werkelijke toestand van CodeTaal per juni 2026.*  
*Gebaseerd op: `ast.rs`, `parser.rs`, `synthesis.rs`, `resolver.rs`, `semantic.rs`.*  
*Niet op aannames, niet op plannen, niet op externe blueprints.*
