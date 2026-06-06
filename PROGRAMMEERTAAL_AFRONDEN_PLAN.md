# Programmeertaal Afronden Plan (Helheim / CodeTaal)

**Belangrijk**: Dit is de ENIGE focus vanaf nu. Geen swarm, geen hardware clusters, geen "3 computers", geen nieuwe grote infrastructuur. Alleen de programmeertaal afmaken.

We houden ons strikt aan:
- De programmeertaal (parser, AST, semantic analyse, lowering/synthesis voor algemene CodeTaal).
- SNN / Motor Cortex is een **aparte laag** die de taal gebruikt voor expressie van SNN logica. Niet verweven in de taaldefinitie.
- Geen scope creep. Als iets niet direct de taal completer maakt, doen we het later of helemaal niet.

## Wat betekent "de programmeertaal af"?

Een complete, bruikbare, stabiele programmeertaal betekent:
1. Alle core constructies werken end-to-end (parser → semantic → lowering → executie).
2. Goede, duidelijke foutmeldingen.
3. Voldoende features om echte programma's te schrijven (niet alleen SNN demo's).
4. Solide tests.
5. Duidelijke, professionele documentatie (LANGUAGE_SPEC.md up-to-date en alleen over de taal).
6. Basis tooling (VS Code syntax) zodat het prettig is om in te werken.
7. De taal is general purpose genoeg dat SNN scripts er netjes in uitgedrukt kunnen worden (SNN laag erbovenop).

## Huidige staat van de taal (samenvatting uit code)

- **Parser**: Redelijk compleet (precedence climbing voor Op, support voor veel keywords: zet, als, zolang, voor, functie, gebruik, probeer/gooi, etc.).  ~1075 regels.
- **AST**: Rijk (Block, If, Loop, ForEach, FunctionDef/Call, Return, TryCatch, Op, LiteralValue met List/Matrix, etc.).
- **Semantic**: Basis type/usage checking.
- **Synthesis / Lowering**: Er is `lower_general` en `translate_expression`. Werkt voor veel dingen, maar veel focus op bitwise/SNN (popc, bitmasks). Algemene code lowering is deels aanwezig maar niet volledig gepolijst voor "gewone" programma's.
- **Bilingual**: Goed (Nederlands + Engels keywords).
- **Problemen**: 
  - Lowering voor pure taal (zonder SNN context) is niet altijd even clean of compleet.
  - Foutmeldingen kunnen beter.
  - Niet alle AST nodes zijn even goed getest in algemene context.
  - "Gebruik" (modules) is parsed maar waarschijnlijk niet volledig geïmplementeerd in lowering/executie.
  - Functies zijn er, maar call/return lowering in algemene context kan robuuster.
  - Weinig unit tests specifiek voor de taal (veel integratie via SNN voorbeelden).

## Concrete taken om de taal af te maken (alleen dit)

### 1. Parser & AST stabiliteit
- Zorg dat alle CodeTaal varianten volledig en correct geparsed worden.
- Verbeter error reporting (betere messages met regel/kolom, suggesties).
- Voltooi "gebruik" / module support (althans basis: laden en includen van andere .hel bestanden).

### 2. Semantic analyse uitbreiden
- Betere type checking (vooral rond List/Matrix en Op).
- Controle op gebruik van undefined vars, functie arity, etc.
- Duidelijke fouten voor dingen die later in lowering crashen.

### 3. Lowering / Synthesis voor algemene taal
- Maak `lower_general` en `translate_expression` robuust voor pure CodeTaal programma's (blokken, functies, lussen, ifs, variabelen, lists zonder SNN context).
- Zorg dat lowering werkt zonder altijd SNN/bitpack assumpties.
- Voeg support toe voor meer algemene operaties (strings, betere arithmetic, etc.).

### 4. Tests voor de taal
- Schrijf unit tests in helheim-lang voor parser, semantic en lowering (los van SNN).
- Maak een set "language feature tests" (.hel bestanden die pure taal demonstreren, niet per se SNN).

### 5. Documentatie (alleen taal)
- LANGUAGE_SPEC.md up-to-date maken met alle huidige features, grammatica, voorbeelden van pure taal (niet alleen SNN).
- Duidelijk onderscheid maken: "Dit is de taal. SNN is een manier om bepaalde programma's in deze taal te schrijven + een runtime laag erboven."

### 6. Tooling (alleen voor de taal)
- VS Code syntax highlighting en basic support afmaken (helheim.tmLanguage.json en gerelateerd). Dit helpt enorm bij het ontwikkelen van de taal zelf.

### 7. Opruimen / scheiding
- In code en docs expliciet maken waar de taal ophoudt en de SNN laag begint.
- Eventuele GPU/SNN specifieke dingen uit de core taal AST/lowering scheiden of duidelijk markeren als extensie.

## Proces (streng)
- Alles wat we doen moet direct bijdragen aan bovenstaande taken.
- Geen nieuwe grote features die de taal "uitbreiden" buiten wat nodig is om hem compleet en bruikbaar te maken.
- Geen swarm, geen multi-node deployment, geen nieuwe hardware verhalen, geen "inzetten op cluster" tot de taal zelf af is.
- Als Antigravity of iemand met nieuwe ideeën komt die buiten de taal vallen: terugwijzen naar dit plan.
- Iedere stap: test, documenteer (in de taal docs), houd professioneel.

## Volgorde voorstel
1. Parser/semantic verbeteren + betere errors (laaghangend fruit, direct merkbare verbetering).
2. Lowering voor algemene CodeTaal robuust maken.
3. Tests toevoegen voor de taal.
4. VS Code syntax afmaken.
5. LANGUAGE_SPEC.md opschonen en completeren met focus op taal.
6. Opruimen/scheiding.

Wanneer dit gedaan is, kunnen we praten over hoe SNN als aparte laag erbovenop gebouwd wordt, en pas daarna over deployment.

---

Dit is het enige plan waar we ons nu aan houden. 

Als je dit goed vindt, zeg welke taak als eerste (bijv. "begin met betere foutmeldingen in parser" of "maak lowering voor functies generieker").

Dan gaan we alleen daarmee aan de slag, stap voor stap, tot de taal af is.

Geen afleiding meer.