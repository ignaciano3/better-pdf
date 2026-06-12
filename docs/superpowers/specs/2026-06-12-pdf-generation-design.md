# better-pdf — PDF Generation Design Spec

**Date:** 2026-06-12
**Status:** Approved for implementation planning

## 1. Purpose

Add PDF generation to `better-pdf`: creating documents from scratch and drawing content
(text, images, vector graphics) on pages — both newly created pages and pages of existing
loaded PDFs. This lifts the v1 restriction that the library only operates on existing
AcroForms, and moves the package toward pdf-lib feature parity for the most common
generation use cases.

Alongside the feature work, the package is reorganized into `core` / `forms` / `generate`
areas with subpath exports, so users who only fill forms or only generate documents get a
focused API surface — without splitting into separate npm packages.

## 2. Scope

### In scope

- **Create documents:** `PdfDocument.create()`, `addPage(size)` with standard page sizes.
- **Draw text:** standard-14 fonts, size, color, position. Works on new and existing pages.
- **Draw images:** embed JPEG and PNG (same decoder constraints as visual signatures),
  draw at position/size. Works on new and existing pages.
- **Vector graphics:** lines, rectangles, ellipses; fill color, border color/width, opacity.
- **Page introspection:** page count, page sizes, rotation for loaded documents.
- **Text measurement:** width of a string at a font/size (for caller-side layout).
- **Packaging split:** `src/core`, `src/forms`, `src/generate` with `./forms` and
  `./generate` subpath exports. Root export keeps re-exporting everything (no breaking
  change). Single WASM binary, single npm package.
- **Fixture cleanup:** delete the 17 fixture PDFs (of 24) that no test, script, Rust
  test, fuzz corpus, or doc references. The used set is: `Form.-D.P.-2.4.1-Ficha-personal.pdf`,
  `Anexo-3-sssalud.pdf`, `Convenio-OSFATUN-Discapacidad-2022.pdf`,
  `Formulario asistencia al viajero 1.pdf`, `Modulo-de-Diabetes.pdf`, and both files in
  `tests/fixtures/generated/`.

### Out of scope (future candidates)

- Custom font embedding (TTF/OTF, subsetting) — standard-14 only; text is limited to the
  WinAnsi charset (accents work, CJK does not).
- Creating AcroForm fields on generated documents.
- Arbitrary SVG-style paths, bezier curves, clipping, transforms beyond position/size.
- CMYK color (RGB and grayscale only).
- Splitting the WASM binary per feature area (no cargo feature flags for now).
- Copying/merging pages between documents.

## 3. Architecture

### Approach: extend the op-queue pattern

The existing form path is stateless at the WASM boundary: TypeScript queues mutations,
`doc.save()` makes one call per concern (`fill_fields`, `flatten_fields`) passing the PDF
bytes, a JSON op list, and a binary blob payload. Drawing follows the same pattern:

- `page.drawText(...)`, `page.drawImage(...)`, etc. push ops into a per-document draw
  queue on the TypeScript side. No WASM call happens at draw time.
- `doc.save()` applies queued work in order: **fills → flattens → draw ops**, each as one
  stateless WASM call.
- For loaded documents, draw ops produce an incremental update: a new content stream is
  appended to each touched page's `/Contents` array, and the existing content is wrapped
  in `q`/`Q` so unbalanced graphics state in the original streams cannot leak into the
  new content.
- For created documents, the Rust side builds the document skeleton (catalog, page tree,
  pages) and then runs the same draw machinery over it.

Alternatives considered and rejected:

- **Stateful WASM document handle** (open in WASM memory, mutate live): more natural for
  heavy generation but breaks the stateless pattern, adds cross-boundary lifetime and
  memory management, and forces a refactor of the working form path.
- **Building content streams in TypeScript** (Rust only splices bytes): splits PDF
  knowledge across two languages and duplicates font metrics handling.

