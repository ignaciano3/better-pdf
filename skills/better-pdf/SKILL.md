---
name: better-pdf
description: Fill and flatten PDF AcroForm fields (text, checkbox, radio, dropdown, visual signature) in existing PDFs with the @ignaciano3/better-pdf npm package, generate TypeScript types from a PDF form for compile-time-safe filling, create new PDFs from scratch, draw text with custom TTF/OTF fonts (full Unicode including CJK), draw images and vector graphics, read/write PDF document metadata (title/author/keywords/dates), merge multiple PDFs, extract/copy/reorder pages, split PDFs into single-page files, rotate or resize individual pages. Use when filling or flattening PDF forms, reading AcroForm fields, embedding a visual signature image, creating PDF documents, drawing Unicode text, reading or setting PDF metadata, merging PDFs, extracting or reordering pages, rotating or resizing pages, or when the user mentions better-pdf, pdf-lib, or AcroFields.
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
| `page.setRotation(degrees)` | Rotate page (multiple of 90; normalised) — loaded or created |
| `page.setSize(width, height)` | Resize page (sugar for setMediaBox(0,0,w,h)) — loaded or created |
| `page.setMediaBox(x0, y0, x1, y1)` | Set PDF /MediaBox directly — loaded or created |
| `doc.embedFont(bytes, { subset? })` → `Promise<PdfFont>` | Embed TTF/OTF; returns a `PdfFont` for `drawText` |
| `doc.getForm()` / `doc.getForm<typeof schema>()` | Untyped / type-narrowed form view |
| `form.getFields()` / `form.getField(name)` | `FieldInfo[]` / one `FieldInfo` |
| `form.getTextField(name).setText(v)` | Set text |
| `form.getCheckBox(name).check()` / `.uncheck()` | Toggle using real on-state |
| `form.getRadioGroup(name).select(v)` | Select by real export value |
| `form.getDropdown(name).select(v)` | Select by real option value |
| `form.getListBox(name).select(v)` | Select list-box option (single-select) |
| `form.getSignature(name).setImage(bytes)` | Embed visual signature (JPEG/PNG) |
| `form.flattenField(name)` / `form.flatten()` | Flatten one / all fields |
| `generateFormTypes(fields, { typeName })` | Emit a typed `…Fields` module (string) |

`FieldInfo = { name, type, value, states, options, readOnly, required, exported, maxLength, widgets }`, where `exported` is false only when the `NoExport` flag is set, `maxLength` is a text field's `/MaxLen` (or null), and `widgets: { page, rect: [x0,y0,x1,y1] }[]` gives each widget's 0-based page index and `/Rect` in PDF points (origin bottom-left). `setText` throws if longer than `maxLength`. `type` ∈ `text | checkbox | radio | dropdown | listbox | signature | pushbutton | unknown`.

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
- Form fields on merged/assembled pages keep their **visual appearance** but are **not interactive** — the AcroForm is not reconstructed. Flatten before merging if needed.
- In-place page rotation/resize is **supported**: `page.setRotation(degrees)`, `page.setSize(w, h)`, `page.setMediaBox(x0, y0, x1, y1)` — works on loaded and created pages.
- Blank-page insertion is not yet available.

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
