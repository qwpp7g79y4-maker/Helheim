# Helheim Execution Policy

**Status:** Actief  
**Eigenaar:** Helheim Core Team
**Geldig voor:** Helheim v1.x en hoger

---

## 1. Uitgangspunten

Helheim is een bare-metal programmeertaal ontworpen voor directe GPU- en systeemuitvoering. Met die kracht komen harde grenzen. Dit document definieert wat het systeem mag, wat het niet mag, en hoe die grenzen worden afgedwongen.

De drie kernregels:

> **1. Niets wat het systeem raakt zonder toestemming.**  
> **2. GPU-executie is een privilege, geen standaard.**  
> **3. Elk eiland blijft een eiland.**

---

## 2. Uitvoeringscontexten

Helheim kent twee uitvoeringscontexten. Elke uitvoering begint in de sandboxmodus.

| Context | Toegang | Activering |
|---|---|---|
| **Sandbox** | Berekeningen, variabelen, functies, loops, IO naar stdout | Standaard |
| **Privileged** | FileOp (lezen/schrijven/verwijderen), bare-metal GPU (`hel`), systeemcommando's | Vereist expliciete autorisatie |

Autorisatie voor privileged uitvoering vereist een geldig Ed25519-handtekening op het script, aangeleverd door de Helheim Signer (`SIGNED: <sig> | <script>`). Zonder geldige handtekening blijft de context sandbox, ongeacht de inhoud van het script.

---

## 3. FileOp Governance

FileOps zijn systeemacties met directe impact op het bestandssysteem. Ze worden uitsluitend uitgevoerd in privileged context.

### Toegestane operaties (privileged)
- `lees` — bestand lezen, pad binnen de projectroot
- `schrijf` — bestand schrijven of overschrijven
- `verwijder` — bestand verwijderen

### Absolute grenzen (ook in privileged)
- Geen operaties buiten de geconfigureerde projectroot
- Geen toegang tot systeempaden (`/etc`, `/sys`, `/proc`, `/boot`)
- Geen toegang tot privéschijven gemarkeerd als `PROTECTED` in de Helheim configuratie
- Geen uitvoering van shell-commando's via FileOp

Schending van een absolute grens resulteert in een harde fout. Het systeem voert de operatie niet uit en logt de poging.

---

## 4. Bare-Metal GPU Executie (`hel`-blok)

Het `hel { ... }` blok geeft directe toegang tot de GPU via NVRTC JIT-compilatie. Dit is het krachtigste construct in de taal.

### Vereisten
- Uitvoering vereist de `cuda` feature flag (compilatietijd)
- Uitvoering vereist privileged context (runtime)
- De kernelfunctie **moet** de naam `custom_kernel` dragen

### Kernel regels
- Geen host-side systeemaanroepen vanuit een kernel
- Geen onbeperkte geheugenallocatie — de executor alloceert een vaste buffer (4M threads standaard)
- De kernel ontvangt één pointer (`float* data`) — geen andere interfacevarianten zonder expliciete versie-upgrade

### GPU doelstelling
- Standaard compute-device: RTX 5060 Ti (device 1) — Helheim compute
- Display-device (device 0) wordt niet belast tenzij expliciet geconfigureerd via `HELHEIM_GPU_DEVICE`

---

## 5. Swarm / HSP Communicatie

Helheim-knooppunten communiceren via het Helheim Secure Protocol (HSP). De volgende regels gelden voor swarm-uitvoering:

- Alle inter-node berichten worden versleuteld via HSP (ChaCha20-Poly1305)
- Een knooppunt voert nooit code uit van een onbekend knooppunt zonder HSP-verificatie
- Swarmberichten mogen geen FileOps of `hel`-blokken bevatten tenzij het ontvangende knooppunt in privileged context staat en het bericht een geldige handtekening draagt
- Een knooppunt mag zichzelf niet upgraden naar privileged via een swarmbericht

---


## 6. Projectisolatie

Elk project dat Helheim gebruikt, is een eiland.

- Code en documenten van het ene project worden niet gemengd met een ander
- De Helheim taalruntime deelt geen globale staat tussen projecten
- `CONCEPTS/` en interne planbestanden worden niet gepubliceerd op GitHub

---

## 7. Foutafhandeling en Logging

- Elke mislukte privileged operatie wordt gelogd met tijdstempel, context, en fouttype
- Compilatiefouten in `hel`-blokken worden teruggegeven als leesbare foutberichten, nooit als panics
- Het systeem lekt geen padnamen of systeeminformatie in publieke foutberichten

---

## 8. Wat Helheim niet is

- Geen algemeen besturingssysteem
- Geen vervanger voor een containerisatielaag
- Geen communicatieprotocol voor onvertrouwde netwerken
- Geen AI-model of inferentie-engine — het is de taal waarmee ze aangestuurd worden

---

## Wijzigingen aan dit document

Wijzigingen vereisen goedkeuring van het Core Team.
