# Helheim Professionalization Plan (Reviewed Version)

This document is the reviewed and improved version of the professionalization plan.

- The reviewer performed only read-only inspection of the current source and proposed text improvements in this document.
- No implementation was performed: no README rewritten, no docs/ created, no examples/ restructured, no git commits or pushes executed.
- Execution of the concrete steps is the responsibility of Antigravity / Claude. The user must explicitly approve this plan before any changes are built or pushed.

## Huidige Staat (na inspectie 2026-06)
- Helheim heeft 3 crates: helheim-lang (CodeTaal parser/semantic/synthesis + PtxGenerator met lowered blocks), helheim-core (orchestra/executor + PtxBackend met VRAM ringbuffer + execute_lowered_block), helheim-cli.
- Echte Motor Cortex / SNN op GPU bestaat: 
  - lowered general blocks via `lower_general_with_context` → "hel_lowered" PTX entry.
  - Context binding (HashMap<String, LiteralValue>) → .param inputs.
  - Bit-packing: bool lists / waar-onwaar → u32 mask → .b32 registers, bitwise & | ^ << >>, popc.b32 voor threshold (tel_spikes).
  - Result via st.global.b32 (bit-cast in f32 pool slot) + host to_bits() unpack.
  - VRAM ringbuffer/pool (pre-alloc 512 slots, atomic index) om per-launch alloc te vermijden.
  - Echt cudarc launch op RTX 5060 Ti / 3060.
- examples/ is al deels gecurateerd in subdirs (snn/, gpu/, swarm/, logic/, features/).
- usb_payload/ met echte `helheim` binary + .service + .sig + install.sh is aanwezig en **MOET BLIJVEN**.
- Geen docs/ map.
- Huidige README noemt nog "Dutch-Based".
- Geen WebGPU in de code (puur CUDA/PTX/cudarc + NVRTC lowering).

## MUST KEEP (Heilig — Nooit Verwijderen)
- `helheim-cli/usb_payload/` volledig (helheim binary + helheim.service + helheim.sig + install.sh + README.txt). Dit is expliciet gevraagd voor "met binarys".
- Volledige `helheim-lang/` (de CodeTaal core met Dutch keywords + lowered PTX lowering).
- De SNN/Motor Cortex implementatie in helheim-core (ptx_backend.rs execute_lowered_block, executor lowered path, synthesis popc/bitwise/bitcast).
- De Antigravity / "Native Ascension" / "Antigravity Standard" stem en Nederlandse syntax in voorbeelden.

## 1. De README.md Herschrijven (Verbeterd Voorstel)
Huidige tekst is solide ("Native Ascension", "Body to the AI Brain", "Zero Bloat", "Double-Buffered CUDA", "HSP Swarm Protocol").

**Verbeteringen:**
- Vervang "Dutch-Based Abstract Syntax (CodeTaal)" door **"Native Bilingual (English & Dutch) — CodeTaal"**.
  Reden: De taal ondersteunt zowel Nederlandse keywords (zet, als ... dan, zolang, retourneer, waar/onwaar, roep_aan, voer uit, haal, schrijf, lees, stuur, druk_af, geef_terug, functie, gpu_kernel) als Engelse equivalenten. Dit is een sterk, uniek selling point.
- Voeg een duidelijke sectie toe over **"Zero-Overhead SNN on Bare Metal (Motor Cortex)"**:
  > Helheim maakt Python + PyTorch overbodig voor specifieke high-performance workloads.  
  > Spikes worden bit-packed als u32 masks, direct lowered naar PTX met popc.b32 thresholding, uitgevoerd via echte JIT `hel_lowered` entry op CUDA zonder Python interpreter of PyTorch tensor overhead. Context binding laat host variabelen (zet x=...) naadloos in GPU code vloeien. Resultaat via bit-cast in VRAM ringbuffer + host unpack naar waar/onwaar lijsten.
- Houd 1-2 korte elegante syntax voorbeelden met Nederlandse keywords (zet, als ... dan, [waar, onwaar], tel_spikes).
- Voeg onder Architecture een bullet: "Lowered Blocks & Real PTX JIT (geen interpreter overhead voor Block/If/Loop/Op)".

## 2. Documentatie Architectuur (docs/) — Verbeterd Voorstel
Maak `docs/` (bestaat nog niet).

