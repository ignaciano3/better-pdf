---
title: Migrating from pdf-lib
description: API and semantic mapping from pdf-lib to better-pdf.
---

`@ignaciano3/better-pdf` covers pdf-lib's form-filling workflow with a faster
core and stricter validation. This guide maps the APIs.

## API mapping

| pdf-lib | better-pdf |
| --- | --- |
| `PDFDocument.load(bytes)` | `PdfDocument.load(bytes)` |
| `pdfDoc.getForm()` | `doc.getForm()` |
| `form.getFields()` | `form.getFields()` → plain `FieldInfo[]` (name/type/value/states/options/…) |
| `form.getTextField(n).setText(v)` | `form.getTextField(n).setText(v)` |
| `form.getCheckBox(n).check()` / `.uncheck()` | same — uses the field's real on-state automatically |
| `form.getRadioGroup(n).select(v)` | same — `v` must be a real export value (`field.states`) |
| `form.getDropdown(n).select(v)` | same — `v` must be a real option (`field.options`) |
| `form.getOptionList(n).select(v)` | `form.getListBox(n).select(v)` (single-select) |
| `field.acroField.getWidgets()` | `field` info's `widgets` (`{page, rect}` per widget) |
| `form.flatten()` | `form.flatten()` (or `form.flattenField(name)`) |
| `pdfDoc.save()` | `doc.save()` — **incremental, append-only** |
| `form.updateFieldAppearances()` | not needed — appearances are generated on fill |

## Semantic differences

- **Saves are incremental.** Output begins with the original bytes verbatim and
  appends an update section. `save()` always starts from the loaded bytes, so
  calling it twice yields the same result.
- **Validation is strict and typed.** Unknown fields, wrong-type access, invalid
  options/states, over-`maxLength` text, and core rejections (XFA, CMYK JPEGs)
  throw `PdfError` subclasses instead of writing garbage.
- **Appearances are always generated** (pdf-lib often leaves `/NeedAppearances`
  rendering to the viewer); flattening therefore works on PDFs where pdf-lib
  throws `Unexpected N type: undefined`.
- **Signatures are visual only** — image appearances, not cryptographic signing.
- **Scope:** creation, page drawing, and form filling are all covered.
  Encryption is not supported.

## Generating documents

better-pdf covers pdf-lib's document generation API. The method names are largely
identical; the differences are noted below.

### API mapping

| pdf-lib | better-pdf |
| --- | --- |
| `PDFDocument.create()` | `await PdfDocument.create()` (async — returns `Promise<PdfDocument>`) |
| `pdfDoc.addPage([width, height])` | `doc.addPage([width, height])` or `doc.addPage(PageSizes.A4)` etc. |
| `pdfDoc.getPageCount()` | `doc.getPageCount()` |
| `pdfDoc.getPages()` | `doc.getPages()` |
| `pdfDoc.getPage(i)` | `doc.getPage(i)` — throws `PageOutOfRangeError` instead of returning `undefined` |
| `pdfDoc.embedJpg(bytes)` | `doc.embedJpg(bytes)` (async) |
| `pdfDoc.embedPng(bytes)` | `doc.embedPng(bytes)` (async) |
| `pdfDoc.embedFont(StandardFonts.Helvetica)` | `doc.getFont(StandardFonts.Helvetica)` (sync) |
| `pdfDoc.embedFont(fontBytes, { subset: true })` | `await doc.embedFont(fontBytes, { subset: true })` (async; same `{ subset }` option) |
| `font.widthOfTextAtSize(text, size)` | same |
| `page.drawText(text, options)` | same |
| `page.drawImage(img, options)` | same |
| `page.drawRectangle(options)` | same |
| `page.drawLine(options)` | same |
| `page.drawEllipse(options)` | same — see note below |
| `page.setRotation(degrees({angle}))` | `page.setRotation(degrees)` — pass a plain `number` (multiple of 90); no `degrees()` wrapper needed |
| `page.setSize(width, height)` | `page.setSize(width, height)` — same signature |
| `page.setMediaBox(x0, y0, x1, y1)` | `page.setMediaBox(x0, y0, x1, y1)` — same signature |
| `rgb(r, g, b)` / `grayscale(v)` | same |
| `StandardFonts.Helvetica` etc. | same enum values |
| `pdfDoc.save()` | `doc.save()` — returns `Promise<Uint8Array>` |

### Differences from pdf-lib

