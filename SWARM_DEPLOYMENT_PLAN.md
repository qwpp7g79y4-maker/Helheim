# Programmeertaal Focus Plan (CodeTaal eerst, SNN apart)

**Doel**: We focussen primair op de ontwikkeling van de programmeertaal (Helheim / CodeTaal: parser, AST, semantics, lowering/synthesis, features). De SNN / Motor Cortex laag blijft strikt apart – het is een toepassing/laag die de taal gebruikt, niet geïntegreerd in de taal zelf.

We houden de programmeertaal en de SNN strikt gescheiden, zoals je aangaf ("snn is snn en progameer taal is progameer taal" en "we blijven ons nu ook focussen op de progameer taal").

Dit plan bouwt voort op Antigravity's voorstel (Swarm / SNN / VS Code), maar met de correcte focus: programmeertaal centraal, SNN als aparte validatie-laag. Geen verwijzingen naar specifieke hardware aantallen of clusters.

VS Code tooling (optie 3) is ondersteunend voor snellere taalontwikkeling. SNN experimenten (optie 2) zijn aparte validatie van wat de taal kan uitdrukken. Swarm (optie 1) is later, als de taal zelf sterk genoeg is.

## Huidige staat (gebaseerd op inspectie)
- De programmeertaal (CodeTaal) heeft parser (precedence climbing), AST (met Op, ListLiteral, MatrixLiteral), semantic analyzer en PtxGenerator voor lowering.
- SNN laag is apart: bit-packing, tel_spikes (popc), lowered blocks in hel_lowered entry, Motor Cortex execution via PtxBackend.
- Er zijn al .hel voorbeelden voor SNN (03_snn_cortex.hel) die de taal gebruiken voor SNN logica.
- Gateway + dashboard is er voor testing van taal + SNN output.
- Swarm/HSP logica bestaat in de core (voor later gebruik van de taal in gedistribueerde setting), maar is geen focus nu.

**Wat nog ontbreekt / risico's voor taal focus**:
- Meer features in de programmeertaal zelf (bijv. betere functies, modules, types, error handling in CodeTaal).
- Betere lowering voor algemene CodeTaal expressies (niet alleen SNN bitwise).
- Goede VS Code support voor de taal (syntax, highlighting, autocomplete) – dit versnelt ontwikkeling enorm.
- Duidelijke scheiding in docs en code: taal vs SNN laag.
- Meer tests voor de taal (parser, lowering van complexe Op chains, recursive expressions).

## Uitvoeringsplan (Stapsgewijs, gecontroleerd) – Focus op Programmeertaal

### Stap 0: Versterk de programmeertaal kern (parser, lowering, features)
- Verbeter de CodeTaal parser en lowering voor algemene expressies (niet alleen SNN-specifiek).
- Voeg of verfijn features in de taal: betere functie support, error handling, modules, complexere literals (2D matrices al deels daar).
- Schrijf meer tests voor de taal zelf (niet alleen SNN scripts): recursive Op, blocks, context binding in lowering, etc.
- Gebruik de bestaande lowered path en gateway om taal features te valideren.

**Deliverable**: Nieuwe of uitgebreide .hel voorbeelden die pure taal features demonstreren + bijbehorende lowering tests.

### Stap 1: VS Code tooling voor de programmeertaal (optie 3 als accelerator)
- Werk de syntax highlighting, snippets en basic language server uit voor CodeTaal (helheim.tmLanguage.json en gerelateerde bestanden).
- Dit maakt het veel sneller om complexe taal scripts te schrijven en te debuggen.

### Stap 2: SNN als aparte laag valideren met de taal (optie 2, gescheiden)
- Schrijf SNN scripts die de programmeertaal gebruiken (bijv. functies die SNN logica uitdrukken).
- Test dat de SNN laag (bit-pack, popc, Motor Cortex) correct werkt op output van de taal.
- Houd docs en code comments expliciet: "Dit is taal feature X. De SNN laag gebruikt dit zo."

### Stap 3: Documentatie & Professionalisering (taal + scheiding)
- Update LANGUAGE_SPEC.md met nieuwe taal features.
- Zorg dat MOTOR_CORTEX.md duidelijk maakt dat SNN een aparte applicatie is op de taal.
- Alle publieke tekst professioneel (geen emojis, formele toon).
- Maak duidelijke voorbeelden die de scheiding illustreren.

### Stap 4: (Later) Swarm als deployment voor de taal (optie 1)
- Pas de bestaande swarm logica aan zodat je .hel programma's (pure taal scripts) kunt distribueren.
- SNN workloads draaien dan als aparte applicatie op nodes die de taal gebruiken.

Dit houdt de focus op de programmeertaal, met SNN als aparte validatie-laag.

## Hoe we dit gecontroleerd aanpakken
- Ik (Grok) focus op de programmeertaal (parser, lowering, features, tests, VS Code support) + duidelijke scheiding met de SNN laag.
- Jij beslist de prioriteit en geeft expliciet sein.
- Antigravity kan worden ingezet voor uitvoering (builds, tests, docs updates, pushes) **alleen** nadat jij het plan hebt goedgekeurd en "execute" zegt.
- Geen eenzijdige beslissingen of pushes van interne dingen. Interne plannen blijven buiten de publieke GitHub (zoals recent schoongemaakt).

## Mijn voorstel aan jou
**Start met Stap 0 (taal kern versterken) + Stap 1 (VS Code voor de taal)**.

Dit geeft directe vooruitgang op de programmeertaal zelf, terwijl SNN als aparte laag gebruikt wordt voor validatie.

Als je het eens bent met deze focus, zeg "go" of "start met taal focus".

Dan maak ik concrete taken (bijv. uitbreiding van de parser/lowering voor nieuwe taal features, of de VS Code syntax verder uitwerken).

Welke richting wil je dat ik als eerste uitwerk?

- Verbeteringen aan de programmeertaal (specifieke features?)
- VS Code syntax / tooling voor CodeTaal
- Meer tests of voorbeelden die de scheiding taal vs SNN duidelijk maken
- Updates aan LANGUAGE_SPEC.md met focus op de taal

Zeg het maar, dan focus ik daarop. 

Dit plan is puur tekstueel en klaar voor jouw review en sein. We focussen op de programmeertaal, SNN apart.