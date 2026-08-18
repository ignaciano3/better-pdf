# better-pdf

A maintained, fast alternative to `pdf-lib` for PDF AcroForms and document generation.

`better-pdf` exposes a TypeScript API backed by a Rust core compiled to WebAssembly. It covers two workflows: (1) **AcroForm-first** — load an existing PDF, inspect fields, fill/flatten/sign, and save an incremental update; and (2) **generate & draw** — create new PDFs from scratch or stamp text, images, and vector graphics onto existing pages.

> **Status:** 1.0.0 — stable. The full feature set — AcroForm reading/filling/flattening/visual-signatures/typed-form generation (incl. multi-line text fields and multi-select list boxes), PDF generation and drawing (text, images, rectangles, lines, ellipses, SVG paths/arcs, polygons, links), custom TTF/OTF font embedding with Unicode/CJK, document metadata, outlines, page operations (merge/copy/reorder/split/insert/remove/move), page rotation/resize, PNG transparency + palette, PDF page embedding, and encrypted-PDF decryption (RC4 / AES-128 / AES-256 via `load(bytes, { password })`) — is implemented and tested. The public API is frozen as of 1.0.0 (the final breaking changes landed in 0.20.0 — see the [0.19 → 0.20 migration guide](docs/site/src/content/docs/migrating/0.19-to-0.20.md)); the package follows Semantic Versioning.

Coming from pdf-lib? See the [migration guide](docs/migrating-from-pdf-lib.md).

From 1.0.0, the package follows Semantic Versioning — breaking changes only in major releases. See [docs/STABILITY.md](docs/STABILITY.md) for the full policy.

## Runtime support

| Runtime | Init | Status |
| --- | --- | --- |
| Node.js ≥ 18 | zero-config — wasm self-initializes on import | **Verified** (Node v24.16.0) |
| Bun | zero-config — wasm self-initializes on import | **Verified** (Bun v1.3.14) |
| Deno | zero-config — use `npm:@ignaciano3/better-pdf` specifier | Config provided |
| Browser (Playwright) | `initializeWasm(wasmUrl)` | **Verified** |
| Vite | `initializeWasm(wasmUrl)` — `import wasmUrl from "@ignaciano3/better-pdf/wasm?url"` | **Verified** (Vite v5.4.21 build) |
| webpack 5 | `initializeWasm(wasmUrl)` — `new URL("@ignaciano3/better-pdf/wasm", import.meta.url)` | **Verified** (webpack v5.107.2 build) |
| Next.js | `initializeWasm("/better_pdf_core_bg.wasm")` — copy wasm to `public/` | **Verified** (Next.js v15.5.19 build) |
| Cloudflare Workers | `initializeWasm(wasmModule)` — import wasm as a module; use `/browser` entry | Config provided |

"Verified" = ran end-to-end (create + draw + save, valid `%PDF-` output) in this environment.
"Config provided" = runnable example + config shipped; toolchain absent from this environment.

See the [per-runtime guide](docs/site/src/content/docs/guides/runtimes.md) and [examples/runtimes/](examples/runtimes/) for working code.

## Features

- Read AcroForm fields with fully-qualified names, types, values, options, and button states.
- Fill text fields and text areas.
- Check/uncheck checkboxes using the real on-state value.
- Select radio options using real export values.
- Select dropdown and list-box options.
- Add visual-only signature images from JPEG or supported PNG bytes.
- Flatten one field or all fields after filling.
- Save append-only incremental PDF updates.
- Create new PDFs with `PdfDocument.create()` and standard page sizes.
- Draw text, images, lines, rectangles, and ellipses on new and existing pages.
- Embed custom TTF/OTF fonts with glyph subsetting for full Unicode text (CJK, accented Latin, any script) — selectable and searchable in PDF viewers.
- Create fillable AcroForm fields (text, checkbox, radio, dropdown, listbox, signature) with `doc.createForm()` — on generated documents, and on documents opened with `PdfDocument.load()` (added fields must precede the first `getForm()`).
- Read and write document metadata (Title, Author, Subject, Keywords, Creator, Producer, CreationDate, ModDate) via `doc.setTitle()` / `doc.getMetadata()` on both created and loaded documents; dates round-trip to JS `Date`.
- Merge multiple PDFs into one with `PdfDocument.merge([a, b, c])`.
- Extract or reorder pages from a loaded document with `doc.copyPages([0, 2, 4])`.
- Split a loaded document into single-page PDFs with `doc.splitPages()`.
- Assemble a new PDF from an explicit cross-document page selection with `PdfDocument.assemble(docs, selections)`.
- Rotate individual pages with `page.setRotation(degrees)` and resize them with `page.setSize(width, height)` or `page.setMediaBox(x0, y0, x1, y1)` — on both loaded and created pages.
- Embed transparent PNG images: the alpha channel of RGBA and gray+alpha PNGs is preserved as a PDF soft mask (`/SMask`). `embedPng` just works — no API change.
- Embed pages from other PDFs as Form XObjects with `doc.embedPdfPage(src, pageIndex)` + `page.drawPage(embedded, {x, y, width?, height?})` — watermarks, letterhead, N-up layouts, overlays. Works on loaded and created documents.
- Add clickable link annotations with `page.drawLink({ x, y, width, height, url })` (external URI) or `page.drawLink({ x, y, width, height, goToPage })` (internal page-index jump) on loaded and created documents.
- Draw vector paths: `page.drawSvgPath(d, { fill?, stroke?, strokeWidth?, opacity? })` (SVG path-data string; supports M/L/H/V/C/S/Q/T/Z and arcs A/a) and `page.drawPolygon(points, { fill?, stroke?, strokeWidth?, opacity?, closed? })` on loaded and created documents.
- Draw rotated and translucent text: `page.drawText(text, { rotate, opacity })` — `rotate` is free-angle degrees (counter-clockwise about the anchor), `opacity` is 0–1. Works on loaded and created documents.
- Build PDF bookmarks / navigation outline: `doc.setOutline(items)` where each item is `{ title: string; page: number; children?: OutlineItem[] }`. Nested to arbitrary depth. Works on loaded and created documents.
- Add, insert, remove, and move pages on loaded documents: `doc.addPage(size?)` appends a blank, immediately drawable page; `doc.insertPage(index, size?)`, `doc.removePage(index)`, and `doc.movePage(from, to)` restructure the page order (reflected after save + reload). Incremental — existing forms and content are preserved.
- Non-ASCII document metadata: `setTitle`/`setAuthor`/etc. encode non-Latin text (Japanese, accented Latin, Arabic, etc.) as UTF-16BE for correct round-trip fidelity.
- Palette (indexed-color) PNG embedding: `embedPng` handles color-type-3 PNGs with `tRNS` transparency — transparency is stored as a soft mask, same as RGBA PNGs. No API change.
- Decrypt and modify encrypted PDFs: `PdfDocument.load(bytes, { password })` decrypts RC4 / AES-128 / AES-256 encrypted PDFs (use `{ password: "" }` for owner-locked / empty-user-password files). Decryption is opt-in — bare `load(bytes)` is unchanged. Saving an edited encrypted PDF produces a **decrypted** output. Re-encryption and creating encrypted PDFs are still unsupported.
- Deflate-compress generated content, appearance, and font streams on save — on by default, opt out with `doc.save({ compress: false })`. Streams already compressed (images, embedded fonts) are left untouched.
- Optionally pack non-stream objects into PDF object streams for even smaller files on full-document saves — opt in with `doc.save({ objectStreams: true })` (created docs) or `PdfDocument.merge(docs, { objectStreams: true })`. Off by default; not applied to incremental (loaded-document) saves.
- **File attachments** — `doc.attach(bytes, name, { mimeType, description, afRelationship })`
  embeds files (`/EmbeddedFiles`), `doc.getAttachments()` reads them back (metadata + bytes).
  `afRelationship` writes the `/AFRelationship` + catalog `/AF` structure used by
  ZUGFeRD/Factur-X e-invoices.