- **Custom font embedding has parity with pdf-lib.** `doc.embedFont(bytes, { subset? })`
  accepts TTF/OTF bytes and the same `{ subset: boolean }` option as pdf-lib (default
  `true`). The result is a `PdfFont` you pass to `drawText`. The embedded font is a
  Unicode-capable Type0/CIDFontType2 composite with a ToUnicode CMap, so CJK, accented
  Latin, and full Unicode text is selectable and searchable. `widthOfTextAtSize` works
  for embedded fonts. `getFont()` (sync, no bytes) remains available for standard-14
  fonts.
- **Form creation uses a builder.** pdf-lib mutates `form` in place via
  `form.createTextField(...)`; better-pdf accumulates fields through a chainable
  `doc.createForm()` builder (see below). `getForm()` itself is not available on
  a created document until it is saved and reloaded.
- **RGB and grayscale only.** CMYK color is not supported.
- **Ellipse center semantics.** `drawEllipse({ x, y, xScale, yScale, … })` uses
  `(x, y)` as the center and `xScale`/`yScale` as the x and y radii — the same
  as pdf-lib.
- **`setRotation` takes a plain number.** pdf-lib wraps the angle in a `degrees(n)`
  object from `pdf-lib`; better-pdf takes a plain `number` (e.g. `page.setRotation(90)`).
  The value must be a multiple of 90; non-multiples throw `InvalidRotationError`.
- **`save()` is always async** and returns `Promise<Uint8Array>`. There is no
  synchronous `saveSync()`.
