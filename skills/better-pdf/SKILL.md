---
name: better-pdf
description: Fill and flatten PDF AcroForm fields (text, checkbox, radio, dropdown, list box, visual signature) in existing PDFs with the @ignaciano3/better-pdf npm package, generate TypeScript types from a PDF form for compile-time-safe filling, create brand-new AcroForm fields on created or loaded PDFs with doc.createForm(), reset fields to their defaults, toggle field flags (readOnly/required/hidden/print/multiline/comb/password), read encrypted PDFs with load(bytes, {password}) plus isEncrypted()/passwordType() probes, create new PDFs from scratch, draw text with custom TTF/OTF fonts (full Unicode including CJK, word wrap via maxWidth, rotate/opacity), fill form fields with embedded Unicode fonts, add document outlines/bookmarks, draw images (transparent PNGs, rotate/skew/flip/opacity) and vector graphics (lines, rectangles, ellipses, SVG paths including arcs, polygons, dashed strokes), add clickable link annotations, embed file attachments (ZUGFeRD/Factur-X /AF structure), read/write PDF metadata, merge/assemble/extract/split pages, add/insert/remove/move/rotate/resize pages, embed pages from other PDFs as Form XObjects, and compress output with save({compress, objectStreams}). Use when filling or flattening PDF forms, reading AcroForm fields, creating form fields, embedding a visual signature image, opening password-protected PDFs, creating PDF documents, drawing Unicode/rotated/translucent/wrapped text, adding bookmarks, embedding images or attachments, drawing vector paths, adding hyperlinks, reading or setting metadata, merging PDFs, extracting/reordering/inserting/removing/rotating pages, stamping a page from another PDF, or when the user mentions better-pdf, pdf-lib, or AcroFields.
---

# better-pdf

A maintained, fast alternative to pdf-lib for **filling and flattening AcroForm fields in existing PDFs**, plus full document generation. Rust→WASM core + a fully-typed TS API; runs in Node/Bun/Deno and the browser. Zero runtime npm deps. Stable since 1.0.0 (semver frozen public API; current 1.14.x).

## Quick start

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);        // Uint8Array | ArrayBuffer
const form = doc.getForm();

for (const f of form.getFields()) {
  console.log(f.type, f.name, f.states, f.options); // inspect BEFORE writing
}