### WASM additions

All stateless, matching the existing three exports:

| Function | Signature | Purpose |
|---|---|---|
| `read_pages` | `(data) → JSON` | Page list: index, width, height, rotation. |
| `apply_draw_ops` | `(data, ops_json, blobs) → bytes` | Apply draw queue to an existing document (incremental update). |
| `create_document` | `(ops_json, blobs) → bytes` | Build a new document (pages + draw ops) from scratch. |
| `measure_text` | `(font, size, text) → width` | Text width from standard-14 metrics. |

The `ops_json` payload is a versionless ordered op list, e.g.
`[{ "op": "addPage", "width": 595, "height": 842 }, { "op": "text", "page": 0, ... }]`.
Image bytes travel in the `blobs` buffer with offsets referenced from the JSON, the same
mechanism the fill queue uses for signature images today.

### Rust module layout

```
crates/core/src/
  lib.rs            wasm exports (existing + 4 new)
  forms.rs, fill.rs, flatten.rs, appearance.rs, font_metrics.rs   (existing, unchanged)
  pages.rs          read page info for read_pages
  create.rs         new-document skeleton (catalog, page tree)
  draw/
    mod.rs          op deserialization, dispatch
    content.rs      ops → PDF content-stream operators
    images.rs       JPEG/PNG → image XObjects (reuses signature decode path)
    resources.rs    font/XObject/ExtGState registration on page resources
```

Reuse: `appearance.rs` already emits text-rendering operators and `font_metrics.rs`
already carries standard-14 widths; the signature image path already decodes JPEG/PNG.
The draw module builds on all three rather than duplicating them.

## 4. Public API

Shaped after pdf-lib (a migration guide already exists; mirroring lowers switching cost).
Coordinates use the PDF convention: origin at the bottom-left of the page, y grows upward.

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb, grayscale } from "@ignaciano3/better-pdf";

// New document
const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);            // or [width, height]; default A4
const font = doc.getFont(StandardFonts.Helvetica);

page.drawText("Hello world", {
  x: 50, y: 780, size: 24,
  font,                                            // default: Helvetica
  color: rgb(0, 0, 0),                             // default: black
  lineHeight: 28,                                  // for multiline strings ("\n")
  opacity: 1,                                      // 0..1, applies to all draw ops (ExtGState)
});

const img = await doc.embedPng(pngBytes);          // also: doc.embedJpg(jpgBytes)
page.drawImage(img, { x: 50, y: 400, width: 200, height: 100 });
// img.width / img.height expose intrinsic pixel size; img.scale(0.5) helper

page.drawLine({ start: { x: 50, y: 300 }, end: { x: 250, y: 300 }, thickness: 2, color: rgb(1, 0, 0) });
page.drawRectangle({ x: 50, y: 100, width: 200, height: 80, color: rgb(0.9, 0.9, 0.9), borderColor: rgb(0, 0, 0), borderWidth: 1, opacity: 0.5 });
page.drawEllipse({ x: 150, y: 140, xScale: 100, yScale: 40, color: rgb(0, 0, 1) });

const bytes = await doc.save();

// Existing document — identical draw API
const loaded = await PdfDocument.load(existingBytes);
loaded.getPageCount();
loaded.getPages();                                  // PdfPage[]
const p0 = loaded.getPage(0);                       // throws RangeError-style PdfError when out of bounds
p0.drawText("APPROVED", { x: 400, y: 700, size: 36, color: rgb(1, 0, 0), opacity: 0.4 });
```

- `PdfFont.widthOfTextAtSize(text, size)` calls the `measure_text` WASM helper.
- Forms and drawing compose on the same document: fill fields, flatten, and stamp text in
  one `save()`. Draw ops are applied last, so stamps land above flattened content.
- New error type `InvalidImageError` (extends `PdfError`) for unsupported image bytes,
  consistent with the existing error hierarchy; out-of-range pages and invalid draw
  options throw `PdfError` subclasses, never raw WASM errors.

## 5. Packaging

Single npm package, subpath exports, no breaking change:

```
src/
  index.ts            root entry — re-exports everything (back-compat)
  index.browser.ts    browser root entry (same surface, browser WASM init)
  core/               document.ts, wasm.ts, wasm-browser.ts, errors.ts
  forms/              form.ts, fields.ts, schema.ts, typegen.ts, index.ts
  generate/           page.ts, draw-queue.ts, image.ts, fonts.ts, color.ts, index.ts
