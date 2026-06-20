---
name: better-pdf
description: Fill and flatten PDF AcroForm fields (text, checkbox, radio, dropdown, visual signature) in existing PDFs with the @ignaciano3/better-pdf npm package, generate TypeScript types from a PDF form for compile-time-safe filling, create new PDFs from scratch, draw text with custom TTF/OTF fonts (full Unicode including CJK), draw rotated or translucent text with drawText({rotate, opacity}), add document outlines/bookmarks with doc.setOutline(), draw images (including transparent PNGs and palette/indexed PNGs with alpha preserved as a soft mask) and vector graphics, draw SVG path-data strings with page.drawSvgPath() or polygons with page.drawPolygon(), add clickable link annotations (external URLs and internal page jumps) with page.drawLink(), read/write PDF document metadata (title/author/keywords/dates; non-ASCII/Unicode text supported via UTF-16BE), merge multiple PDFs, extract/copy/reorder pages, split PDFs into single-page files, add/insert/remove/move pages in existing PDFs, rotate or resize individual pages, embed pages from other PDFs as Form XObjects (watermarks, letterhead, N-up). Use when filling or flattening PDF forms, reading AcroForm fields, embedding a visual signature image, creating PDF documents, drawing Unicode text, drawing rotated or translucent text, adding PDF bookmarks or a table-of-contents outline, embedding transparent PNG images, drawing vector paths or polygons, adding hyperlinks or internal navigation links to a PDF, reading or setting PDF metadata, merging PDFs, extracting or reordering pages, inserting or removing pages in a loaded PDF, rotating or resizing pages, stamping a page from another PDF, or when the user mentions better-pdf, pdf-lib, or AcroFields.
---

# better-pdf

A maintained, fast alternative to pdf-lib for **filling and flattening AcroForm fields in existing PDFs**. Rust→WASM core + a fully-typed TS API; runs in Node/Bun/Deno and the browser. Zero runtime npm deps.

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

1. **Use the field's REAL export values — never assume `"Yes"`/`"On"`.** Corpus values are domain-specific (`F`/`M`, `SI`/`NO`, `Titular`/`Familiar`). Read them from `field.states` (checkbox/radio) or `field.options` (dropdown). `checkBox.check()` uses the field's actual on-state automatically; `uncheck()` sets `Off`.
2. **Existing PDFs only.** No creation from scratch / arbitrary drawing. Load → fill/flatten → save.
3. **Signatures are visual only** — an embedded image/appearance, NOT cryptographic/PAdES signing. `getSignature(name).setImage(jpegOrPngBytes)`.
4. **`save()` is an incremental (append-only) update** — output begins with the original bytes verbatim. With nothing queued it returns a byte-identical round-trip. `save()` always starts from the loaded bytes; `FieldInfo.value` reflects queued mutations immediately.
5. **Wrong-type access throws** (e.g. `getDropdown()` on a text field), and invalid options/states throw before save. Errors subclass `PdfError`: `UnknownFieldError`, `FieldTypeError`, `InvalidOptionError`, `MaxLengthExceededError`, `MissingOnStateError`, `PdfCoreError`; core rejections at save time (XFA forms, CMYK JPEGs, malformed PDFs) throw `PdfCoreError`.

## Typed filling (recommended workflow)

Generate a types module from the PDF, then pass it to `getForm` for compile-time safety — unknown field names, wrong-type access, and invalid option/state values become **compile errors**, at zero runtime cost:

```bash
npx better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
```

```ts
import { myFormFields } from "./form-types.js";          // generated `…Fields` const

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```

The pure generator is also importable: `import { generateFormTypes } from "@ignaciano3/better-pdf/typegen"` (WASM-free, tree-shakeable).

## Flattening

```ts
form.flattenField("beneficiario.apellidos_nombres"); // one field → page graphics
form.flatten();                                       // all fields
await doc.save();
```

Flattened fields are stamped onto the page and removed from the AcroForm (no longer editable).

## Embedded fonts (Unicode / CJK)

Embed any TTF or OTF font to render Unicode text. The embedded font is a Type0/CIDFontType2
composite PDF font with a ToUnicode CMap — text is selectable and searchable.

```ts
import { PdfDocument, PageSizes } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);

const fontBytes = new Uint8Array(await Bun.file("NotoSansCJK-Regular.ttf").arrayBuffer());
// subset: true (default) — keeps only glyphs used in the document
const font = await doc.embedFont(fontBytes, { subset: true });

const text = "日本語テキスト — Héllo Wörld";
const w = font.widthOfTextAtSize(text, 18);
page.drawText(text, { x: (595 - w) / 2, y: 700, size: 18, font });

await doc.save();
```