form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getCheckBox("conformidad.acepto").check();
const out = await doc.save();                      // Promise<Uint8Array>
```

`load()` and `save()` are async (WASM init). `getForm()` returns the same instance each call, so mutations accumulate and apply on `save()`.

## Critical rules (agents get these wrong)

1. **Use the field's REAL export values — never assume `"Yes"`/`"On"`.** Corpus values are domain-specific (`F`/`M`, `SI`/`NO`, `Titular`/`Familiar`). Read them from `field.states` (checkbox/radio) or `field.options` (dropdown/list box). `checkBox.check()` uses the field's actual on-state automatically; `uncheck()` sets `Off`.
2. **Both modes exist:** `PdfDocument.load(bytes)` (edit an existing PDF, incremental save) and `PdfDocument.create()` (build one from scratch, full save). Most generation APIs — drawing, form-field creation, outlines, attachments, metadata, page ops — work in **both** modes; the per-API notes below flag the exceptions.
3. **Signatures are visual only** — an embedded image/appearance, NOT cryptographic/PAdES signing. `getSignature(name).setImage(jpegOrPngBytes)`.
4. **`save()` on a loaded doc is an incremental (append-only) update** — output begins with the original bytes verbatim, so signatures on the original revision stay valid. With nothing queued it returns a byte-identical round-trip. `save()` always starts from the loaded bytes; `FieldInfo.value` reflects queued mutations immediately.
5. **Wrong-type access throws** (e.g. `getDropdown()` on a text field), and invalid options/states throw before save. Errors subclass `PdfError`: `UnknownFieldError`, `FieldTypeError`, `InvalidOptionError`, `MaxLengthExceededError`, `MissingOnStateError`, `MultiSelectError`, `MissingGlyphError`, `InvalidImageError`, `InvalidRotationError`, `PageOutOfRangeError`, `FormSealedError`, `EncryptedPdfError`, `IncorrectPasswordError`, `DuplicateAttachmentError`, `AttachmentNotFoundError`, `PdfCoreError` (core rejections at save time: XFA forms, CMYK JPEGs, malformed PDFs).
6. **Missing glyphs throw** (since 1.11.0). `drawText` and embedded-font form fill raise `MissingGlyphError` when the font lacks a character. Opt out for drawing only: `drawText(text, { font, onMissingGlyph: "skip" })`.

## Typed filling (recommended workflow)

Generate a types module from the PDF, then pass it to `getForm` for compile-time safety — unknown field names, wrong-type access, and invalid option/state values become **compile errors**, at zero runtime cost:

```bash
npx better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
npx better-pdf-generate-types secured.pdf --name Secured --password s3cret  # encrypted input
```

```ts
import { myFormFields } from "./form-types.js";          // generated `…Fields` const

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
form.reset();                                                   // typed too
```

- The generated module carries every readable field property (`type`, `value`, `defaultValue`, `states`, `options`, `readOnly`, `required`, `multiSelect`, `password`, `multiline`, `comb`, `editable`, `align`, `tooltip`, `fontName`, `fontSize`, widget geometry, plus a deduplicated `pages` tuple).
- Use `--password ''` for owner-locked files.
- The pure generator is also importable: `import { generateFormTypes } from "@ignaciano3/better-pdf/typegen"` (WASM-free, tree-shakeable).

## Reading fields

`form.getFields(): FieldInfo[]`, `form.getField(name): FieldInfo | undefined`.

```ts
FieldInfo = {
  name, type,            // fully-qualified name; text|checkbox|radio|dropdown|listbox|signature|pushbutton|unknown
  value, defaultValue,   // string | null  (/V and /DV)
  states, options,       // string[] — checkbox/radio on-states; dropdown/listbox options
  readOnly, required, exported,   // /Ff ReadOnly, Required, NoExport (exported === !NoExport)
  maxLength,             // text /MaxLen or null
  multiSelect, password, multiline, comb, editable,  // flag reads
  align,                 // "left" | "center" | "right" (from widget /Q)
  tooltip,               // /TU or null
  fontName, fontSize,    // effective /DA font + size; fontSize 0 means auto-size; null when N/A
  widgets: { page, rect: [x0,y0,x1,y1], hidden, print, noView }[],  // 0-based page, PDF points
}
```

Hierarchical (dotted) names and orphaned widget fields (widgets on a page never linked into `/AcroForm/Fields`) are resolved and fillable.

## Writing fields

```ts
form.getTextField(n).setText(v, { font? })      // throws past maxLength; font = embedded PdfFont
form.getTextField(n).setDefaultText(v, { font? })
form.getTextField(n).setMultiline(true)         // regenerates the appearance
form.getTextField(n).setComb(true, 9) / .setComb(false)
form.getTextField(n).setPassword(true)          // empty appearance; /V preserved
form.getCheckBox(n).check() / .uncheck() / .setDefaultChecked(b)
form.getRadioGroup(n).select(v) / .setDefaultSelected(v)
form.getDropdown(n).select(v) / .setDefaultSelected(v)
form.getListBox(n).select(v) / .selectMultiple([...]) / .setDefaultSelected(v)
form.getSignature(n).setImage(jpegOrPngBytes)

// flags on any field (PdfField base class)
f.setReadOnly(b) / f.setRequired(b) / f.setExported(b)      // /Ff bits
f.hide() / f.show() / f.setPrintable(b) / f.setNoView(b)    // widget /F bits