```

`package.json` exports gains:

- `./forms` → form/field classes, errors, typegen, schema types.
- `./generate` → `PageSizes`, `StandardFonts`, `rgb`/`grayscale`, draw option types.

`PdfDocument` is exported only from the root entries (`.` and `./browser`), because its
`load()` is runtime-specific (Node reads the WASM binary from disk; the browser entry
initializes it asynchronously). The subpaths stay runtime-neutral: they export only code
with no WASM imports, so either entry can compose with them.

`core/document.ts` is extracted in M20, when `save()` grows draw logic: today
`PdfDocument` is duplicated across `index.ts` and `index.browser.ts` (the only difference
is WASM initialization), and adding draw handling to both copies would double the
maintenance. M19 keeps the two entry files at the root untouched.

`sideEffects: false` already lets bundlers tree-shake unused classes; the
subpaths are primarily about a focused, discoverable surface per audience. Install size
is dominated by the single WASM binary and does not change; a per-feature WASM split is
explicitly deferred.

The existing `./browser` and `./typegen` subpaths and the CLI bin are unchanged.

## 6. Testing

- **Unit/integration (bun test):** per-feature test files following the existing layout —
  `create.test.ts`, `draw-text.test.ts`, `draw-image.test.ts`, `draw-shapes.test.ts`,
  `pages.test.ts`, plus draw-on-existing tests against the fixture corpus.
- **Structural validation:** every generated/modified PDF passes the existing
  qpdf-validate harness.
- **Render checks:** extend `scripts/render-check.ts` (pdfjs-dist) to render generated
  documents and assert drawn text is extractable and pages rasterize without errors.
- **Cross-check against pdf-lib:** pdf-lib is already a devDependency; tests compare
  coordinate placement and text width calculations for a sample of draw calls.
- **Fuzzing:** new fuzz target for the draw-ops JSON deserializer, alongside the existing
  four targets.
- **Type-level:** subpath exports compile under `tsc --noEmit` from both entries; the
  restructure milestone must keep `tests/types/typed-form.types.ts` green.

## 7. Milestones

1. **M19 — Restructure:** move TS sources into `core/` and `forms/`, add `./forms`
   subpath export, root re-exports unchanged. Delete the 17 unreferenced fixture PDFs.
   Zero behavior change; all existing tests pass unmodified.
2. **M20 — Pages + draw text on existing PDFs:** `read_pages`, `getPage`/`getPages`/
   `getPageCount`, draw queue, `drawText`, `apply_draw_ops` with incremental update and
   `q`/`Q` wrapping. `./generate` subpath lands here.
3. **M21 — Create documents:** `PdfDocument.create()`, `addPage`, `PageSizes`,
   `create_document` building the skeleton and reusing the draw machinery.
4. **M22 — Images:** `embedJpg`/`embedPng`, `drawImage`, image XObject plumbing reusing
   the signature decode path; `InvalidImageError`.
5. **M23 — Vector graphics:** `drawLine`, `drawRectangle`, `drawEllipse`, opacity via
   ExtGState, `rgb`/`grayscale` helpers.
6. **M24 — Polish:** `measure_text` + `widthOfTextAtSize`, docs site, README, pdf-lib
   migration guide update, benchmarks for generation, changelog, version bump.

Each milestone is independently shippable and gets its own implementation plan.