**docs/LANGUAGE_SPEC.md** (formele specificatie):
- Inleiding: tweetalige (NL/EN) CodeTaal, gecompileerd naar AST, lowered naar native PTX.
- Keywords (beide talen): lijst de werkelijke uit de parser (zet/if, als/then, dan/else, zolang/while, retourneer/return, waar/true, onwaar/false, roep_aan/call, voer uit/do, gpu_kernel, haal/fetch, lees/read, schrijf/write, stuur/send, druk_af/print, geef_terug, functie/fn, etc.).
- Types & Literals: Int, Float, String, Bool (waar/onwaar), List (voor spikes [waar,onwaar]).
- Statements: VarDef (zet x = ...), If (als ... dan {} anders {}), Loop (zolang), Block, Return, Print (druk_af), Send (stuur), FileOp (lees/schrijf), HttpOp (haal), GpuKernelDef (gpu_kernel).
- Expressions: Op (inclusief bitwise &|^<<>> voor spikes, popc/tel_spikes intrinsics), VarGet, ListLiteral.
- Context & Lowering: vrije variabelen worden via context binding aan lowered kernels doorgegeven.
- Semantic types (uit semantic.rs): String voor lees/haal, Void voor schrijf etc.

**docs/MOTOR_CORTEX.md** (diepe tech doc — accuraat maken):
- **Niet "WebGPU"** — de implementatie is **CUDA/PTX via cudarc + NVRTC lowering**.
- VRAM Ringbuffer/Pool in PtxBackend (pre-alloc Vec<CudaSlice<f32>>, Atomic index, hergebruik voor result + bitcast b32).
- Bit-packing: host bool list of LiteralValue::List(waar/onwaar) → u32 mask als LiteralValue::Int.
- Lowering: .param .b32 voor masks, ld.param.b32 in %r regs, and.b32/or.b32/xor.b32/shl/shr.b32, popc.b32 voor fire count (tel_spikes).
- Result: st.global.b32 (bit-cast) naar f32 pool slot (past in 4 bytes).
- Host side: memcpy_dtoh → f32.to_bits() as u32 → unpack bits naar "waar"/"onwaar" lijst.
- Context binding: HashMap<String, LiteralValue> → sorted .param inputs + ld.param in lowered entry (hel_lowered).
- execute_lowered_block + synthesize_lowered_with_context.
- Voordeel t.o.v. Python/PyTorch: zero interpreter, bare metal registers, directe GPU launch voor de drempel-logica.

## 3. Voorbeelden (examples/) Structureren — Verbeterd Voorstel
Bouw verder op de bestaande subdir structuur (snn/, gpu/, swarm/, logic/, features/) in plaats van alles plat te gooien.

Voorgestelde structuur (niet destructief ten opzichte van huidige curated subdirs):

- examples/01_basics.hel (of houd in logic/ + symlink of copy met duidelijke naam)
- examples/02_swarm.hel (of swarm/swarm_control.hel)
- examples/03_snn_cortex.hel (cruciaal — met de echte lowered JIT SNN)

**Belangrijk voor 03_snn_cortex.hel (moet gebaseerd zijn op werkende lowered code):**
Gebruik de originele user spec uit de ontwikkelgeschiedenis:
```
zet input_spikes = [waar, onwaar, waar, waar, onwaar, waar];
zet weight_mask  = [waar, waar, onwaar, waar, waar, onwaar];

functie neuron_vuurt met spikes mask {
    zet overlap = spikes & mask;          # bitwise op bit-packed u32
    retourneer overlap;
}

zet resultaat = roep_aan neuron_vuurt input_spikes weight_mask;
druk_af resultaat;   # host unpack naar [waar,onwaar,...]

# Drempel / fire-misfire variant met popc/tel_spikes
zet drempel = 3;
zet count = tel_spikes(overlap);   # of popc(overlap)
als count > drempel dan { ... }
```

Dit demonstreert precies: list literals van waar/onwaar, context, bitwise, popc lowering, lowered block execution, host unpack.

Verwijder of negeer (via .gitignore) oude test_*.hel die niet curated zijn.

Houd python_benchmark.py of verplaats naar internal/ als het niet kern is.

## Verification Plan

1. Modify or update only the specified files and structure (docs/, README updates, curated examples, etc.).
2. Verify that `helheim-cli/usb_payload/helheim` remains present and untouched.
3. Verify that Markdown files are properly formatted (headings, code blocks, no broken links).
4. Include binary presence checks in verification commands:
   ```bash
   ls -l helheim-cli/usb_payload/helheim && echo "Binary intact"
   ```
5. No push until the user explicitly authorizes "push now" or delegates the push to Antigravity.
6. Example commit message: "docs: professionalize README + add LANGUAGE_SPEC.md + MOTOR_CORTEX.md + curated SNN examples (bilingual, accurate PTX lowered SNN, preserve binary)"

## Scope Limitations

- No independent git operations or pushes by the reviewer.
- The usb_payload binary must not be removed.
- The process builds upon the existing curated subdirectories and the previously implemented Motor Cortex lowered JIT, rather than assuming a clean slate.

---

*Based on read-only inspection of the current source (parser keywords, lowered execution in ptx_backend and synthesis, current examples subdirectories, absence of WebGPU, presence of the binary).*