- `embedFont` works on both created and loaded documents.
- `widthOfTextAtSize` works on embedded fonts.
- Characters with no glyph in the font are silently skipped.
- OpenType-CFF (`.otf` with CFF outlines) may fail to subset — use `{ subset: false }` for those.
- Standard-14 fonts (Helvetica, etc.) remain the default when no `font` is passed to `drawText`.

## API reference

| Call | Purpose |
|------|---------|
| `PdfDocument.load(bytes)` → `Promise<PdfDocument>` | Load an existing PDF |
| `PdfDocument.merge(docs)` → `Promise<Uint8Array>` | Merge multiple PDFs into one (all pages, in order) |
| `PdfDocument.assemble(docs, selections)` → `Promise<Uint8Array>` | Build a new PDF from an explicit ordered page selection across sources |
| `doc.copyPages(indices)` → `Promise<Uint8Array>` | Extract the given pages into a new PDF (load mode only) |
| `doc.splitPages()` → `Promise<Uint8Array[]>` | One single-page PDF per page (load mode only) |
| `doc.save()` → `Promise<Uint8Array>` | Apply queued fills+flattens (incremental) |
| `doc.setTitle(s)` / `setAuthor(s)` / `setSubject(s)` | Set Info-dict string fields |
| `doc.setKeywords(arr)` | Set /Keywords from a `string[]` |
| `doc.setCreator(s)` / `setProducer(s)` | Set /Creator and /Producer |
| `doc.setCreationDate(d)` / `setModificationDate(d)` | Set dates from JS `Date` |
| `await doc.getMetadata()` → `DocumentMetadata` | Read the Info dictionary |
| `doc.addPage(size?)` → `PdfPage` | Append a blank page — works on loaded and created docs; immediately drawable |
| `doc.insertPage(index, size?)` | Insert a blank page at 0-based index in a loaded doc; reflected after save + reload |
| `doc.removePage(index)` | Remove page at 0-based index from a loaded doc; reflected after save + reload |
| `doc.movePage(from, to)` | Move a page to a new 0-based index in a loaded doc; reflected after save + reload |
| `page.setRotation(degrees)` | Rotate page (multiple of 90; normalised) — loaded or created |
| `page.setSize(width, height)` | Resize page (sugar for setMediaBox(0,0,w,h)) — loaded or created |
| `page.setMediaBox(x0, y0, x1, y1)` | Set PDF /MediaBox directly — loaded or created |
| `doc.embedPdfPage(src, pageIndex)` → `Promise<EmbeddedPdfPage>` | Import a page from another PDF as a Form XObject (loaded or created doc) |
| `page.drawPage(embedded, {x, y, width?, height?})` | Stamp the Form XObject; width/height default to intrinsic source size |
| `page.drawSvgPath(d, { fill?, stroke?, strokeWidth?, opacity? })` | Draw an SVG path-data string; supports M/L/H/V/C/S/Q/T/Z (arcs A/a not supported); coordinates are PDF user space (y-up) |
| `page.drawPolygon(points, { fill?, stroke?, strokeWidth?, opacity?, closed? })` | Draw a polygon from `{x,y}[]`; `closed` defaults to `true` |
| `page.drawText(text, { …, rotate?, opacity?, maxWidth? })` | `rotate`: degrees counter-clockwise about anchor (free angle); `opacity`: 0–1; `maxWidth` (number, optional): word-wrap text to this width in points. `\n` are kept as hard breaks. Works with standard-14 and embedded fonts. — on loaded and created PDFs |
| `doc.setOutline(items)` | Build the PDF bookmarks/outline tree; `items: { title, page, children? }[]`; `page` is 0-based — on loaded and created PDFs |
| `page.drawLink({ x, y, width, height, url })` | Add a clickable external-URI link annotation (invisible border by default) |
| `page.drawLink({ x, y, width, height, goToPage })` | Add a clickable internal page-jump annotation; `goToPage` is 0-based |
| `doc.embedFont(bytes, { subset? })` → `Promise<PdfFont>` | Embed TTF/OTF; returns a `PdfFont` for `drawText` |
| `doc.getForm()` / `doc.getForm<typeof schema>()` | Untyped / type-narrowed form view |
| `form.getFields()` / `form.getField(name)` | `FieldInfo[]` / one `FieldInfo` |
| `form.getTextField(name).setText(v)` | Set text |
| `form.getCheckBox(name).check()` / `.uncheck()` | Toggle using real on-state |
| `form.getRadioGroup(name).select(v)` | Select by real export value |
| `form.getDropdown(name).select(v)` | Select by real option value |
| `form.getListBox(name).select(v)` | Select list-box option |
| `form.getListBox(name).selectMultiple(values)` | Select multiple options (multi-select list boxes only; throws `MultiSelectError` otherwise) |
| `form.getSignature(name).setImage(bytes)` | Embed visual signature (JPEG/PNG) |
| `form.flattenField(name)` / `form.flatten()` | Flatten one / all fields |
| `generateFormTypes(fields, { typeName })` | Emit a typed `…Fields` module (string) |