## Install

```bash
bun add @ignaciano3/better-pdf
```

For local development from this repository:

```bash
bun install
bun run build
```

## Usage

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const input = new Uint8Array(await Bun.file("form.pdf").arrayBuffer());
const doc = await PdfDocument.load(input);
const form = doc.getForm();

for (const field of form.getFields()) {
  console.log(field.name, field.type, field.value);
}

form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA, IGNACIO");
form.getRadioGroup("beneficiario.tipo_beneficiario").select("Titular");
form.getDropdown("beneficiario.estado_civil").select("Casado");
form.getCheckBox("declaracion.acepta").check();

const signature = new Uint8Array(await Bun.file("signature.png").arrayBuffer());
form.getSignature("firma.titular").setImage(signature);

form.flattenField("beneficiario.apellidos_nombres");

const output = await doc.save();
await Bun.write("filled.pdf", output);
```

### Compression

`save()` deflate-compresses the content, appearance, and font streams it
generates, producing smaller PDFs. It is on by default; opt out to keep the
streams plaintext (e.g. for debugging or byte-level assertions):

```ts
const small = await doc.save();                    // compressed (default)
const plain = await doc.save({ compress: false }); // uncompressed
```

Streams that are already compressed (images, embedded fonts) are left
untouched, and incremental saves only compress the newly appended section — the
original revision's bytes are preserved, so existing digital signatures on it
stay valid.

For created and merged documents you can also pack the object structure into
object streams (PDF 1.5+), shrinking object-heavy files further:

```ts
const doc = await PdfDocument.create();
// ... add pages / fields ...
const small = await doc.save({ objectStreams: true });      // + object streams
const merged = await PdfDocument.merge([a, b], { objectStreams: true });
```

`objectStreams` defaults to `false`. It applies only to full-document saves
(`create()`, `merge`, `assemble`, `copyPages`, `splitPages`); it is ignored on
incremental (loaded-document) saves, which stay append-only.

### File attachments

```ts
const doc = await PdfDocument.load(bytes);
doc.attach(xmlBytes, "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  afRelationship: "Alternative",
});
const saved = await doc.save();

// later
const attachments = await (await PdfDocument.load(saved)).getAttachments();
```

`attach()` queues the file and writes it to `/EmbeddedFiles` at `save()`, on
both created and loaded documents. `afRelationship` additionally sets the
filespec `/AFRelationship` and appends the file to the catalog `/AF` array —
the structure ZUGFeRD/Factur-X e-invoices require. Attaching a name that
already exists (queued, or already in the document) throws
`DuplicateAttachmentError`.

## Generating & drawing

Import from the `./generate` subpath (or the package root — both export the same classes):

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";
```

### (a) Create a document, draw text

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);               // 595 × 842 pt

const font = doc.getFont(StandardFonts.Helvetica);
const text = "Hello, world!";
const textWidth = font.widthOfTextAtSize(text, 24);   // centre-align helper

page.drawText(text, {
  x: (PageSizes.A4[0] - textWidth) / 2,
  y: 750,
  size: 24,
  font,
  color: rgb(0.1, 0.2, 0.8),
});

const output = await doc.save();   // returns Uint8Array
await Bun.write("hello.pdf", output);
```

> **Coordinate system:** origin is bottom-left, y increases upward — same as pdf-lib and raw PDF.
> **Fonts:** standard-14 (Helvetica, HelveticaBold, Courier, CourierBold, TimesRoman, TimesBold, TimesItalic, TimesBoldItalic, and more) are the default; use `doc.embedFont(bytes)` for Unicode/CJK text (see [(f) Custom fonts](#f-custom-fonts)).

### (b) Stamp onto an existing PDF

```ts
import { PdfDocument, rgb } from "@ignaciano3/better-pdf";

const bytes = new Uint8Array(await Bun.file("existing.pdf").arrayBuffer());
const doc = await PdfDocument.load(bytes);

const imgBytes = new Uint8Array(await Bun.file("logo.png").arrayBuffer());
const img = await doc.embedPng(imgBytes);             // PdfImage with .width / .height
const scaled = img.scale(0.5);                        // { width, height }

const page = doc.getPage(0);
page.drawImage(img, { x: 40, y: 700, width: scaled.width, height: scaled.height });
page.drawText("Confidential", { x: 40, y: 680, size: 12, color: rgb(0.8, 0, 0) });

