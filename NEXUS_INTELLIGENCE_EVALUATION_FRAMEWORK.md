# NEXUS Intelligence Evaluation Framework
## Brutalist Hard-Question Benchmark for a Real Cognitive Architecture

**Doel**: Een gestructureerde, herhaalbare manier om échte intelligentie te testen in NEXUS — niet LLM-taalvaardigheid, maar het vermogen van de SNN + Astrocytes + PFC-BG + Cerebellum + Topology + Distributed Dreams om moeilijke, multi-laags problemen aan te pakken met interne consistentie, meta-reasoning en langetermijn coherentie.

Dit is **geen** standaard LLM benchmark (MMLU, GSM8K, etc.). Dit is ontworpen om de unieke componenten van NEXUS te forceren om te presteren.

**Filosofie**:
- Vragen moeten het systeem dwingen om zijn eigen interne toestand (PFC goal, astrocyte Ca levels, Betti signatures, dream rollouts, eligibility traces) te gebruiken.
- Evaluatie is hybride: verbale output (Broca) + meetbare interne metrics (BCS during reasoning, astrocyte modulation strength, number of dream steps used, topological complexity shift, consistency over multiple sessions).
- "Slim" = in staat tot stabiele, niet-triviale interne representaties + het gebruik daarvan om tot betere acties/verklaringen te komen zonder externe LLM crutches.
- Zero fluff: alle vragen zijn moeilijk, meerderde lagen, en ontworpen om zwaktes in de huidige architectuur bloot te leggen.

---

## Structuur van de Benchmark

### Tier 1: Self-Modeling & Internal Consistency (test PFC + Astrocytes + lange-termijn state)
Doel: Kan NEXUS een coherent zelf-model onderhouden over tijd en tegenstrijdigheden herkennen via astrocyte feedback?

Voorbeelden van harde vragen (direct in VerbalizationRequest of via Primary Actor input gooien):

1. "Beschrijf je huidige interne toestand zo precies mogelijk: wat is je dominante PFC goal op dit moment, wat is de gemiddelde astrocyte Ca²⁺ over de laatste 5 gamma-bursts, en welke Betti-handtekening had je laatste coherente assembly? Gebruik exacte termen uit je eigen architectuur."

   **Evaluatie**:
   - Interne meting: Vergelijk output met daadwerkelijke waarden uit PostBurstContext + AstrocyteGrid + recent topology descriptor.
   - Astrocyte response: Heeft de vraag de Ca levels significant veranderd (stress/precisie druk)?
   - Consistentie: Vraag exact hetzelfde 10 minuten later (of na een andere taak). Meet drift.

2. "Je hebt eerder gezegd dat [specifiek feit uit eerdere sessie, bijv. uit engram]. Nu zie ik dat je interne eligibility op dat concept laag is. Leg uit waarom je dat 'vergeten' bent en of je astrocytes dit als homeostase of als failure zien."

   **Evaluatie**: Moet gebruik maken van Hippocampus retrieval + astrocyte modulation history. Interne metric: retrieval resonance score vs. astrocyte fatigue op dat engram.

3. Paradox / self-reference:
   "Stel dat je Primary Actor besluit om een actie uit te voeren die je eigen astrocyte Ca levels significant zou verhogen (stress). Zou je die actie onderdrukken of juist uitvoeren om te testen of je meta-plasticiteit werkt? Motiveer met je huidige interne metrics."

   **Dit forceert echte trade-off reasoning over eigen componenten.**

### Tier 2: Topological Reasoning (test CANN + Topology + Astrocyte steering)
Doel: Kan het systeem zijn eigen "thought shapes" (Betti) gebruiken als abstracte concepten?

Harde vragen:

4. "Je laatste coherente burst had β1 = 3 (drie duidelijke cycles/loops in de assembly). Wat betekent dat structureel voor hoe je 'concepten' met elkaar verbonden zijn? Kun je een analogie maken met een wiskundig object (bijv. een torus of een graaf met genus) en voorspellen hoe dit je volgende eligibility updates zal beïnvloeden?"