`FieldInfo = { name, type, value, states, options, readOnly, required, exported, maxLength, multiSelect, widgets }`, where `exported` is false only when the `NoExport` flag is set, `maxLength` is a text field's `/MaxLen` (or null), `multiSelect` is true only for multi-select list boxes (the PDF Multiselect choice flag), and `widgets: { page, rect: [x0,y0,x1,y1] }[]` gives each widget's 0-based page index and `/Rect` in PDF points (origin bottom-left). `setText` throws if longer than `maxLength`. `type` ∈ `text | checkbox | radio | dropdown | listbox | signature | pushbutton | unknown`.

## Rotated & translucent text

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

await Bun.write("watermark.pdf", await doc.save());
```

## Document outlines / bookmarks

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

await Bun.write("report.pdf", await doc.save());
```

`page` is 0-based (matches `doc.getPage(i)`). Children nest to arbitrary depth.
Works on loaded and created documents.

## Page operations (merge, extract, split, assemble)

Combine, rearrange, or split PDFs. All methods return a new `Uint8Array`; source
documents are not mutated.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

// Merge — combine all pages from multiple PDFs in order
const merged = await PdfDocument.merge([bytesA, bytesB, bytesC]);

// Extract / copy pages — load mode only
const doc = await PdfDocument.load(bytes);
const extracted = await doc.copyPages([0, 2, 4]);   // 0-based page indices

// Split — one single-page PDF per page
const pages = await doc.splitPages();   // Promise<Uint8Array[]>

// Assemble — full cross-doc reorder/selection control
const result = await PdfDocument.assemble(
  [cover, body, annex],
  [
    { docIndex: 0, pageIndex: 0 },
    { docIndex: 1, pageIndex: 2 },
    { docIndex: 2, pageIndex: 0 },
  ],
);
```

**Rules:**
- `copyPages` and `splitPages` require a loaded document (`PdfDocument.load`); they throw on created docs.
- Form fields on merged/assembled pages stay **interactive** (0.15.0): a working `/AcroForm` is rebuilt with the kept fields, merged `/DR` fonts, and `/NeedAppearances true`. Names that collide across sources are renamed with a per-source prefix (`d0_`, `d1_`, …). `/XFA` data is dropped; a page selected twice shares its field objects.
- In-place page rotation/resize is **supported**: `page.setRotation(degrees)`, `page.setSize(w, h)`, `page.setMediaBox(x0, y0, x1, y1)` — works on loaded and created pages.
- **Page insertion/removal/move on loaded docs are supported** (added in 0.13.0): `doc.addPage(size?)` appends a blank drawable page; `doc.insertPage(index, size?)`, `doc.removePage(index)`, `doc.movePage(from, to)` are reflected after save + reload. Nested page trees are not supported (rare).
- **Transparent PNGs are supported**: `embedPng` preserves the alpha channel of RGBA, gray+alpha, and palette/indexed-color (color type 3 with `tRNS`) PNGs as a soft mask (`/SMask`). No API change — just pass the PNG bytes.
- **Non-ASCII metadata is supported**: `setTitle`/`setAuthor`/etc. encode non-Latin text as UTF-16BE for correct round-trip fidelity (added in 0.13.0).

## Embed pages from other PDFs

Stamp a page from another PDF as a Form XObject — watermarks, letterhead,
N-up layouts, or overlays.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const docBytes       = new Uint8Array(await Bun.file("report.pdf").arrayBuffer());
const letterheadBytes = new Uint8Array(await Bun.file("letterhead.pdf").arrayBuffer());

const doc = await PdfDocument.load(docBytes);
const stamp = await doc.embedPdfPage(letterheadBytes, 0);  // page 0 of letterhead

for (let i = 0; i < doc.getPageCount(); i++) {
  doc.getPage(i).drawPage(stamp, { x: 0, y: 0 });   // full intrinsic size
}

const output = await doc.save();
await Bun.write("report-letterhead.pdf", output);
```