const output = await doc.save();
```

`embedJpg` works the same way for JPEG files. Both methods are available on loaded and created documents.

### (c) Vector graphics

```ts
// filled + bordered rectangle with transparency
page.drawRectangle({
  x: 50, y: 50, width: 200, height: 100,
  fill: rgb(0.9, 0.95, 1),
  stroke: rgb(0.2, 0.4, 0.8),
  strokeWidth: 2,
  opacity: 0.85,
});

// line
page.drawLine({
  start: { x: 50, y: 40 },
  end:   { x: 250, y: 40 },
  strokeWidth: 1.5,
  stroke: rgb(0.5, 0.5, 0.5),
});

// ellipse — (x, y) is the centre; radiusX/radiusY are the x and y radii
page.drawEllipse({ x: 150, y: 200, radiusX: 60, radiusY: 30, fill: rgb(1, 0.8, 0) });
```

### (d) Text layout with `widthOfTextAtSize`

```ts
const font = doc.getFont(StandardFonts.HelveticaBold);
const label = "Invoice #1234";
const w = font.widthOfTextAtSize(label, 16);
page.drawText(label, { x: pageWidth - w - 40, y: pageHeight - 60, size: 16, font });
```

### (e) Custom fonts (Unicode / CJK)

Embed any TTF or OTF font to draw Unicode text. The font is stored as a
Type0/CIDFontType2 composite with a ToUnicode CMap, so text is selectable and
searchable. Works on both created and loaded documents.

```ts
import { PdfDocument, PageSizes } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);

const fontBytes = new Uint8Array(await Bun.file("NotoSansCJK-Regular.ttf").arrayBuffer());

// subset: true (default) — strip unused glyphs; keeps file size small
const font = await doc.embedFont(fontBytes, { subset: true });

const text = "日本語テキスト — Héllo Wörld";
const textWidth = font.widthOfTextAtSize(text, 18);

page.drawText(text, {
  x: (PageSizes.A4[0] - textWidth) / 2,
  y: 700,
  size: 18,
  font,
});