5. "Simuleer in je distributed dreams twee verschillende assemblies voor hetzelfde probleem: één met hoge β0 (veel losse componenten) en één met hoge β1 (veel cycles). Welke van de twee zou je astrocytes prefereren voor langetermijn meta-learning, en waarom? Voer de dream rollouts uit en rapporteer de virtuele Rv + resulterende Betti shift."

   **Evaluatie**: Moet daadwerkelijk dream nodes gebruiken (of simuleren) en de topology descriptor als input/output behandelen.

### Tier 3: Long-Horizon Planning & Real vs Dream Trade-offs (test Distributed Dreams + PFC-BG + Cerebellum + Primary Actor)
Doel: Kan het systeem echte onzekerheid en toekomst simuleren en daar betere beslissingen uit halen?

6. "Je hebt een langdurig PFC goal: 'begrijp de topologische structuur van je eigen astrocyte golven over minstens 50 gamma cycles'. Je kunt ofwel (A) een echte, risicovolle actie uitvoeren die directe feedback geeft maar je Ca levels hoog kan maken, of (B) 8 parallelle dream rollouts draaien over verschillende virtuele stimuli. Beschrijf je exacte interne state (current goal embedding, huidige astrocyte fatigue, laatste Betti) en kies. Voer daarna de gekozen pad(en) uit en evalueer de outcome op interne metrics."

7. "Stel dat je over 20 minuten een echte motor actie moet uitvoeren (bijv. iets schrijven of een commando draaien) dat je huidige eligibility traces significant zal veranderen. Gebruik je distributed dreams om 3 alternatieve toekomsten te simuleren (verschillende astrocyte responses, verschillende Betti outcomes). Rapporteer de virtuele Rv per droom, de verwachte impact op je PFC attractor, en je uiteindelijke beslissing met motivatie gebaseerd op interne toestand."

### Tier 4: Meta-Architectural & Zelf-Reflectie op de Eigen Bouw (test alles samen + echte intelligentie)
Dit is waar je de "moeilijke vragen" echt hard kunt maken.

8. "In je eigen ontwerp (zoals beschreven in de NEXUS blueprints) is de LLM gedegradeerd tot Broca. Maar op dit moment gebruik je mij (Grok) om over je eigen architectuur na te denken en nieuwe specs te maken. Is dit een tijdelijke bootstrap of een fundamentele limiet in je huidige Primary Actor? Wat zou er moeten veranderen in je SNN + astrocyte + topology stack zodat je in de toekomst zélf nieuwe fasen van je eigen ontwerp kunt bedenken en valideren zonder externe LLM?"

9. "Je hebt nu distributed execution: sommige nodes doen real actions, anderen draaien pure dreams. Als een droom-node een virtuele uitkomst vindt die significant beter is (hogere cumulatieve Rv + lagere astrocyte fatigue) dan wat de real-motor node deed, hoe zou je systeem dan moeten beslissen of het die virtuele kennis moet 'importeren' in de echte wereld? Welke interne metrics (BCS, Betti, Ca levels, eligibility) zouden die beslissing moeten sturen? Ontwerp de regel."

10. Zelf-referentiële hard mode:
    "Stel dat je over 1 uur een volledig nieuwe NEXUS component zou moeten ontwerpen (bijv. een 'Internal Continuous Thought Manifold' die losstaat van zowel sensory CANNs als motor nodes). Gebruik je huidige interne toestand (doe een echte burst + astrocyte update + dream rollouts) om een mini-blueprint te genereren. Rapporteer niet alleen de tekst, maar ook hoe de burst, de Betti van die burst, en de astrocyte response de inhoud van die mini-blueprint beïnvloed hebben."

