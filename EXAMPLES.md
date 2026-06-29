# Helheim Examples Repository

Welkom bij de Helheim voorbeelden. Deze scripts demonstreren de kracht van de Helheim "Motor Cortex", de unieke functies zoals Algebraïsche Effecten, gedistribueerde Actor modellen, en state-behoudende teleportatie via continuations.

Om een script uit te voeren, gebruik de `helheim-cli`:
```bash
cargo run --bin helheim-cli -- script examples/<bestandsnaam>.hel
```

## 1. Gedistribueerde Teleportatie (`distributed_teleport.hel`)
**Beschrijving**: Dit script toont hoe een Helheim proces tijdens het uitvoeren gepauzeerd wordt en compleet met al zijn variabelen (state) over het netwerk wordt gestuurd naar een andere server.
**Testen lokaal**:
1. Open Terminal A en start een lokale ontvangende node: `cargo run --bin helheim-cli -- service 9050`
2. Open Terminal B en draai: `cargo run --bin helheim-cli -- script examples/distributed_teleport.hel`

## 2. Swarm Killer Demo (`swarm_killer_demo.hel`)
**Beschrijving**: Gelijksoortig aan `distributed_teleport.hel`, maar simuleert een "nood-migratie". Wanneer een server dreigt te crashen (door resource limieten), migreert het script de workload naar een andere swarm node.

## 3. Resource Re-Acquisition (`migrate_reacq.hel`)
**Beschrijving**: Teleporteren van variabelen is cool, maar wat als je verbonden bent met een database of bestand via TCP? Dit voorbeeld demonstreert het gebruik van `handle Migratie` waarbij resources *voor_vertrek* veilig gesloten worden, en *na_aankomst* correct heropend (re-acquired) worden zonder de logica van de app te breken.
**Testen lokaal**:
1. Open Terminal A en start de ontvanger: `cargo run --bin helheim-gateway -- --port 9055`
2. Open Terminal B en draai: `cargo run --bin helheim-cli -- script examples/migrate_reacq.hel`

## 4. Qualified Perform (`qualified_perform.hel`)
**Beschrijving**: Helheim voorkomt globale effect-botsingen door Gekwalificeerde Effecten. In plaats van gewoon `perform haal()`, demonstreert dit script hoe je `perform Std::IO::Console::druk_af("hallo")` roept om specifieke handlers aan te spreken in een veilige, namespaced context.

## 5. Volledige Re-Acquisition (`reacq_full.hel`)
**Beschrijving**: Een geavanceerde versie van `migrate_reacq.hel` die faalscenario's toont. Wat gebeurt er als een `voor_vertrek` handler of `na_aankomst` handler een error opgooit? Dit script demonstreert de robuuste error-handling van Helheim via `probeer / vang`.

---
*Tip: Zet `HELHEIM_TRACE=1` aan in je console voor het draaien van een test. Dit genereert een audit-log waarin je exact ziet hoe de engine lokaal en remote de CodeTaal AST verwerkt.*