const output = await doc.save();
await Bun.write("unicode.pdf", output);
```

> **OpenType-CFF:** The subsetter supports TrueType (`glyf`) outlines. `.otf`
> files with CFF outlines may fail to subset — pass `{ subset: false }` for
> those. **Characters with no glyph throw `MissingGlyphError`** by default —
> pass `page.drawText(text, { font, onMissingGlyph: "skip" })` to restore the
> old behavior of silently skipping them.

### (f) Creating form fields

On a document created with `PdfDocument.create()`, call `doc.createForm()` to get
a chainable `FormBuilder` and declare AcroForm fields. There are six field types —
`addTextField`, `addCheckBox`, `addRadioGroup`, `addDropdown`, `addListBox`, and
`addSignatureField` — each placed by `page` index plus a position/size in PDF
points. The fields are serialized into the document on `save()`.

```ts
import { PdfDocument, PageSizes, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
doc.addPage(PageSizes.A4);

const form = doc
  .createForm()
  .addTextField("applicant.name", {
    page: 0, x: 56, y: 740, width: 240, height: 22,
    value: "GARCIA, IGNACIO",
    maxLength: 64,
    border: { color: rgb(0.1, 0.1, 0.4), width: 1 },
    background: rgb(0.97, 0.97, 1),
  })
  .addTextField("applicant.notes", {
    page: 0, x: 56, y: 660, width: 240, height: 60, multiline: true,
  })
  .addCheckBox("applicant.agree", {
    page: 0, x: 56, y: 620, size: 14, checked: true, required: true,
  })
  .addRadioGroup("applicant.kind", {
    selected: "primary",
    options: [
      { value: "primary", page: 0, x: 56, y: 590, size: 14 },
      { value: "dependent", page: 0, x: 120, y: 590, size: 14 },
    ],
  })
  .addDropdown("applicant.status", {
    page: 0, x: 56, y: 550, width: 160, height: 22,
    options: ["single", "married"], selected: "married",
  })
  .addListBox("applicant.plan", {
    page: 0, x: 56, y: 500, width: 160, height: 48,
    options: ["basic", "plus", "premium"],
  })
  .addSignatureField("applicant.signature", {
    page: 0, x: 56, y: 440, width: 200, height: 48,
  });

console.log(form.getFieldNames()); // typed array of the declared names

const output = await doc.save();
await Bun.write("form.pdf", output);
```

> **A normal fillable form.** The result is a standard AcroForm: reload it with
> `PdfDocument.load(output)` and you can fill it (`getForm().getTextField(...)`,
> `.getCheckBox(...).check()`, …) and flatten it with this same library.

Every field supports `required`, `readOnly`, `tooltip`, and the optional
`border` (`{ color, width? }`) / `background` (a `Color`) appearance — colors come
from `rgb(r, g, b)` and `grayscale(v)` (0–1).

#### Add fields to an existing PDF

`createForm()` also works on a document opened with `PdfDocument.load()` — it
adds new AcroForm fields to a PDF that already exists (with or without a form
of its own):

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);
const form = doc.createForm();
form.addTextField("signature_date", {
  page: 0, x: 72, y: 120, width: 160, height: 22,
});

// Fill it in the same session — the field is injected on this first getForm() call.
doc.getForm().getTextField("signature_date").setText("2026-07-02");

const output = await doc.save();
```

> **Add fields before the first `getForm()`.** New fields are injected into
> the loaded PDF on the first `getForm()`/`save()` call, so every
> `createForm()`-declared field must be added before that first call — calling
> `createForm()` again afterward throws. A declared field name that collides
> with a field already in the PDF is rejected.

### (f2) Filling form fields with embedded fonts (CJK / Unicode)

`field.setText(value, { font })` / `.setDefaultText(value, { font })` accept an
embedded font from `doc.embedFont(bytes)`, so text-field values can carry any
Unicode script — not just the standard-14 WinAnsi fonts. Works on plain and
multiline text fields, on both loaded and builder-created documents.

```ts
const fontBytes = new Uint8Array(await Bun.file("NotoSansJP-Regular.ttf").arrayBuffer());
const font = await doc.embedFont(fontBytes);

form.getTextField("full_name").setText("山田太郎", { font });
```

> Passing `{ font }` with a standard-14 handle throws — embedded-font fill
> requires a font from `doc.embedFont()`. Comb, dropdown, and listbox fields
> reject an embedded font (`FieldTypeError`); they remain standard-14 only.
> A `setText({ font })` call cannot be combined with `insertPage`/`removePage`/
> `movePage` in the same `save()` — call `save()` separately before or after
> the page-structure change.

### (g) Page operations (merge, extract, split, assemble)

All four methods return a new `Uint8Array` — no source document is mutated.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

// Merge: combine all pages from multiple PDFs in order
const merged = await PdfDocument.merge([bytesA, bytesB, bytesC]);

// Extract / reorder pages (load mode only — open the doc first)
const doc = await PdfDocument.load(bytes);
const extracted = await doc.copyPages([2, 0, 1]);   // reorder: page 2 first

// Split into one PDF per page
const singlePages = await doc.splitPages();         // Promise<Uint8Array[]>

// Assemble from multiple sources with full page control
const result = await PdfDocument.assemble(
  [cover, body, annex],
  [
    { docIndex: 0, pageIndex: 0 },   // cover page 0
    { docIndex: 1, pageIndex: 0 },   // body page 0
    { docIndex: 1, pageIndex: 1 },   // body page 1
    { docIndex: 2, pageIndex: 0 },   // annex page 0
  ],
);
await Bun.write("assembled.pdf", result);
```

> **Form fields on merged/assembled pages stay interactive (0.15.0).** A working
> `/AcroForm` is rebuilt from the kept widgets — merged `/DR` fonts and `/DA`,
> `/NeedAppearances true` — so fields remain fillable in the output. Names that
> collide across source documents are renamed with a per-source prefix (`d0_`,
> `d1_`, …). Caveat: `/XFA` data is dropped (output is a plain AcroForm), and a
> page selected twice in `assemble` shares one field object (fields linked, not
> renamed).

### (h) Link annotations

Add clickable link annotations to any page — external URIs or internal page jumps.
Works on both loaded and created documents. No visible border is drawn by default.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);
const page = doc.getPage(0);

// External URI link — clickable region at (50, 746), 140 × 18 pt
page.drawLink({ x: 50, y: 746, width: 140, height: 18, url: "https://example.com" });

// Internal page jump — table-of-contents entry pointing to page 2 (0-based)
page.drawLink({ x: 50, y: 700, width: 200, height: 18, goToPage: 2 });

const output = await doc.save();
```

> Exactly one of `url` or `goToPage` must be provided; passing both or neither throws. `goToPage` is 0-based (matches `doc.getPage(i)`). Named destinations and cross-document jumps are not supported.

### (i) Vector paths

Draw SVG path-data strings or polygons. Coordinates are PDF user space (origin
bottom-left, y increases upward). Works on loaded and created documents.

```ts
import { PdfDocument, PageSizes, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);

// SVG path — house icon outline
page.drawSvgPath(
  "M 150 200 L 100 150 L 100 100 L 200 100 L 200 150 Z",
  { fill: rgb(0.2, 0.5, 0.9), stroke: rgb(0.1, 0.3, 0.7), strokeWidth: 1.5 },
);

// Polygon — triangle
page.drawPolygon(
  [{ x: 300, y: 250 }, { x: 240, y: 150 }, { x: 360, y: 150 }],
  { fill: rgb(0.9, 0.6, 0.1), strokeWidth: 1, closed: true },
);

const output = await doc.save();
await Bun.write("paths.pdf", output);
```

> Supported SVG commands: `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`, `S`/`s`,
> `Q`/`q`, `T`/`t`, `Z`/`z`, and `A`/`a` (elliptical arcs, converted to cubic béziers).
> SVG artwork authored for screen (y-down) will appear flipped — negate y or apply a
> transform before passing path data.

### (j) Rotated & translucent text

`drawText` accepts `rotate` (degrees, counter-clockwise about the anchor) and
`opacity` (0–1). Both work on loaded and created documents.

```ts
import { PdfDocument, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);
const font = doc.getFont(StandardFonts.HelveticaBold);

for (let i = 0; i < doc.getPageCount(); i++) {
  doc.getPage(i).drawText("CONFIDENTIAL", {
    x: 150, y: 300, size: 60, font,
    color: rgb(0.8, 0, 0),
    rotate: 45,    // degrees counter-clockwise
    opacity: 0.15, // faint watermark
  });
}

const output = await doc.save();
await Bun.write("report-watermark.pdf", output);
```

### (k) Document outlines / bookmarks

`doc.setOutline(items)` builds the PDF outline/bookmarks tree visible in the
viewer sidebar. Works on loaded and created documents.

```ts
import { PdfDocument, PageSizes } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
for (let i = 0; i < 6; i++) doc.addPage(PageSizes.A4);

doc.setOutline([
  { title: "Introduction", page: 0 },
  {
    title: "Chapter 1", page: 1,
    children: [
      { title: "1.1 Background", page: 1 },
      { title: "1.2 Methods",    page: 2 },
    ],
  },
  { title: "Conclusion", page: 5 },
]);

const output = await doc.save();
await Bun.write("report.pdf", output);
```

`page` is 0-based (matches `doc.getPage(i)`). Children may nest to arbitrary depth.

### (l) Rotate & resize pages

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

// Rotate a loaded page
const doc = await PdfDocument.load(bytes);
doc.getPage(0).setRotation(90);   // clockwise 90° — must be a multiple of 90
const output = await doc.save();
await Bun.write("rotated.pdf", output);
```

`setRotation` accepts any multiple of 90 and normalises it to 0 / 90 / 180 / 270.
Non-multiples of 90 throw `InvalidRotationError`.

```ts
// Resize a created page
const doc2 = await PdfDocument.create();
const page = doc2.addPage([595, 842]);   // A4
page.setSize(612, 792);                  // switch to US Letter
// or equivalently: page.setMediaBox(0, 0, 612, 792)
const output2 = await doc2.save();
```

`setSize(width, height)` is sugar for `setMediaBox(0, 0, width, height)`.
All three methods work on both `doc.getPage(i)` (loaded) and `doc.addPage(...)` (created).

### (m) Document metadata

Set and read the PDF Info dictionary on any document — created or loaded.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();

doc.setTitle("Q2 Report");
doc.setAuthor("Ignacio Garcia P");
doc.setSubject("Quarterly financials");
doc.setKeywords(["finance", "Q2", "2026"]);
doc.setCreator("Acme App");
doc.setCreationDate(new Date());

const output = await doc.save();

// Read back
const loaded = await PdfDocument.load(output);
const meta = await loaded.getMetadata();
console.log(meta.title);        // "Q2 Report"
console.log(meta.keywords);     // ["finance", "Q2", "2026"]
console.log(meta.creationDate); // Date object
```

On a **loaded** PDF, setters write only an incremental update — Info-dict keys you do not
touch are preserved. Dates round-trip to JS `Date`. XMP metadata streams are not modified.

Browser bundlers can import the explicit browser entry, or use the package root
when the bundler honors the `browser` export condition:

```ts
import { PdfDocument } from "@ignaciano3/better-pdf/browser";

const input = new Uint8Array(await file.arrayBuffer());
const doc = await PdfDocument.load(input);
const fields = doc.getForm().getFields();
const output = await doc.save();
```

`PdfDocument.load()` initializes the browser WASM module on first use.

## API

### `PdfDocument`

- `PdfDocument.load(input: Uint8Array | ArrayBuffer, options?: { password?: string }): Promise<PdfDocument>` — open an existing PDF; pass `{ password }` to decrypt an encrypted PDF (use `""` for owner-locked files)
- `PdfDocument.isEncrypted(input: Uint8Array | ArrayBuffer): Promise<boolean>` — report whether a PDF is encrypted, without decrypting or needing a password; use it to decide whether to pass `{ password }` to `load`
- `PdfDocument.passwordType(input: Uint8Array | ArrayBuffer, password: string): Promise<"owner" | "user" | null>` — classify how a password authorizes an encrypted PDF (`"owner"` = full access, `"user"` = restricted); `null` when the password authenticates neither role or the file is not an encrypted classic-`trailer` PDF (xref-stream encrypted files return `null`)
- `PdfDocument.create(): Promise<PdfDocument>` — create a new empty document
- `PdfDocument.merge(docs: Uint8Array[]): Promise<Uint8Array>` — combine multiple PDFs into one (all pages, in order)
- `PdfDocument.assemble(docs: Uint8Array[], selections: {docIndex: number, pageIndex: number}[]): Promise<Uint8Array>` — build a new PDF from an explicit ordered page selection across sources
- `doc.copyPages(indices: number[]): Promise<Uint8Array>` — extract the given pages into a new PDF (load mode only)
- `doc.splitPages(): Promise<Uint8Array[]>` — one single-page PDF per page (load mode only)
- `doc.addPage(size: [number, number]): PdfPage` — add a page; `PageSizes.A4` etc. are `[width, height]` tuples
- `doc.getPageCount(): number`
- `doc.getPages(): PdfPage[]`
- `doc.getPage(index: number): PdfPage` — throws `PageOutOfRangeError` if out of bounds
- `doc.getFont(font: StandardFonts): PdfFont` — standard-14 fonts (sync)
- `doc.embedFont(bytes: Uint8Array, options?: { subset?: boolean }): Promise<PdfFont>` — embed a TTF/OTF font; `subset` defaults to `true`
- `doc.embedJpg(bytes: Uint8Array): Promise<PdfImage>`
- `doc.embedPng(bytes: Uint8Array): Promise<PdfImage>`
- `doc.getForm(): PdfForm`
- `doc.setTitle(s: string): void`
- `doc.setAuthor(s: string): void`
- `doc.setSubject(s: string): void`
- `doc.setKeywords(arr: string[]): void`
- `doc.setCreator(s: string): void`
- `doc.setProducer(s: string): void`
- `doc.setCreationDate(d: Date): void`
- `doc.setModificationDate(d: Date): void`
- `doc.getMetadata(): Promise<DocumentMetadata>` — reads the Info dictionary; all fields are optional
- `doc.setOutline(items: OutlineItem[]): void` — set the PDF bookmarks/outline tree; `OutlineItem = { title: string; page: number; children?: OutlineItem[] }`; `page` is 0-based
- `doc.attach(bytes: Uint8Array, name: string, options?: AttachOptions): void` — queue a file attachment; written to `/EmbeddedFiles` at `save()`. `AttachOptions = { mimeType?, description?, creationDate?, modificationDate?, afRelationship? }`; `afRelationship` is one of `"Source" | "Data" | "Alternative" | "Supplement" | "EncryptedPayload" | "FormData" | "Schema" | "Unspecified"` and additionally writes the catalog `/AF` array. Throws `DuplicateAttachmentError` for a name already queued or already present in the document.
- `doc.getAttachments(): Promise<PdfAttachment[]>` — read every attachment's metadata and bytes from the saved document. `PdfAttachment = { name, description?, mimeType?, creationDate?, modificationDate?, size, afRelationship?, bytes }`.
- `doc.save(options?: SaveOptions): Promise<Uint8Array>` — `SaveOptions = { compress?: boolean }`; `compress` defaults to `true` (deflate generated streams). Pass `{ compress: false }` for plaintext output.

`save()` applies queued fills first, then queued flattens. With no queued operations it returns a byte-identical round trip.
`save()` always starts from the originally loaded bytes (calling it twice
returns the same result), and `FieldInfo.value` reflects queued mutations as
soon as they are made.

**`PageSizes`**: `A3`, `A4`, `A5`, `Letter`, `Legal`, `Tabloid` — each is a `[width, height]` tuple in PDF points.

**`StandardFonts`** (12 standard fonts): `Helvetica`, `HelveticaBold`, `HelveticaOblique`, `HelveticaBoldOblique`, `Courier`, `CourierBold`, `CourierOblique`, `CourierBoldOblique`, `TimesRoman`, `TimesBold`, `TimesItalic`, `TimesBoldItalic`. (`Symbol` and `ZapfDingbats` are intentionally omitted.)

### `PdfPage`

- `page.drawText(text, options)` — `options`: `{ x, y, size, font?, color?, lineHeight?, rotate?, opacity? }` (`rotate` is degrees counter-clockwise; `opacity` 0–1)
- `page.drawImage(image, options)` — `options`: `{ x, y, width?, height?, opacity?, rotate?, xSkew?, ySkew?, flipX?, flipY? }` (`flipX`/`flipY` mirror the image inside the same placement box)
- `page.drawLine(options)` — `options`: `{ start: {x,y}, end: {x,y}, stroke?, strokeWidth?, opacity? }`
- `page.drawRectangle(options)` — `options`: `{ x, y, width, height, fill?, stroke?, strokeWidth?, opacity? }`
- `page.drawEllipse(options)` — `options`: `{ x, y, radiusX, radiusY, fill?, stroke?, strokeWidth?, opacity? }` (`x`,`y` = center; `radiusX`,`radiusY` = radii)
- `page.drawSvgPath(d: string, options): void` — draw an SVG path-data string; `options`: `{ fill?, stroke?, strokeWidth?, opacity? }`; supports M/L/H/V/C/S/Q/T/Z and A/a (arcs)
- `page.drawPolygon(points: {x,y}[], options): void` — draw a polygon; `options`: `{ fill?, stroke?, strokeWidth?, opacity?, closed? }` (`closed` defaults to `true`)
- `page.drawLink(options): void` — add a clickable link annotation; `options` is one of:
  - `{ x, y, width, height, url: string }` — external URI
  - `{ x, y, width, height, goToPage: number }` — internal page jump (0-based); navigates to the top of the target page
  - Border is suppressed by default (invisible clickable region). Exactly one of `url`/`goToPage` must be supplied.
- `page.setRotation(degrees: number): void` — rotate the page; must be a multiple of 90; normalised to 0/90/180/270
- `page.setSize(width: number, height: number): void` — resize the page (sugar for `setMediaBox(0, 0, width, height)`)
- `page.setMediaBox(x0: number, y0: number, x1: number, y1: number): void` — set the PDF `/MediaBox` directly

Available on both loaded pages (`doc.getPage(i)`) and created pages (`doc.addPage(...)`).

### `PdfImage`

- `image.width: number`
- `image.height: number`
- `image.scale(factor: number): { width: number; height: number }`

### `PdfFont`

- `font.widthOfTextAtSize(text: string, size: number): number`

### Color helpers

```ts
import { rgb, grayscale } from "@ignaciano3/better-pdf";
```

- `rgb(r, g, b)` — values 0–1
- `grayscale(v)` — value 0–1

### `PdfForm`

- `form.getFields(): FieldInfo[]`
- `form.getField(name: string): FieldInfo | undefined`
- `form.getTextField(name).setText(value, { font? })` / `.setDefaultText(value, { font? })` — `font` must be an embedded font from `doc.embedFont()`; omit for the default standard-14 rendering
- `form.getCheckBox(name).check()`
- `form.getCheckBox(name).uncheck()`
- `form.getCheckBox(name).setDefaultChecked(checked)`
- `form.getRadioGroup(name).options`
- `form.getRadioGroup(name).select(value)` / `.setDefaultSelected(value)`
- `form.getDropdown(name).options`
- `form.getDropdown(name).select(value)` / `.setDefaultSelected(value)`
- `form.getListBox(name).options`
- `form.getListBox(name).select(value)` / `.setDefaultSelected(value)`
- `form.getSignature(name).setImage(bytes)`
- `field.setReadOnly(bool)` / `.setRequired(bool)` / `.setExported(bool)` — change a loaded field's `/Ff` flags
- `field.hide()` / `.show()` / `.setPrintable(bool)` / `.setNoView(bool)` — change a field's widget `/F` visibility flags
- `form.flattenField(name)` / `form.flatten()`
- `form.resetField(name)` / `form.reset()`

Each `FieldInfo` carries `name`, `type`, `value`, `defaultValue` (the `/DV` reset
value, or `null`), `states`, `options`, `readOnly`, `required`, `exported` (false
when the field has the `NoExport` flag), `maxLength` (a text field's `/MaxLen`, or
`null`), `multiline` (text-area fields), `password` (masked text fields), `comb`
(fixed-pitch per-character text fields), `editable` (combo boxes that accept
custom values), `align` (`"left"`/`"center"`/`"right"`, from `/Q`), `tooltip`
(the `/TU` descriptive name, or `null`), `fontName` / `fontSize` (the effective
`/DA` font resource name and size for variable-text fields, else `null`),
`multiSelect` (multi-select list boxes), and `widgets` — one entry per widget
annotation giving its 0-based `page` index, `rect` (`[x0, y0, x1, y1]` in PDF
points, origin bottom-left), and the annotation visibility flags `hidden` /
`print` / `noView` (from `/F`). `setText()` throws if its value exceeds `maxLength`.

Use `listBox.selectMultiple(values)` for multi-select list boxes (those with
`FieldInfo.multiSelect === true`); `listBox.select(value)` for single-select ones.

The default/reset value (`/DV`, restored by a viewer's "reset form") is
independent of the current value and writable two ways: on new fields via the
builder options `defaultValue` (text), `defaultChecked` (checkbox), and
`defaultSelected` (radio/dropdown/listbox); and on existing fields via the
setters `setDefaultText` / `setDefaultChecked` / `setDefaultSelected`.
`form.resetField(name)` and `form.reset()` restore fields to their default
value (or clear them when there is none) — the equivalent of a viewer's
"reset form".

### Errors

Every error thrown by the library subclasses `PdfError`, so you can catch the
whole family or a specific case:

- `UnknownFieldError` — no field with that name (`.field`).
- `FieldTypeError` — field accessed as the wrong type, e.g. `getDropdown()` on a
  text field (`.field`, `.actual`, `.expected`).
- `InvalidOptionError` — selecting a value that is not one of the field's options
  (`.field`, `.fieldType`, `.value`, `.options`).
- `MaxLengthExceededError` — `setText()` value longer than the field's `/MaxLen`
  (`.field`, `.maxLength`, `.actualLength`).
- `MissingOnStateError` — checking a checkbox with no declared on-state (`.field`).
- `PdfCoreError` — an operation the core rejected at `save()` time (XFA forms,
  unsupported images, malformed PDFs); the core's message is preserved.
- `PageOutOfRangeError` — `getPage(i)` called with an index outside `[0, pageCount)`.
- `InvalidImageError` — `embedJpg`/`embedPng` rejected the image bytes (unsupported format or CMYK JPEG).
- `EncryptedPdfError` — the PDF is encrypted and no password was supplied; pass `{ password }` (or `{ password: "" }` for owner-locked files) to `PdfDocument.load`.
- `IncorrectPasswordError` — the password supplied to `PdfDocument.load` did not decrypt the PDF.
- `MultiSelectError` — `selectMultiple()` was called on a list box that does not have the Multiselect flag set (`.field`).
- `DuplicateAttachmentError` — `doc.attach()` called with a name already queued, or already present in the loaded document (`.attachmentName`).

```ts
import { FieldTypeError } from "@ignaciano3/better-pdf";

try {
  form.getDropdown("some.text.field");
} catch (e) {
  if (e instanceof FieldTypeError) console.log(e.actual, e.expected);
}
```

### Generate Form Types

Generate a TypeScript module from an existing PDF:

```bash
better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
```

Encrypted PDFs need a password — pass `--password s3cret`, or `--password ''`
for owner-locked files that open without a user password.

The generated module exports field-name unions and literal metadata for every
readable property of a field: type, dropdown/listbox options, radio states,
current and default values, the read-only/required/exported/multi-select flags,
text flags (`password`, `multiline`, `comb`, `maxLength`), editable combo boxes,
alignment, tooltip, the `/DA` font name and size, and the page indices the
field's widgets sit on.

Then pass the generated metadata as a type argument to get a fully-narrowed
form — unknown field names, wrong-type access, and invalid option/state values
become compile errors, at zero runtime cost (the schema is referenced only via
`typeof`):

```ts
import { myFormFields } from "./form-types.js";

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```

The untyped `doc.getForm()` keeps working unchanged.

### Signature Images

Visual signatures are appearances only. They do not create cryptographic/PAdES signatures.

Supported image inputs:

- JPEG (grayscale or RGB), embedded directly as `/DCTDecode`. CMYK JPEGs are rejected.
- PNG, for 8-bit non-interlaced grayscale, RGB, grayscale+alpha, or RGBA images.

PNG alpha is currently dropped rather than preserved as a PDF soft mask.

## For AI agents

better-pdf ships an [agent skill](skills/better-pdf/SKILL.md) — procedural
knowledge for driving the library correctly (the load → inspect → generate
types → fill/flatten/sign → save workflow, plus the non-obvious rules: use a
field's *real* export values, never assume `Yes`/`On`; visual signatures are not
cryptographic; `save()` is an incremental update). It installs into 20+ agents
via [skills.sh](https://www.skills.sh).

The strongest agent-readiness feature is the typed workflow above: generate a
types module from the PDF and `doc.getForm<typeof myFormFields>()` turns
hallucinated field names and invalid values into compile errors.

## Benchmarks

`better-pdf` is consistently faster than `pdf-lib` on end-to-end mutation
workloads, thanks to its Rust/WebAssembly core and append-only incremental
saves. Indicative results from `bun run bench` on the bundled fixture corpus
(50 iterations after warmup):

The **fill** and **flatten** rows are the like-for-like comparison. The
*load + save unchanged* rows compare better-pdf's no-op incremental round-trip
(it returns the original bytes) against pdf-lib's full parse + re-serialize —
they showcase the architectural difference, not parser speed.

### Small mixed form

`Form.-D.P.-2.4.1-Ficha-personal.pdf` — 57 KB, 30 fields: text, radio, dropdown.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.02 ms | 1.29 ms | 58.4× |
| load + read fields | 0.48 ms | 0.79 ms | 1.7× |
| fill 24 text fields + save | 1.10 ms | 5.86 ms | 5.3× |
| fill 2 choice fields + save | 0.80 ms | 4.57 ms | 5.7× |
| flatten all + save | 0.89 ms | 4.83 ms | 5.5× |

### Medium dense form

`Modulo-de-Diabetes.pdf` — 259 KB, 109 fields: text, radio, checkbox, dropdown, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.07 ms | 13.89 ms | 186.4× |
| load + read fields | 1.70 ms | 5.57 ms | 3.3× |
| fill 24 text fields + save | 3.87 ms | 26.43 ms | 6.8× |
| fill 19 choice fields + save | 3.77 ms | 27.03 ms | 7.2× |
| stamp 2 signature images + save | 8.31 ms | n/a | n/a |
| stamp first signature + flatten it | 7.49 ms | n/a | n/a |
| flatten all + save | 4.68 ms | error | n/a |

### Large signature form

`Convenio-OSFATUN-Discapacidad-2022.pdf` — 735 KB, 22 fields: text, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.24 ms | 1.33 ms | 5.5× |
| load + read fields | 0.36 ms | 0.78 ms | 2.1× |
| fill 20 text fields + save | 1.02 ms | 4.36 ms | 4.3× |
| stamp 2 signature images + save | 5.94 ms | n/a | n/a |
| stamp first signature + flatten it | 3.81 ms | n/a | n/a |
| flatten all + save | 0.96 ms | error | n/a |

### PDF generation

Building or stamping documents from scratch (no fixture). The `create + draw`
rows compare against `pdf-lib`'s equivalent generation API; vector shapes have
no direct `pdf-lib` one-liner equivalent.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| create + draw text | 0.15 ms | 1.25 ms | 8.2× |
| stamp text on existing | 1.10 ms | 2.16 ms | 2.0× |
| create + draw image | 0.07 ms | 0.50 ms | 7.3× |
| create + vector shapes | 0.09 ms | n/a | n/a |

In the two `error` rows, `pdf-lib` threw `Unexpected N type: undefined` while
flattening real-world fixtures. Absolute timings vary by machine; reproduce
them on yours with `bun run bench` (set `BENCH_ITER` to change the iteration
count).

## Limitations

Gaps better-pdf does not cover **yet** — things we intend to close. (For features
we will deliberately never add, see [Non-Goals](#non-goals).)

> **Behavioral change in 1.11.0:** `page.drawText()` and embedded-font form
> fill (`setText(value, { font })`) now **throw `MissingGlyphError`** when the
> font has no glyph for a character, instead of silently dropping it — a
> silently-skipped glyph was data loss dressed up as success. Pass
> `page.drawText(text, { font, onMissingGlyph: "skip" })` to restore the old
> silent-skip behavior for drawing (there is no skip opt-out for form fill).

- **Encrypted PDF decryption** (RC4, AES-128, AES-256) is supported via `PdfDocument.load(bytes, { password })` (use `{ password: "" }` for owner-locked / empty-user-password files). Decryption is opt-in — bare `load(bytes)` is unchanged; an encrypted file loaded without a password throws `EncryptedPdfError` (nudging you to pass one), and a wrong password throws `IncorrectPasswordError`. Saving an edited encrypted PDF produces a **decrypted** output. **Still unsupported:** producing encrypted output (re-encryption) and encrypting documents you create.
- No cryptographic signing (the API leaves room to add PAdES later).
- Appearance-affecting form-field flags (`multiline`, `comb`, `password`) are set
  at field creation only — they cannot be toggled on a loaded field (the
  ReadOnly / Required / NoExport and widget Hidden / Print / NoView flags can).
- Form text fields with the Multiline flag are filled with wrapped, top-aligned
  multi-line appearances (honoring `\n` hard breaks and per-line quadding);
  single-line fields are filled single-line. Mid-word breaking is not performed
  — a word wider than the field overflows onto its own line.
- Drawing APIs support standard-14 fonts and custom TTF/OTF font embedding via
  `doc.embedFont(bytes)` (Type0/CIDFontType2, full Unicode including CJK).
  OpenType-CFF subsetting may be unsupported — use `{ subset: false }` for CFF-outline `.otf` fonts.
  Characters with no glyph in the font throw `MissingGlyphError` (see
  "Behavioral changes in 1.11.0" below).
- Appearance metrics cover the standard 14 text fonts (with Arial / Times New
  Roman / Courier New aliases and subset-prefix handling) and any simple font
  carrying a `/Widths` array; unrecognized fonts fall back to Helvetica metrics.
- **Form-field text appearance:** field values render in a standard-14 font by
  default — selectable per field via the builder `font` option (Helvetica /
  Times / Courier families), with `fontSize`, `textColor`, and `align` also
  configurable. `field.setText(value, { font })` / `.setDefaultText(value, {
  font })` additionally accept an embedded font (`doc.embedFont(bytes)`) for
  Unicode/CJK values, on plain and multiline text fields of any origin
  (loaded or builder-created). **Still standard-14 only:** comb, dropdown, and
  listbox fields reject an embedded font.
- Color: RGB and grayscale only; CMYK is not supported.
- XMP metadata streams are not written or modified (only the Info dictionary is
  updated via `doc.setTitle()` / `doc.getMetadata()` etc.).
- ZUGFeRD/Factur-X **structure** is supported (`/AF`, `/AFRelationship`); PDF/A-3
  conformance metadata (XMP) is not written — that part is your responsibility.
- Nested page trees are not supported by `insertPage`/`removePage`/`movePage`
  (PDFs with nested `/Pages` nodes are rejected; use `merge`/`assemble` instead).
- SVG path coordinates are PDF user space (y-up), so y-down artwork appears flipped.
- Interlaced and 16-bit-per-channel PNGs are not supported.
- Malformed / off-spec PDFs are repaired on load (added in 1.13.0): when strict
  parsing fails (broken/missing xref or trailer, junk before the `%PDF` header,
  invalid `/Root`, missing `endobj`/`endstream`), a recovery pass rescans the
  bytes and rebuilds the document. Well-formed files never pay for it, and
  encrypted files still throw `EncryptedPdfError` rather than being "repaired"
  into plaintext.
- Primary test coverage is the bundled fixture corpus (classic-xref PDF 1.3 forms,
  plus generated xref-stream/object-stream variants) and the malformed/tricky-PDF
  corpus ported from pdf-lib's test suite.
- Browser/bundler support requires calling `initializeWasm(wasmUrl)` before any
  PDF operation. Pass the URL of `@ignaciano3/better-pdf/wasm` (use `?url` in
  Vite, `new URL(…, import.meta.url)` in webpack, or copy to `public/` in
  Next.js). Node, Bun, and Deno self-initialize — no call needed. See the
  [per-runtime guide](docs/site/src/content/docs/guides/runtimes.md) and
  [examples/runtimes/](examples/runtimes/).

## Non-Goals

Deliberately unsupported. **Not planned** — legacy, rare, or better served by
another tool.

- **XFA forms** — Adobe's XML-based form format, deprecated and removed in
  PDF 2.0. Detected and rejected on fill/flatten; reading the static AcroForm
  fields still works.

## Develop

Prerequisites: `bun`, the Rust toolchain, the `wasm32-unknown-unknown` target, and `wasm-pack` (it downloads and runs its own `wasm-opt`, so no system Binaryen is needed).

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
bun install
bun run build      # compile Rust core to pkg-web/ and TypeScript API to dist/
bun test           # run TS API tests
bun run test:browser-entry
bun run test:browser  # load the web build in headless Chromium (needs `bunx playwright install chromium`)
bun run typecheck  # run TypeScript checks
npm pack --dry-run # inspect package contents
```

Rust checks:

```bash
cargo test --manifest-path crates/core/Cargo.toml
cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings
```

Manual playground:

```bash
bun run play
bun run play tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf signature.png
```

Benchmarks against `pdf-lib`:

```bash
bun run bench
```

API reference (TypeDoc → `docs/api`):

```bash
bun run docs
```

### Releasing

Publishing is automated by `.github/workflows/release.yml`: push a `vX.Y.Z` tag
that matches `package.json`'s `version` and it builds and runs
`npm publish --provenance`. It needs an `NPM_TOKEN` repo secret (an npm
automation token). Provenance is attached via OIDC, so publishing only works from
CI — a local `npm publish` fails because `publishConfig.provenance` is `true`.

```bash
npm version patch   # or minor / major — bumps package.json + creates the tag
git push --follow-tags
```