form.resetField(name) / form.reset()   // restore /DV, or clear; reset() skips signature + pushbutton
form.flattenField(name) / form.flatten()
```

- `selectMultiple` throws `MultiSelectError` on a single-select list box.
- Multiline text fields fill with wrapped, top-aligned appearances (hard `\n` preserved, per-line quadding honored). No mid-word breaking — an over-wide word overflows onto its own line.
- Flattened fields are stamped onto the page and removed from the AcroForm (no longer editable).

## Embedded-font form fill (CJK / Unicode)

```ts
const font = await doc.embedFont(new Uint8Array(await Bun.file("NotoSansJP.ttf").arrayBuffer()));
form.getTextField("full_name").setText("山田太郎", { font });
```

- Plain and multiline **text fields only**, loaded or builder-created. Comb, dropdown, and list box reject an embedded font (`FieldTypeError`); those stay standard-14.
- Values are written to `/V`/`/DV` as UTF-16BE and round-trip.
- A `setText({ font })` call **cannot** be combined with `insertPage`/`removePage`/`movePage` in the same `save()` — save separately before or after the page-structure change.
- Passing a standard-14 handle as `{ font }` throws; omit `font` for standard-14 rendering.

## Creating form fields

`doc.createForm()` returns a chainable `FormBuilder`. Works on **created and loaded** documents.

```ts
import { PdfDocument, PageSizes, rgb, StandardFonts } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
doc.addPage(PageSizes.A4);

const form = doc.createForm()
  .addTextField("applicant.name", {
    page: 0, x: 56, y: 740, width: 240, height: 22,
    value: "GARCIA", defaultValue: "", maxLength: 64,
    align: "left", fontSize: 12, font: StandardFonts.Helvetica, textColor: rgb(0, 0, 0),
    border: { color: rgb(0.1, 0.1, 0.4), width: 1 }, background: rgb(0.97, 0.97, 1),
    required: true, tooltip: "Full legal name",
  })
  .addTextField("applicant.notes", { page: 0, x: 56, y: 660, width: 240, height: 60, multiline: true })
  .addTextField("applicant.ssn",   { page: 0, x: 56, y: 630, width: 180, height: 22, comb: true, maxLength: 9 })
  .addCheckBox("applicant.agree",  { page: 0, x: 56, y: 600, size: 14, checked: true, onValue: "SI", checkStyle: "cross" })
  .addRadioGroup("applicant.kind", {
    selected: "primary", checkStyle: "circle",
    options: [
      { value: "primary",   page: 0, x: 56,  y: 570, size: 14 },
      { value: "dependent", page: 0, x: 120, y: 570, size: 14 },
    ],
  })
  .addDropdown("applicant.status", { page: 0, x: 56, y: 530, width: 160, height: 22,
    options: ["single", "married"], selected: "married", editable: true })
  .addListBox("applicant.plan",    { page: 0, x: 56, y: 470, width: 160, height: 48,
    options: ["basic", "plus", "premium"], multiSelect: true })
  .addSignatureField("applicant.signature", { page: 0, x: 56, y: 410, width: 200, height: 48 });

