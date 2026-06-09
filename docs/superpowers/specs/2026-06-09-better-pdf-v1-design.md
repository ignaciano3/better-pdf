# better-pdf — v1 Design Spec

**Date:** 2026-06-09
**Status:** Approved for implementation planning

## 1. Purpose

`better-pdf` is a maintained, fast alternative to [pdf-lib](https://www.npmjs.com/package/pdf-lib)
(unmaintained since 2021), focused initially on **filling and flattening AcroForm fields** in
existing PDFs. It targets both **browser and server** runtimes, using a Rust core compiled to
WebAssembly for the CPU-intensive work.

## 2. Scope (v1)

The first release operates on **existing** PDFs only: `load → fill/flatten → save`. It does **not**
create PDFs from scratch or draw arbitrary page content (beyond rendering field appearances and the
visual signature image).

### In scope — features

- **Fill AcroForm fields:**
  - Set text on text fields and multiline/text-area fields.
  - Select a radio option in a radio group (by its real export value).
  - Choose an option in a dropdown (choice field).
  - Check/uncheck checkboxes (using the field's real on-state, not an assumed `/Yes`).
  - Place a **visual** signature (embed an image/appearance) on a signature field.
- **Flatten AcroForm fields:**
  - A single named field.
  - All fields.

### Input PDFs it must handle

- **Primary (mandatory):** PDFs with **classic cross-reference tables** and `FlateDecode` streams.
  This is what the entire fixture corpus (22 real OSFATUN forms, all PDF 1.3) uses.
- **Secondary (goal):** modern compressed PDFs with cross-reference streams + object streams
  (PDF 1.5+). Not exercised by current fixtures; requires dedicated test PDFs.

### Explicitly out of scope for v1 (future candidates)

- Encrypted PDFs.
- Lenient recovery of malformed / off-spec PDFs.
- Cryptographic / PAdES digital signatures (visual signatures only in v1; API leaves room to add).
- Creating PDFs from scratch / general content drawing.
- Custom font embedding beyond standard-14 and fonts already present in the form's resources.

## 3. Technical Requirements

- Public npm package (public, ESM-first, with TypeScript types).
- Rust for CPU-intensive tasks (PDF parsing), compiled to **WebAssembly** so it runs in browser and
  server (Node/Bun/Deno/edge) — matching pdf-lib's "runs everywhere" property.
- Public API is plain JS/TS; the `.wasm` binary is bundled inside the package.
- **Minimal external dependencies:** zero npm/runtime dependencies on the JS side; Rust crates
  allowed as build-time dependencies, kept to the minimum needed (each crate must earn its place).
- `bun` as package manager.

### Non-functional requirements

- Thoroughly tested, preferably via TDD.
- Fast.
- Documented (doc comments on the public API + guides), to later generate a docs site.

## 4. Architecture

**Approach: Rust core + thin JS API.** Rust owns the full PDF document model; JS is a thin, fully
typed, ergonomic wrapper. The JS↔WASM boundary is **coarse** (bytes in → operations → bytes out,
plus field-metadata queries) to minimize crossing overhead.

```
┌─────────────────────────────────────────────┐
│  Public TS API  (better-pdf)                 │  ← what users import
│  load(), getForm(), fields, flatten(), save()│
├─────────────────────────────────────────────┤
│  WASM binding shim (wasm-bindgen)            │  ← coarse boundary: bytes/handles
├─────────────────────────────────────────────┤
│  Rust core (compiled to .wasm)               │
│   Parser · Object model · Field engine       │
│   Appearance engine · Serializer             │
└─────────────────────────────────────────────┘
```

### Rust core components

1. **Parser** — PDF bytes → object model. Mandatory: classic xref tables + `FlateDecode`. Goal:
   xref streams + object streams (PDF 1.5+). Strict (no lenient recovery in v1).
2. **Object model** — in-memory PDF graph (dictionaries, arrays, streams, indirect refs). Single
   source of truth that everything mutates.
3. **Field engine** — walks the AcroForm tree; exposes fields (name, type, current value, valid
   states/options); applies mutations (set text, select radio/dropdown, check/uncheck, place visual
   signature); and **flattens** (one or all) by baking appearances into page content and removing
   the widget annotations.
4. **Appearance engine** — generates real appearance streams for filled fields (required because the
   corpus uses `/NeedAppearances` with no value appearances, and because flattening cannot rely on
   the viewer). Includes a minimal text-layout + font-metrics module: built-in **standard-14** font
   metrics (covers the corpus's Helvetica) plus widths for fonts already in the form's `/DR`. Embeds
   the signature image (PNG/JPEG) as an XObject.
5. **Serializer** — writes the modified model to valid PDF bytes. Default: **incremental save**
   (append-only update — fast, preserves original bytes); full rewrite available as an option.

**Crate selection** will be finalized in the implementation plan, honoring the minimal-dependency
rule (candidates: an object-model/parser crate such as `lopdf` or a lighter custom parser;
`flate2`/`miniz_oxide` for inflate; image decoding only if strictly required).

## 5. Public API (TypeScript)

Async because WASM initializes asynchronously; `load()` auto-initializes WASM (callers do not manage
the WASM lifecycle).

```ts
import { PdfDocument } from 'better-pdf';

const doc = await PdfDocument.load(bytes); // Uint8Array | ArrayBuffer
const form = doc.getForm();

// query
form.getFields();                  // FieldInfo[] { name, type, value, states?/options? }
const f = form.getField('email');  // typed accessor

// fill — typed per field kind
form.getTextField('email').setText('a@b.com');
form.getRadioGroup('sexo').select('M');     // real export value
form.getCheckBox('acepta').check();         // maps to the field's real on-state
form.getDropdown('provincia').select('BA');
form.getSignature('firma1').setImage(pngBytes); // visual appearance only

// flatten
form.flattenField('email');
form.flatten();

// save
const out: Uint8Array = await doc.save();              // incremental (default)
const full = await doc.save({ incremental: false });   // full rewrite
```

- Strongly typed field classes; wrong-type access throws a clear error.
- Button fields expose their valid export/on-state values; `.select()` / `.check()` operate on the
  **real** values, never an assumed `/Yes`.
- Zero npm runtime deps; the `.wasm` ships inside the package; works unchanged in browser and server.

## 6. Testing strategy (TDD)

- **Rust unit tests** — parser, object model, field mutations, appearance generation, serializer;
  written test-first.
- **Golden-file / fixture tests** — the 22 real fixtures in `tests/fixtures/`. Fill → save →
  re-parse → assert values; flatten → assert widgets removed and appearance baked; round-trip and
  byte-stability checks for incremental save.
- **JS/TS API tests** — public surface and error cases (wrong field type, missing field); run under
  both Bun and a browser/WASM harness for parity.
- **Cross-validation** — open outputs with an independent parser (e.g. pdf.js render and/or `qpdf
  --check`) to catch spec violations.
- **Coverage gaps to address with extra fixtures:** dropdowns (only 4 in the corpus) and compressed
  PDFs / xref streams (zero in the corpus).

## 7. Repository & build

```
better-pdf/
  crates/core/        # Rust core (the .wasm source)
  src/                # TypeScript public API
  pkg/                # generated wasm-bindgen output (gitignored)
  tests/fixtures/     # 22 sample OSFATUN PDFs (already present)
  docs/               # docs-site source (TSDoc-generated + guides)
  package.json        # bun, build scripts
```

- **Build:** Rust → WASM via `wasm-pack` / `wasm-bindgen`; bundle wasm + TS into the published
  package. CI builds and tests on every push.
- **Distribution:** single public npm package, ESM-first with types; one artifact for all platforms.
- **Docs:** generated from TSDoc comments + hand-written guides; CI enforces doc comments on the
  public API and runs a docs build.

## 8. Success criteria

- Correctly fills and flattens a corpus of real PDFs (the 22 fixtures), across text, checkbox,
  radio, dropdown, and visual-signature fields.
- Outputs validate cleanly in an independent tool (qpdf `--check` and/or pdf.js render).
- The same API works in both Node/Bun and the browser.
- Zero npm runtime dependencies; minimal, justified Rust crates.
- Public API fully documented.

## 9. Key insights from the real fixture corpus (22 OSFATUN forms)

- All are **PDF 1.3 with classic xref tables** — none use xref/object streams. Classic xref parsing
  is the true must-have; compressed-PDF support needs its own test PDFs.
- All set **`/NeedAppearances true`** with no value appearance streams → the appearance engine is
  essential and must generate appearances for both rendering and flattening.
- The DA font is **Helvetica (`Helv`)** throughout → standard-14 metrics suffice for v1.
- Button export/on-states are **domain-specific** (`Yes/Off`, `F/M`, `SI/NO`, `Titular/Familiar`,
  multi-option groups) → the API must expose and operate on real values.
