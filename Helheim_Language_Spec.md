# Helheim Language Specification (v3.1)

De Helheim-programmeertaal is een Nederlandse DSL gebouwd op Rust en CUDA.
Zero overhead, bare-metal GPU toegang, gedistribueerde compute — allemaal in begrijpelijke Nederlandse syntax.

Laatste verificatie: 2026-05-25
Hardware: Cross-Platform (CPU/Nvidia GPU) | Rust release build
Geïmplementeerde commando's: 50+

---

## Status

| Onderdeel | Status |
|---|---|
| Lexer & Parser | Werkend |
| AST Execution Engine | Werkend |
| REPL modus | Werkend |
| Script modus (enkelvoudige statements) | Werkend |
| Script modus (pipes in voer uit) | Werkend |
| GPU compute (matmul, gpu work, inferno) | Werkend |
| Variabele interpolatie in commando's | Werkend |
| Functies & return values | Werkend |
| Try/catch | Werkend |
| Memory persistentie (onthoud/herinner) | Werkend |
| Swarm networking | Werkend (nodes vereist) |
| Rune engine (bare metal) | Werkend (unlock vereist) |
| Shield encryptie | Werkend |

---

## Uitvoeren

### REPL (aanbevolen voor scripts met pipes)
```
./helheim-cli repl
unlock HELL-MASTER-2026
```

### Script bestand
```
./helheim-cli script mijn_script.hel
```

### Losse opdracht
```
./helheim-cli run "print \"Hallo\";"
```

### Service modus (stil, geen output)
```
./helheim-cli service
```

---

## Autorisatie

De Native Execution Layer (NEL) is standaard vergrendeld.
Vereist voor: `gpu work`, `inferno work`, `rune`, `synthesis`
Niet vereist voor: `matmul`, `print`, `voer uit`, `lees`, `schrijf`, `haal`

Ontgrendelen in REPL:
```
unlock HELL-MASTER-2026
```

---

## 1. Variabelen

```helheim
zet NAAM = "Helheim";
zet GETAL = 42;
zet LIJST = ["apple", "banaan", "kersen"];
zet INFO = {"versie": "3.0", "naam": "Helheim"};
```

Array en dictionary toegang:
```helheim
zet EERSTE = LIJST[0];
zet VERSIE = INFO["versie"];
```

User input:
```helheim
zet NAAM = vraag "Wie ben je?";
```

Bestand inlezen als variabele:
```helheim
zet INHOUD = lees pad/naar/bestand.txt;
```

Variabelen worden automatisch geïnterpoleerd in alle commando's:
```helheim
zet TERM = pepai;
voer uit grep -rl TERM /opt/project/src;
```

Rekenkundige expressies:
```helheim
zet SOM = A + B;
zet VERSCHIL = A - B;
zet PRODUCT = A * B;
zet DELING = A / B;
```

Memory persisteren over sessies:
```helheim
onthoud;
herinner;
```

---

## 2. Output & IO

```helheim
print "Hallo wereld";
print VARIABELE;

lees pad/naar/bestand.txt;
schrijf pad/naar/bestand.txt "inhoud hier";

voer uit nvidia-smi;
voer uit find /home -name *.rs 2>/dev/null | wc -l;

haal https://api.example.com/data;

wacht 2;

installeer htop;
```

---

## 3. Logica

```helheim
als WAARDE == "test" dan {
    print "Gevonden";
} anders {
    print "Niet gevonden";
}
```

Boolean waarden: `waar` en `onwaar`

```helheim
zet ACTIEF = waar;
als ACTIEF == waar dan {
    print "Systeem actief";
}
```

---

## 4. Lussen

While loop:
```helheim
zet TELLER = 0;
zolang TELLER < 10 {
    print TELLER;
    zet TELLER = TELLER + 1;
}
```

For-each over lijst:
```helheim
zet FRUIT = ["appel", "peer", "banaan"];
voor elke item in FRUIT {
    print item;
}
```

Oneindige loop (max 1000 iteraties ingebouwd):
```helheim
zolang waar {
    gpu work 1024;
    wacht 1;
}
```

---

## 5. Functies

Definitie en aanroepen:
```helheim
functie tel_op met a b {
    zet resultaat = a + b;
    geef_terug resultaat;
}

zet UITKOMST = roep_aan tel_op 15 25;
print UITKOMST;
```

Functie zonder return:
```helheim
functie groet met persoon {
    print persoon;
}

roep_aan groet "Architect";
```

---

## 6. Foutafhandeling

```helheim
probeer {
    voer uit commando_dat_kan_falen;
    gooi "Handmatige fout";
} vang {
    print "Fout afgevangen, systeem blijft draaien";
}
```

---

## 7. GPU Compute

### Directe matrix vermenigvuldiging (synthesis engine)
```helheim
matmul 1940;
```
Prestatie: ~1180 GFLOPS (koude start)