console.log(form.getFieldNames());   // typed array of declared names
const out = await doc.save();
```

- Six field types: `addTextField`, `addCheckBox`, `addRadioGroup`, `addDropdown`, `addListBox`, `addSignatureField`.
- Shared options: `page`, `x`, `y`, `required`, `readOnly`, `tooltip`, `border` (`{ color, width? }`), `background`, `textColor`. Choice/text also take `align`, `fontSize`, `font`.
- `checkStyle`: `"check" | "cross" | "circle" | "square" | "diamond" | "star"` (checkbox default `check`, radio default `circle`).
- `comb` requires `maxLength` and is incompatible with `multiline`. `password: true` masks display only (not encryption). `editable` is dropdown-only; `multiSelect` is list-box-only (`addDropdown` rejects it).
- Text fields accept an embedded `PdfFont` via `font`; choice fields are standard-14 only.
- Generated widgets set the `/F` Print flag, so created fields appear in printed output.
- **On a loaded doc:** declare every field *before* the first `getForm()`/`save()` — fields are injected then, and a later `createForm()` throws. Colliding names are rejected. Embedded-font fill of a field created this way on a loaded doc is not yet supported.
- **On a created doc:** the first `getForm()` materializes and **seals** the document — adding more fields, pages, or drawings afterward throws `FormSealedError`.

## Encrypted PDFs

```ts
if (await PdfDocument.isEncrypted(bytes)) {
  const kind = await PdfDocument.passwordType(bytes, pw);  // "owner" | "user" | null
  const doc = await PdfDocument.load(bytes, { password: pw });
}
```

- RC4 / AES-128 / AES-256 decryption. Use `{ password: "" }` for owner-locked / empty-user-password files.
- Opt-in: bare `load(bytes)` on an encrypted file throws `EncryptedPdfError`; a wrong password throws `IncorrectPasswordError`.
- `passwordType(…) !== null` exactly when `load({ password })` succeeds (both classic-`trailer` and xref-stream files, since 1.14.3).
- **Saving an edited encrypted PDF produces decrypted output.** Producing encrypted output is unsupported.

## Save options (compression)

```ts
await doc.save();                        // deflate-compresses generated streams (default)
await doc.save({ compress: false });     // plaintext streams (debugging / byte assertions)
await doc.save({ objectStreams: true }); // + object streams & xref streams (full-document saves only)
await PdfDocument.merge([a, b], { objectStreams: true });   // ManipulateOptions
```

- Already-filtered streams (images, font programs) are never double-compressed.
- `objectStreams` (default `false`) applies only to full-document paths (`create()`, `merge`, `assemble`, `copyPages`, `splitPages`); incremental saves ignore it. It raises output to PDF 1.5+ and is **not** PDF/A-1 conformant.
- Consumers that snapshot raw saved bytes should pass `{ compress: false }`.

## Embedded fonts (Unicode / CJK)

Embed any TTF or OTF font to render Unicode text as a Type0/CIDFontType2 composite font with a ToUnicode CMap — text stays selectable and searchable.

```ts
const font = await doc.embedFont(fontBytes, { subset: true });   // subset defaults to true
const w = font.widthOfTextAtSize("日本語テキスト", 18);
page.drawText("日本語テキスト", { x: (595 - w) / 2, y: 700, size: 18, font });
```

- Works on created and loaded documents; shared between `drawText` and form fill in the same `save()`, and subsetting picks up glyphs used by fill values.
- OpenType-CFF (`.otf` with CFF outlines) may fail to subset — use `{ subset: false }`.
- Standard-14 fonts (`doc.getFont(StandardFonts.HelveticaBold)`) remain the default when no `font` is passed.

## API reference

| Call | Purpose |
|------|---------|
| `PdfDocument.load(bytes, { password? })` | Load an existing PDF (decrypting if a password is given) |
| `PdfDocument.create()` | New empty document |
| `PdfDocument.isEncrypted(bytes)` / `.passwordType(bytes, pw)` | Probe encryption without loading |
| `PdfDocument.merge(docs, opts?)` | Merge multiple PDFs (all pages, in order) |
| `PdfDocument.assemble(docs, selections, opts?)` | New PDF from an explicit ordered `{docIndex, pageIndex}[]` selection |
| `doc.copyPages(indices, opts?)` / `doc.splitPages(opts?)` | Extract given pages / one single-page PDF per page (load mode only) |
| `doc.save(opts?)` → `Promise<Uint8Array>` | Apply all queued changes; `SaveOptions = { compress?, objectStreams? }` |
| `doc.getPageCount()` / `doc.getPages()` / `doc.getPage(i)` | Page access (0-based; out of range → `PageOutOfRangeError`) |
| `doc.addPage(size?)` / `insertPage(i, size?)` / `removePage(i)` / `movePage(from, to)` | Page structure — loaded and created |
| `page.setRotation(deg)` / `setSize(w, h)` / `setMediaBox(x0,y0,x1,y1)` | Rotate (multiple of 90) / resize |
| `doc.embedJpg(bytes)` / `doc.embedPng(bytes)` → `PdfImage` | Embed an image; `image.width/height`, `image.scale(f)` |
| `doc.embedPdfPage(src, pageIndex)` → `EmbeddedPdfPage` | Import a page from another PDF as a Form XObject |
| `doc.embedFont(bytes, { subset? })` → `PdfFont` | Embed TTF/OTF; `font.widthOfTextAtSize(text, size)` |
| `doc.getFont(StandardFonts.X)` → `PdfFont` | Standard-14 font handle |
| `page.drawText(text, opts)` | See options below |
| `page.drawImage(image, opts)` / `page.drawPage(embedded, opts)` | `{ x, y, width?, height?, opacity?, rotate?, xSkew?, ySkew? }` (+ `flipX`/`flipY` on images, unreleased) |
| `page.drawLine({ start, end, stroke?, strokeWidth?, opacity?, dash?, dashPhase? })` | Line |
| `page.drawRectangle({ x, y, width, height, fill?, stroke?, strokeWidth?, opacity?, dash?, dashPhase? })` | Rectangle (x,y = lower-left) |
| `page.drawEllipse({ x, y, radiusX, radiusY, fill?, stroke?, strokeWidth?, opacity?, dash?, dashPhase? })` | Ellipse (x,y = center) |
| `page.drawSvgPath(d, opts)` / `page.drawPolygon(points, opts)` | Vector paths; same fill/stroke/dash options (`closed?` on polygon) |
| `page.drawLink({ x, y, width, height, url \| goToPage })` | External URI or internal page-jump annotation |
| `doc.setOutline(items)` | Bookmarks tree: `{ title, page, children? }[]`, `page` 0-based |
| `doc.setTitle/setAuthor/setSubject/setKeywords/setCreator/setProducer/setCreationDate/setModificationDate` | Info dictionary writes |
| `await doc.getMetadata()` → `DocumentMetadata` | `{ title?, author?, subject?, keywords?: string[], creator?, producer?, creationDate?: Date, modificationDate?: Date }` |
| `doc.attach(bytes, name, opts?)` / `await doc.getAttachments()` | Embedded files (`/EmbeddedFiles`) |
| `doc.createForm()` → `FormBuilder` | Declare new AcroForm fields (created or loaded doc) |
| `doc.getForm()` / `doc.getForm<typeof schema>()` | Untyped / type-narrowed form view |
| `generateFormTypes(fields, { typeName })` | Emit a typed `…Fields` module (string) |

`drawText` options: `{ x, y, size, font?, color?, lineHeight?, rotate?, opacity?, maxWidth?, onMissingGlyph? }` — `rotate` in degrees counter-clockwise about the anchor, `opacity` 0–1, `maxWidth` word-wraps to that width in points (`\n`, `\r\n`, `\r` are hard breaks), `onMissingGlyph: "throw" | "skip"` (default `throw`).

All coordinates are PDF user space: **origin bottom-left, y-up**, units in points.

## Rotated & translucent text (watermark)

```ts
import { PdfDocument, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);
const font = doc.getFont(StandardFonts.HelveticaBold);

