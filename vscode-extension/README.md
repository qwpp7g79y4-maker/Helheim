# Helheim Language Support for VS Code

Officiële syntax highlighting, language configuration en snippets voor de Helheim programmeertaal (CodeTaal / bilingual).

## Installatie (Development / Direct)

Kopieer de hele map naar je VS Code extensions map:

```bash
# Voorbeeld (pas pad aan)
cp -r vscode-extension ~/.vscode/extensions/helheim-language-0.2.0
```

Herstart VS Code en open een `.hel` bestand.

## Features

- Volledige syntax highlighting voor alle keywords — Nederlands én Engels:
  - Variabelen: `zet`, `let`, `set`
  - Control flow: `als`/`if`, `anders`/`else`, `zolang`/`while`, `voor`/`for`
  - Functies: `functie`, `fn`, `function`, `retourneer`, `return`
  - Aanroepen: `roep_aan`, `call`, `invoke`
  - Foutafhandeling: `probeer`/`try`, `vang`/`catch`, `gooi`/`throw`
  - Concurrentie: `tegelijkertijd`/`concurrent`, `achtergrond`/`daemon`
  - TCP: `tcp_luister`, `tcp_verbind`, `tcp_accepteer`, `tcp_stuur`, `tcp_ontvang`
  - GPU: `hel { ... }`, `matmul`
  - Print: `druk_af`, `print`, `log`
- Highlighting van functie-namen bij definitie
- Lijsten `[ ... ]` en matrices (geneste lijsten)
- Bytes literals: `b"raw bytes"`
- String interpolatie: `{VAR}` en `$VAR`
- Support voor integers, floats en booleans (`waar`/`onwaar`/`true`/`false`)
- Auto-closing brackets, auto-indent en comment toggling (`#`)
- Snippets voor alle veelgebruikte constructen

## TypeScript Setup & Build (voor .vsix)

De extensie bevat een minimale TypeScript setup (voor toekomstige language features zoals hover, completion, diagnostics, etc.).

### Vereiste bestanden

- `package.json` (met `main`, `activationEvents`, scripts)
- `tsconfig.json`
- `src/extension.ts` (minimale activate/deactivate)

### Exacte build instructies

1. Ga naar de extensie map:
   ```bash
   cd vscode-extension
   ```

2. Installeer dependencies (eenmalig):
   ```bash
   npm install
   ```

3. Compileer de TypeScript code:
   ```bash
   npm run compile
   ```
   Dit bouwt naar de `out/` map (volgens tsconfig.json).

4. Maak de .vsix package:
   ```bash
   npx vsce package
   ```

   Of gebruik het npm script:
   ```bash
   npm run package
   ```

5. Installeer de gegenereerde `.vsix` in VS Code:
   - Open Extensions view
   - Klik op de `...` menu rechtsboven
   - Kies **Install from VSIX...**
   - Selecteer `helheim-language-0.2.0.vsix`

### Watch mode (tijdens ontwikkeling)

```bash
npm run watch
```

Wijzigingen in `src/extension.ts` worden automatisch gecompileerd.

## Bestanden structuur

```
vscode-extension/
├── package.json
├── tsconfig.json
├── language-configuration.json
├── README.md
├── src/
│   └── extension.ts
├── syntaxes/
│   └── helheim.tmLanguage.json
├── snippets/
│   └── helheim.code-snippets
└── out/                  ← gegenereerd na `npm run compile`
```

## Gerelateerd

Helheim is een tweetalige (Nederlands/Engels) algemene programmeertaal met sterke focus op eenvoud en research-toepassingen.

Zie ook de hoofd Helheim repository voor de taal specificatie, parser en executor.