### GPU werk met warmup (aanbevolen voor benchmarks)
```helheim
gpu work 1940;
gpu work 1940 on 0;
gpu work 1940 on 1;
```
Voert 3 runs uit, negeert de eerste (GPU warmup), gemiddelde van 2.
Prestatie: ~1943 GFLOPS op RTX 5060 Ti bij 1940x1940

### Multi-GPU parallel (beide kaarten tegelijk)
```helheim
inferno work 2048;
```
Splitst werklast over RTX 3060 + RTX 5060 Ti via rayon threads.

### Distributed over swarm nodes
```helheim
hive work 4096;
```

### AI inference via GPU
```helheim
gpu infer "Wat is de betekenis van intelligentie?";
```
Verbindt via Unix socket `/tmp/helheim_brain.sock` met Helheim Brain.

### Directe PTX synthese
```helheim
synthesis {"type": "MatMul", "m": 512, "n": 512, "k": 512};
```

---

## 8. Rune Engine (Bare Metal)

Vereist `unlock HELL-MASTER-2026` eerst.

### Memory operaties
```helheim
rune READ 0x7fff1234;
rune WRITE 0x7fff1234 0xFF;
rune PHOTO 0x7fff1234 64;
rune REVERSE 0x7fff1234;
rune PEEK;
```

### Stress tests (hardware burn-in)
```helheim
rune KETS;
```
180 seconden, alle cores 4x oversubscribed. Steady-state saturatie.

```helheim
rune GALLOP;
```
120 seconden, dynamische burst patronen met random pauzes.

```helheim
rune INLOPEN;
```
300 seconden, sinusoïdale load voor silicon burn-in/kalibratie.

### Directe PTX injectie
```helheim
rune DEEP [base64_ptx_code];
```

---

## 9. Shield & Beveiliging

Data versleutelen via HSP (Helheim Secure Protocol):
```helheim
shield encrypt mijn_gevoelige_data;
```

Honeypot genereren (voor Project Artisjok):
```helheim
trap generate;
```

---

## 10. Swarm Networking

Nodes bekijken:
```helheim
nodes;
```

Stuur commando naar specifiek node:
```helheim
stuur "matmul 4096" naar 192.168.0.x;
stuur "voer uit nvidia-smi" naar 192.168.0.x;
```

Broadcast naar alle nodes:
```helheim
stuur "gpu work 2048" naar allemaal;
```

---

## 11. Meerdere commando's op één regel

```helheim
print "start" ; gpu work 512 ; print "klaar";
```

---

## 12. Intent detectie (natuurlijke taal)

Helheim herkent ook vage Nederlandse intenties:

| Intentie | Voorbeeld |
|---|---|
| Sturen | `stuur hallo naar pieter` |
| Variabele zetten | `zet X gelijk aan 10` |
| Matrix berekenen | `matmul 1024` |
| Diagnose | `wat is de status` |
| Fix | `los op` / `fix` |
| Snelheid | `boost` / `snel` |
| Zoeken | `zoek` / `analyseer` |

---

## Bekende Bugs

**Print variabele + tekst op één regel**: print naam en waarde apart.
Workaround: gebruik aparte print statements.

**Multiline blokken in REPL**: `als` en `zolang` blokken werken niet over meerdere REPL regels.
Workaround: gebruik script bestanden.

**`anders` branch in parser**: `als/anders` werkt in script modus, niet altijd in REPL.

---

## Voorbeeldscripts

### Benchmark
```helheim
print "=== GPU BENCHMARK ===";
gpu work 512;
gpu work 1024;
gpu work 2048;
gpu work 4096;
print "Klaar.";
```

### Systeem scanner
```helheim
print "=== SYSTEEM SCAN ===";
voer uit find /home/bitboi/dev_2 -name *.rs -type f 2>/dev/null | wc -l;
voer uit find /home/bitboi/dev_2 -name *.rs 2>/dev/null | xargs wc -l | tail -1;
gpu work 1940;
print "Klaar.";
```

### Bestand zoeker
```helheim
print "=== ZOEKER ===";
zet TERM = vraag "Zoekterm:";
voer uit find /home/bitboi -name TERM -type f 2>/dev/null;
voer uit grep -rl TERM /home/bitboi/dev_2 --include=*.rs --include=*.py --include=*.md 2>/dev/null;
print "Klaar.";
```

### Memory persistentie
```helheim
zet GEBRUIKER = "Bitboi";
zet PROJECT = "Helheim";
onthoud;
print "Opgeslagen. Volgende sessie: herinner;";
```

### Swarm benchmark
```helheim
print "=== SWARM BENCHMARK ===";
nodes;
stuur "gpu work 1024" naar allemaal;
inferno work 2048;
print "Klaar.";
```

---

## Volgende stappen (open)

- Script modus pipe bug fixen
- Modulo operator (`%`) voor wiskundige scripts
- String formatting (`print "Hallo $NAAM"`)
- `helheim_lang_core` als losse Rust library extraheren
- Motor Cortex: AI schrijft en voert zelf Helheim code uit
- Gateway fixen (18 compile errors in openai.rs)