for (const page of doc.getPages()) {
  page.drawText("CONFIDENTIAL", {
    x: 150, y: 300, size: 60, font, color: rgb(0.8, 0, 0),
    rotate: 45, opacity: 0.15,
  });
}
await Bun.write("watermark.pdf", await doc.save());
```

## Document outlines / bookmarks

```ts
doc.setOutline([
  { title: "Introduction", page: 0 },
  { title: "Chapter 1", page: 1, children: [
      { title: "1.1 Background", page: 1 },
      { title: "1.2 Methods",    page: 2 },
  ]},
  { title: "Conclusion", page: 5 },
]);
```

`page` is 0-based. Children nest to arbitrary depth. Loaded and created documents.

## Page operations (merge, extract, split, assemble)

```ts
const merged    = await PdfDocument.merge([bytesA, bytesB, bytesC]);
const doc       = await PdfDocument.load(bytes);
const extracted = await doc.copyPages([0, 2, 4]);
const pages     = await doc.splitPages();          // Uint8Array[]
const result    = await PdfDocument.assemble([cover, body, annex], [
  { docIndex: 0, pageIndex: 0 },
  { docIndex: 1, pageIndex: 2 },
  { docIndex: 2, pageIndex: 0 },
]);
```

- All return new bytes; sources are never mutated. `copyPages`/`splitPages` require a loaded document (they throw on created docs).
- Form fields on merged/assembled pages stay **interactive**: a working `/AcroForm` is rebuilt with merged `/DR` fonts and `/NeedAppearances true`. Colliding names are prefixed per source (`d0_`, `d1_`, …). `/XFA` data is dropped; a page selected twice shares its field objects.
- `insertPage`/`removePage`/`movePage` on loaded docs are reflected after save + reload. **Nested page trees are rejected** — use `merge`/`assemble` for those.

## Embed pages from other PDFs (watermark, letterhead, N-up)

```ts
const doc   = await PdfDocument.load(docBytes);
const stamp = await doc.embedPdfPage(letterheadBytes, 0);
for (const page of doc.getPages()) page.drawPage(stamp, { x: 0, y: 0 });
```

`width`/`height` default to the source page's intrinsic MediaBox size. Interactive fields and annotations on the embedded page are **not** carried over — static appearance only.

## Vector paths

```ts
page.drawSvgPath("M 150 250 L 80 120 L 220 120 Z", {
  fill: rgb(0.2, 0.5, 0.9), stroke: rgb(0.1, 0.3, 0.7), strokeWidth: 1.5,
  opacity: 0.9, dash: [4, 2], dashPhase: 0,
});
page.drawPolygon([{ x: 300, y: 250 }, { x: 250, y: 150 }, { x: 350, y: 150 }],
  { fill: rgb(0.9, 0.6, 0.1), strokeWidth: 1, closed: true });
