# Helheim Package Manager & Security

De Helheim Package Manager is verantwoordelijk voor het decentraal en veilig distribueren van functionaliteit. Dit omvat zowel pure `.hel` libraries als native `.so`/`.dll` FFI plugins (C-ABI).

## Architectuur & Flow

De distributiepijplijn kent drie strikte fases voordat code ooit het geheugen raakt:

1. **Fetching (Ophalen):**
   Code kan lokaal, via HTTP/HTTPS of via de Swarm (P2P) worden opgehaald.
   *SSRF Protectie:* HTTP/HTTPS is hardcoded beperkt tot `https://pkg.helheim.dev/` en `https://registry.helheim.dev/`. Interne netwerken (`192.168.x.x`) of malafide externe domeinen zijn op engine-niveau geblokkeerd.
   *Traversal Protectie:* Lokale paden mogen geen `..` of absolute paden (`/etc/`) bevatten om LFI (Local File Inclusion) te voorkomen.

2. **Verificatie (Crypto-Lock):**
   Nadat een package blob (manifest + signature + data) binnen is, verifieert de `PackageManager` de signature via de `HelSigner` (Ed25519). Cruciaal is dat de signature over de _combinatie_ van Manifest en Data ligt. Dit voorkomt dat aanvallers een geldige plugin in een malafide manifest wikkelen om de geregistreerde naam te spoofen.

3. **Loading (Inladen):**
   Pas ná cryptografische goedkeuring gaat de library door naar de `NativeModuleLoader` (voor `.so`) of de AST parser (voor pure modules).

## Manifest Formaat

Helheim packages gebruiken een JSON manifest, vaak ingebed in de payload of als aparte `.sig` sidecar:

```json
{
  "name": "sqlite",
  "version": "1.0.0",
  "kind": "ffi",
  "description": "Zero-overhead SQLite driver"
}
```
`kind` bepaalt of het "ffi" (native code) of "hel" (pure CodeTaal) is.

## Installeren via CLI / Script

In je script installeer je packages met de `SysOp`:
```helheim
voer uit installeer_ondertekend sqlite test_plugins/libsqlite.so;
```
Dit roept in de achtergrond `import_signed` aan op de `PackageManager`.

Daarna kan je de functionaliteit benaderen:
```helheim
// Gekwalificeerde FFI aanroep:
zet db = perform sqlite::open(":memory:");
```

## Security Tests (Edge Cases)
Helheim bevat een zware test-suite (`test_package_manager.rs`) ter verificatie van de security grenzen:
- `test_package_path_traversal`: Bewijst dat imports naar `../../../etc/passwd` direct crashen.
- `test_manifest_spoofing_signature_failure`: Garandeert dat het wijzigen van de `name` in een legitiem package onmiddellijk leidt tot een signature failure.

Tevens wordt concurrent FFI loading en unloading (hot-reload) getest zonder de thread-safety in gevaar te brengen via een lock-free geclonede `Arc<LoadedNativeModule>` in de executor.