- **`PdfDocument.create()` is async** — it may initialize WASM, so you must
  `await PdfDocument.create()` (pdf-lib's `PDFDocument.create()` is synchronous).
- **`PageSizes` constants** are `[width, height]` tuples in PDF points: `A3`,
  `A4`, `A5`, `Letter`, `Legal`, `Tabloid`.

## Creating form fields

pdf-lib creates AcroForm fields by mutating the form returned from
`pdfDoc.getForm()`. better-pdf uses a chainable builder obtained from
`doc.createForm()` on a **created** document (`PdfDocument.create()`); each
`add*` call also refines the builder's type so `getFieldNames()` is statically
typed to the declared names.

### API mapping

| pdf-lib | better-pdf |
| --- | --- |
| `form.createTextField(name)` + `field.addToPage(page, opts)` | `doc.createForm().addTextField(name, { page, x, y, width, height, … })` |
| `form.createCheckBox(name)` | `.addCheckBox(name, { page, x, y, size, … })` |
| `form.createRadioGroup(name)` | `.addRadioGroup(name, { options: [{ value, page, x, y, size }], … })` |
| `form.createDropdown(name)` | `.addDropdown(name, { page, x, y, width, height, options, … })` |
| `form.createOptionList(name)` | `.addListBox(name, { page, x, y, width, height, options, … })` |
| (no signature-field creation) | `.addSignatureField(name, { page, x, y, width, height, … })` |

### Differences from pdf-lib

- **Created documents only.** `doc.createForm()` throws if the document was
  opened with `PdfDocument.load()`; pdf-lib lets you add fields to any document.
- **Chainable, not in-place.** Every `add*` returns the builder, so fields are
  declared in one fluent chain rather than mutating a shared `form` object.
- **Position is per-call.** Geometry (`page`, `x`, `y`, plus `width`/`height` or
  `size`) is passed to each `add*` call — there is no separate `addToPage` step.
- **Typed names accumulate.** `getFieldNames()` is typed to the declared field
  names. Once saved and reloaded with `PdfDocument.load`, the form is a normal
  fillable AcroForm — fill it via `getForm()` and flatten it with this library.

## Document metadata

better-pdf has full parity with pdf-lib's metadata setters and adds a unified async getter.

### API mapping

| pdf-lib | better-pdf |
| --- | --- |
| `pdfDoc.setTitle(s)` | `doc.setTitle(s)` |
| `pdfDoc.setAuthor(s)` | `doc.setAuthor(s)` |
| `pdfDoc.setSubject(s)` | `doc.setSubject(s)` |
| `pdfDoc.setKeywords(arr)` | `doc.setKeywords(arr)` — `string[]` |
| `pdfDoc.setCreator(s)` | `doc.setCreator(s)` |
| `pdfDoc.setProducer(s)` | `doc.setProducer(s)` |
| `pdfDoc.setCreationDate(d)` | `doc.setCreationDate(d)` |
| `pdfDoc.setModificationDate(d)` | `doc.setModificationDate(d)` |
| `pdfDoc.getTitle()` / `getAuthor()` / … | `await doc.getMetadata()` → `DocumentMetadata` |

### Differences from pdf-lib

- **Unified getter.** pdf-lib exposes individual `getTitle()` / `getAuthor()` / … getters.
  better-pdf returns everything as a single `DocumentMetadata` object from `await doc.getMetadata()`.
- **Works on loaded PDFs (incremental).** Setting metadata on a loaded PDF writes only an
  incremental update — unmodified Info-dict keys from the original document are preserved.
- **Dates round-trip.** `setCreationDate` / `setModificationDate` accept a JS `Date`; reading
  back with `getMetadata()` returns `Date` objects (PDF date syntax is handled by the core).
- **XMP metadata streams are not written** — only the PDF Info dictionary is updated. This
  matches pdf-lib's default behavior.

## Merging and copying pages

pdf-lib merges documents by creating a new `PDFDocument`, then calling
`copyPages` on a source doc and adding each copied page with `addPage`. The
resulting document must be saved separately.

better-pdf provides two static helpers and two instance methods that cover the
same patterns with a single `await`:

| pdf-lib | better-pdf |
| --- | --- |
| `const dest = await PDFDocument.create();` + `const pages = await dest.copyPages(src, indices);` + `pages.forEach(p => dest.addPage(p));` + `await dest.save()` | `await doc.copyPages(indices)` — extracts those pages into a new PDF |
| Loop over multiple sources + `copyPages` + `addPage` | `await PdfDocument.merge([a, b, c])` — all pages in order |
| Manual per-page `copyPages` / `addPage` across sources | `await PdfDocument.assemble(docs, selections)` — full reorder/cross-doc control |
| (no equivalent) | `await doc.splitPages()` — one single-page PDF per page |

### Example — merge

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const merged = await PdfDocument.merge([bytesA, bytesB, bytesC]);
```

### Example — extract / reorder pages

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);
const reordered = await doc.copyPages([2, 0, 1]);   // page 2 first, then 0, then 1
```

### Notes

- `copyPages` and `splitPages` are available on **loaded** documents only
  (documents opened with `PdfDocument.load`).
- Form fields on assembled/merged pages keep their visual appearance but are
  **not interactive** — no AcroForm is reconstructed. Flatten fields before
  merging if you need a flat result.

## Embedding pages from other PDFs

pdf-lib lets you import pages with `embedPdf` / `embedPage` and stamp them with
`page.drawPage`. better-pdf has the same workflow under the same method names.

### API mapping

| pdf-lib | better-pdf |
| --- | --- |
| `await pdfDoc.embedPdf(srcBytes)` → `EmbeddedPdfPage[]` | `await doc.embedPdfPage(srcBytes, pageIndex)` → `EmbeddedPdfPage` (one page per call) |
| `page.drawPage(embedded, { x, y, width, height })` | `page.drawPage(embedded, { x, y, width?, height? })` — same; `width`/`height` default to intrinsic source size |

### Example — watermark overlay

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const docBytes        = new Uint8Array(await Bun.file("report.pdf").arrayBuffer());
const watermarkBytes  = new Uint8Array(await Bun.file("watermark.pdf").arrayBuffer());

const doc = await PdfDocument.load(docBytes);
const stamp = await doc.embedPdfPage(watermarkBytes, 0);

for (let i = 0; i < doc.getPageCount(); i++) {
  doc.getPage(i).drawPage(stamp, { x: 0, y: 0 });
}

await Bun.write("output.pdf", await doc.save());
```

### Differences from pdf-lib

- **One page per call.** pdf-lib's `embedPdf(bytes)` returns all pages at once
  as an array; better-pdf's `embedPdfPage(bytes, pageIndex)` imports a single
  page. Call it once per page you need.
- **Interactive content flattened.** AcroForm fields, annotations, and links on
  the embedded page are **not carried over** — only static visual appearance.
  Flatten fields in the source PDF before embedding if needed.
- **Works on created documents.** pdf-lib also supports this; better-pdf matches
  the behavior — `embedPdfPage` and `drawPage` work on both `PdfDocument.load`
  and `PdfDocument.create` documents.

## Typed forms (no pdf-lib equivalent)

Generate a schema module once, then field names, types, and option values are
compile-checked:

```bash
npx better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
```

```ts
const form = doc.getForm<typeof enrollmentFormFields>();
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```