```

- Supported SVG commands: `M L H V C S Q T Z` **and arcs `A`/`a`** (converted to cubic béziers, since 0.19.0), absolute and relative.
- SVG artwork authored y-down appears flipped — negate y or transform before passing.

## Images

```ts
const img = await doc.embedPng(pngBytes);           // or embedJpg
page.drawImage(img, { x: 50, y: 400, ...img.scale(0.5), opacity: 0.8, rotate: 15 });
```

- Transparent PNGs (RGBA, gray+alpha, palette/indexed with `tRNS`) keep their alpha as a `/SMask`. Interlaced and 16-bit-per-channel PNGs are unsupported; CMYK JPEG is rejected (`PdfCoreError`).
- `rotate` / `xSkew` / `ySkew` are applied via the CTM about `(x, y)`; `opacity` via ExtGState, composing with the soft mask.

## Link annotations

```ts
page.drawLink({ x: 50, y: 746, width: 140, height: 18, url: "https://example.com" });
page.drawLink({ x: 50, y: 700, width: 200, height: 18, goToPage: 2 });   // 0-based
```

Exactly one of `url` or `goToPage` (throws otherwise). Border is invisible by default. Named destinations and cross-document `GoToR` jumps are unsupported.

## File attachments

```ts
doc.attach(xmlBytes, "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  afRelationship: "Alternative",   // Source|Data|Alternative|Supplement|EncryptedPayload|FormData|Schema|Unspecified
  creationDate: new Date(),
});
const list = await doc.getAttachments();  // { name, description?, mimeType?, size, bytes, ... }[]
```

Queued and written to `/EmbeddedFiles` at `save()`, created or loaded docs. `afRelationship` also sets the filespec `/AFRelationship` and appends to the catalog `/AF` array (ZUGFeRD/Factur-X structure — PDF/A-3 XMP conformance metadata is **not** written). Duplicate names throw `DuplicateAttachmentError`.

## Document metadata

```ts
doc.setTitle("Report"); doc.setAuthor("Alice"); doc.setKeywords(["finance", "Q2"]);
doc.setCreationDate(new Date("2026-01-01T00:00:00Z"));
const meta = await doc.getMetadata();   // meta.modificationDate (renamed from modDate in 0.20.0)
```

Non-ASCII text is encoded UTF-16BE and round-trips. On a loaded PDF the setters emit an incremental update and preserve untouched Info keys. Only the Info dictionary is written — XMP streams are not modified.

## Browser & bundlers

Import from `@ignaciano3/better-pdf/browser` — same API — and call `initializeWasm(wasmUrl)` before any PDF operation, passing the URL of the `@ignaciano3/better-pdf/wasm` export subpath (`?url` in Vite, `new URL(…, import.meta.url)` in webpack, `public/` copy in Next.js). Node, Bun, and Deno self-initialize. Runtime starters live in `examples/runtimes/`.

## Not supported

XFA forms (rejected on fill/flatten; static AcroForm reads still work) · cryptographic/PAdES signing · producing encrypted output · CMYK color · XMP metadata · nested page trees in insert/remove/move · interlaced or 16-bit PNGs · embedded fonts on comb/dropdown/list-box fields · toggling `multiline`/`comb`/`password` is supported on loaded **text** fields only.
