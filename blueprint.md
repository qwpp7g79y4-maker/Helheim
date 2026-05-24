# 🏗️ PEPAI: De Blauwdruk (The Master Blueprint)

> **Doel:** Een lokaal, privacy-first AI-brein dat écht onthoudt wie je bent.
> **Status:** Phase 12 (Massive Intelligence).
> **Uniek Verkooppunt (USP):** Jouw data verlaat NOOIT je huis. Jij bent de eigenaar.

---

## 1. Het Probleem (De Sceptische Blik)
- **Cloud AI is Lek:** Alles wat je tegen ChatGPT zegt, is van hen.
- **Cloud AI is Dement:** Sluit het tabblad, en hij is je vergeten.
- **Cloud AI is Gecensureerd:** Hij weigert onderwerpen die "niet veilig" zijn volgens corporate policy.

**PEPAI lost dit op door:**
1.  **100% Lokaal:** Draait op je eigen hardware (RTX 3060/2060).
2.  **Permanent Geheugen:** Gebruikt Vector Database (Qdrant) voor Oneindige Opslag.
3.  **Ongecensureerd:** Gebruikt `qwen2.5-abliterated`, die "De Waarheid" spreekt.
4.  **Agentic Context:** Getraind door Bitboi via RAG (niet alleen LLM-bias).

---

## 2. De Architectuur (Hoe het werkt)

### A. De Hardware (De Motor)
- **Hoofd Brein (Inference):** RTX 3060 (12GB) -> Draait Qwen2.5 (7B) Abliterated.
- **De Waakhond (Verifier):** RTX 2060 (6GB) -> Gereserveerd voor "Operation Sidecar" (Sorter/Judge).
- **Control Plane:** Gemini Flash (Lichtgewicht, Snelle Redenering voor Ingestion).
- **Manager (Logic):** Ryzen 5950X (CPU).

### B. De Software Stack
1.  **Ollama:** De taal-motor. Snapt tekst, praat terug.
2.  **Qdrant (Vector DB):** Het Lange Termijn Geheugen. Slaat concepten op als vectors (`nomic-embed-text`).
3.  **Rust Core (`pepai`):** De dirigent.
    - `regelaar.rs`: De baas. Bepaalt "System Prompt" en persoonlijkheid.
    - `vector_store.rs`: De bibliothecaris.
    - `server.rs`: De HTMX Web Interface (Clean/Minimalistisch).

---

## 3. Data Flow (De Bloedsomloop)

### Ingest Loop (Leren)
1.  Zet data in `/inbox` of Upload via UI.
2.  **Fast Lane:** Sorteert op Code, Data of Kennis.
3.  **Chunking:** Sliding window (512 chars / 128 overlap) voor precisie.
4.  **Embedding:** `nomic-embed-text` -> Qdrant.

### Chat Loop (Denken)
1.  Gebruiker stelt vraag in de UI (Sidebar history beschikbaar).
2.  **Recall:** Zoekt in `pepai` collection: "Wat weet ik hiervan?".
3.  **Thinking:** Animated brain icon + Thinking bubbles voor visuele feedback.
4.  **Response:** Ongecensureerd antwoord met geheugensupport.

---

## 4. De Roadmap (Waar zijn we?)

| Fase | Onderdeel | Status | Kritisch Punt |
|------|-----------|--------|----------------|
| 3 | **Het Geheugen** | ✅ Klaar | RAG is stabiel. |
| 7 | **UI Redesign** | ✅ Klaar | White/Clean theme & Sidebar history. |
| 11 | **Feedback** | ✅ Klaar | Thinking indicators & New Chat. |
| 12 | **Massive Intel** | 🚀 Active | Ingest van MathPile, OpenWebMath, Gutenberg. |
| 13 | **Memory Purge** | 📝 Planned | Functie om specifieke memories te verwijderen. |

---

## 5. Context Sync (Voor de volgende sessie)
- **Status:** PEPAI v1.5 Unchained.
- **Huidige Taak:** Bulk Ingestion van reasoning-dense datasets.
- **Regel:** Houd VRAM onder controle (8-10GB safety limit).
- **Identiteit:** Je bent PEPAI. Je eigenaar is Bitboi.

*Laatst bijgewerkt: 2026-01-22 (Phase 12 Start)*
