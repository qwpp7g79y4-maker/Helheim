# Helheim

> **Taal:** CodeTaal — tweetalige DSL (Nederlands & Engels)  
> **Runtime:** Native Rust, CPU + GPU (CUDA optioneel)

Helheim is een high-performance execution engine met een eigen programmeertaal. Scripts worden gecompileerd naar een AST en uitgevoerd op CPU of direct verlaagd naar PTX voor Nvidia GPU's.

## CodeTaal

Bilingual (Nederlands/Engels). Dezelfde semantiek, beide notaties werken.

```helheim
zet naam = "Helheim";
zet versie = 1;

als versie >= 1 dan {
    druk_af naam;
}

functie optellen a b {
    retourneer a + b;
}

zet resultaat = roep_aan optellen 10, 32;
druk_af resultaat;
```

## Functies

- Variables (`zet` / `let`)
- Control flow (`als`/`if`, `zolang`/`while`, `voor`/`for`)
- Functies (`functie`/`fn`, `roep_aan`/`call`, `retourneer`/`return`)
- Lijsten en matrices (`[1, 2, 3]`, `[[1, 0], [0, 1]]`)
- Concurrente blokken (`concurrent { ... }`)
- Bestands- en netwerk I/O (elevated privileges)
- PTX JIT lowering voor GPU-blokken (`hel { ... }`)
- HSP Swarm Protocol — gedistribueerde uitvoering via TCP

## Gebruik

**Script uitvoeren:**
```bash
helheim script mijn_script.hel
```

**Interactieve REPL:**
```bash
helheim repl
```

**Swarm node starten:**
```bash
helheim service --port 9003
```

## Architectuur

| Crate | Functie |
|---|---|
| `helheim-lang` | Lexer, parser, AST, PTX lowering |
| `helheim-core` | Executor, memory, swarm, GPU backend |
| `helheim-gateway` | HTTP API (`POST /api/execute`) |
| `helheim-cli` | CLI — script / repl / service |

## Documentatie

- [Taalspecificatie](docs/LANGUAGE_SPEC.md)
- [Kernprincipes](docs/architecture/core_principles.md)
