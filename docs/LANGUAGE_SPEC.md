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

CodeTaal is een **algemene taal**. GPU-compute, SNN en tensors zijn toepassingslagen — geen kern.

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

### 3.5 Modules

```
gebruik "bestandsnaam";
use "bestandsnaam";
import "bestandsnaam";
```

Compile-time module expansie via `Resolver`. Het bestand wordt ingeladen en zijn AST wordt samengevoegd vóór semantic analyse. Geen runtime import.

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
Lists zijn algemeen bruikbaar. Gebruik als spike-tensor is een SNN-toepassingskeuze.

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

---

## 8. GPU Kernel Pad (PtxGenerator)

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
- `ForEach` is nog niet ge-lowered naar PTX (`// Unhandled statement`)
- Integer-specifiek PTX (bitwise, popc) verloopt via PtxGenerator, niet GeneralPtxGenerator
- Caching uitgeschakeld — `CodeTaal` mist `Eq + Hash` door `LiteralValue::Float`

---

## 10. SNN als Toepassingslaag

SNN (Spiking Neural Networks) is **geen kern van de taal**. Het is één mogelijke toepassing.

De taal biedt:
- `List` en `MatrixLiteral` — kunnen spike-tensors bevatten, maar zijn niet SNN-specifiek
- Bitwise operatoren (`&`, `|`, `^`, `<<`, `>>`) — nuttig voor spike-packing, ook algemeen bruikbaar
- `popc` — popcount instructie, direct naar `popc.b32` PTX — voor vuurdrempels in SNN

SNN-specifieke logica (astrocyten, hormonen, droomtoestand, CANN) leeft in **Nexus Brain** — een apart project dat CodeTaal scripts aanroept als uitvoeringslaag.

> Helheim taal = taal. Nexus = gebruiker van die taal. Niet omgekeerd.

---

## 11. Bekende Beperkingen & Open Werk

| Item | Status | Toelichting |
|---|---|---|
| Kernel caching | Uitgeschakeld | `CodeTaal` mist `Eq + Hash` door `Float` |
| `ForEach` PTX lowering | Open | Fase 3+ |
| String in PTX | Niet ondersteund | Host-side only by design |
| `ModelDef` / `ModelInit` | Gedefinieerd in AST | Nog niet ge-lowered |
| `Chaos` kernel | Stub | Placeholder, oneindige loops / registerdruk |

---

*Dit document beschrijft de werkelijke toestand van CodeTaal per juni 2026.*  
*Gebaseerd op: `ast.rs`, `parser.rs`, `synthesis.rs`, `resolver.rs`, `semantic.rs`.*  
*Niet op aannames, niet op plannen, niet op externe blueprints.*