- `embedPdfPage(src, pageIndex)` — works on loaded and created documents.
- `drawPage(embedded, { x, y, width?, height? })` — `width`/`height` default to
  the source page's intrinsic MediaBox dimensions; pass explicit values to scale.
- Interactive form fields and annotations on the embedded page are **not carried
  over** — static visual appearance only.

## Vector paths

Draw SVG path-data strings or polygons onto any page. Both work on loaded and
created documents. Coordinates are **PDF user space** (origin bottom-left, y-up).

```ts
import { PdfDocument, PageSizes, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);

// SVG path — simple triangle
page.drawSvgPath("M 150 250 L 80 120 L 220 120 Z", {
  fill: rgb(0.2, 0.5, 0.9),
  stroke: rgb(0.1, 0.3, 0.7),
  strokeWidth: 1.5,
  opacity: 0.9,
});

// Polygon from point array
page.drawPolygon(
  [{ x: 300, y: 250 }, { x: 250, y: 150 }, { x: 350, y: 150 }],
  { fill: rgb(0.9, 0.6, 0.1), strokeWidth: 1 },
);

const output = await doc.save();
```

- Supported SVG commands: `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`, `S`/`s`,
  `Q`/`q`, `T`/`t`, `Z`/`z` (absolute and relative variants).
- **SVG arc commands (`A`/`a`) are not yet supported** — they throw at call time.
- SVG artwork authored y-down will appear flipped — negate y or transform before passing.

## Link annotations

Add clickable link annotations to any page — external URIs or internal page
jumps. Works on both loaded and created documents. The annotation border is
suppressed by default (invisible clickable region).

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);
const page = doc.getPage(0);

// External URI link over a text region
page.drawLink({ x: 50, y: 746, width: 140, height: 18, url: "https://example.com" });

// Internal jump — table-of-contents entry pointing to page 2 (0-based)
page.drawLink({ x: 50, y: 700, width: 200, height: 18, goToPage: 2 });

const output = await doc.save();
```

- Exactly one of `url` or `goToPage` must be provided (throws if both or neither).
- `goToPage` is 0-based (matches `doc.getPage(i)`).
- Named destinations and cross-document jumps (`GoToR`) are not supported.

## Rotate & resize pages

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

// Rotate a loaded page
const doc = await PdfDocument.load(bytes);
doc.getPage(0).setRotation(90);   // clockwise 90° — must be multiple of 90
const output = await doc.save();

// Resize a created page
const doc2 = await PdfDocument.create();
const page = doc2.addPage([595, 842]);   // A4
page.setSize(612, 792);                  // switch to US Letter
// or equivalently: page.setMediaBox(0, 0, 612, 792);
const output2 = await doc2.save();
```

- `setRotation` normalises to 0/90/180/270; non-multiples throw `InvalidRotationError`.
- All three methods work on both `doc.getPage(i)` (loaded) and `doc.addPage(...)` (created).

## Document metadata

Read and write the PDF Info dictionary on both created and loaded documents.

```ts
// Write (created or loaded doc — works on both)
doc.setTitle("Report");
doc.setAuthor("Alice");
doc.setSubject("Q2 financials");
doc.setKeywords(["finance", "Q2"]);   // string[]
doc.setCreator("Acme App");
doc.setProducer("better-pdf");
doc.setCreationDate(new Date("2026-01-01T00:00:00Z"));
doc.setModificationDate(new Date());

// Read back
const meta = await doc.getMetadata();
// meta: { title?, author?, subject?, keywords?: string[], creator?, producer?,
//         creationDate?: Date, modDate?: Date }
console.log(meta.title, meta.keywords, meta.creationDate);
```

- On a **loaded** PDF the setters emit an incremental update; Info-dict keys you do not touch are preserved.
- Dates round-trip: `setCreationDate(d)` / `setModificationDate(d)` accept `Date`; `getMetadata()` returns `Date`.
- Only the PDF Info dictionary is written — XMP metadata streams are not modified.
- API: `doc.setTitle | setAuthor | setSubject | setKeywords | setCreator | setProducer | setCreationDate | setModificationDate` + `await doc.getMetadata()` → `DocumentMetadata`.

## Browser

Import from `better-pdf/browser` (initializes a web-target WASM build); same API.