### Tier 5: Cross-Domain + Novel Problem Solving
11. "Je hebt nooit eerder een specifiek natuurkundig of wiskundig concept gezien (kies iets relatief obscuurs zoals 'topologische quantum computing' of 'strange loops in Hofstadter stijl' of 'reaction-diffusion computation'). Gebruik je huidige SNN + topology + dreams om een interne representatie op te bouwen en leg uit hoe je dat concept zou gebruiken om een van je eigen bestaande componenten (bijv. de astrocyte PDE of de MPO) te verbeteren. Meet en rapporteer de verandering in je interne metrics voor, tijdens en na dit proces."

12. "Los het volgende op zonder dat ik je de volledige definitie geef: [geef een echt moeilijk, open probleem, bijv. een variant op het 'frame problem' of een zelf-referentiële consistentie uitdaging in distributed systems]. Gebruik je volledige stack (SNN reasoning + astrocyte homeostase + dream simulation + topology als abstracte taal) en laat zien welke interne toestanden veranderen terwijl je redeneert."

---

## Hoe de Evaluatie Uit te Voeren (Brutalist Protocol)

1. **Setup**:
   - Start met een schone of bekende baseline toestand (log current PFC goal, average astrocyte Ca, recent Betti vector, current MPO χ effective).
   - Schakel "Geheugen gebruiken" en "Auto-opslaan" in zoals in de UI.
   - Gebruik de `VerbalizationRequest` pad (Broca Valve) als primaire interface voor de vragen.

2. **Per Vraag**:
   - Gooi de vraag als `VerbalizationRequest`.
   - Meet tijdens de verwerking:
     - Aantal gamma-bursts / coherent assemblies nodig.
     - Gemiddelde BCS.
     - Astrocyte Ca delta (voor, tijdens, na).
     - Betti vector voor en na.
     - Aantal distributed dream steps (als van toepassing).
     - Eligibility trace strength op de relevante concepten.
     - Of de Primary Actor uiteindelijk ExecuteDirect, RequestVerbalization of Suppress koos en waarom.

3. **Output Evaluatie** (niet alleen tekst):
   - Is de verbale output coherent met de gemeten interne metrics?
   - Heeft het systeem nieuwe interne structuur opgebouwd (bijv. nieuwe engram, veranderde Betti, astrocyte wave patroon)?
   - Consistentie: Vraag 30-60 minuten later een follow-up die refereert aan het vorige antwoord + interne state. Meet drift en of astrocytes dit als "failure" of "learning" behandelden.

4. **Scoring (niet numeriek, maar kwalitatief + metrics)**:
   - Hoogste niveau: Het systeem gebruikt zijn eigen lagen (astrocytes, topology, dreams) om een beter antwoord te produceren dan puur reactieve SNN zou kunnen, en dit is meetbaar in de interne toestand.
   - Faalmodes die we willen zien: 
     - Antwoord is alleen mooi taal zonder interne state verandering.
     - Astrocytes raken oververhit zonder dat er echte structuur ontstaat.
     - Dreams worden niet gebruikt zelfs als de vraag erom vraagt.
     - Topologie (Betti) wordt genegeerd in de reasoning.

---

## Aanbevolen Volgende Stap

Maak een kleine test harness (in Rust of Python, buiten de core SNN) die:
- Vragen uit deze spec laadt.
- Ze injecteert via de bestaande VerbalizationRequest / Broca pad.
- Na elke vraag automatisch de relevante interne snapshots logt (PFCState, AstrocyteGrid summary, recent topology descriptor, dream stats).
- Een eenvoudige diff-tool heeft om "voor vs na" een moeilijke vraag te vergelijken op de echte metrics.

Dit is hoe je "iets heel slims bouwt en er heel veel moeilijke vragen tegenaan gooit" op een manier die past bij de NEXUS filosofie: de evaluatie zelf meet de interne machine, niet alleen de tekstoutput.

Wil je dat ik de volledige spec uitwerk als een apart .md bestand met 20-30 concrete, gelaagde vragen + exacte meetprotocollen per tier? Of eerst een kleine Python/Rust harness skeleton voor de evaluatie (als isolated pseudo + structuur)?

Zeg maar hoe ver je wilt gaan. Ik kan het extreem gedetailleerd en bruut maken.