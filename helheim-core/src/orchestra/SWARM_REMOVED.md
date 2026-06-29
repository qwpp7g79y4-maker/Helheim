# orchestra/swarm.rs — VERWIJDERD

**Datum:** 2026-06-11  
**Reden:** ConsciousWorker / CleanerWorker hoort niet in helheim-core.  
Dit is sorteerlogica (pepai script, downloads_opschonen) — thuishoort in **helheim-web**.  
Hier beland via de black hole / drag-and-drop sessie.  

**Wat er stond:** `ConsciousWorker` trait + `CleanerWorker` + `Swarm::dispatch()` (text-based worker router)  
**Niet te verwarren met:** `network/hsp_node.rs` — dat is de echte HSP TCP engine (blijft staan)
