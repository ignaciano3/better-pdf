# Migrating from pdf-lib

`@ignaciano3/better-pdf` covers pdf-lib's form-filling workflow with a faster
core and stricter validation. This guide maps the APIs.

## API mapping

| pdf-lib | better-pdf |
| --- | --- |
| `PDFDocument.load(bytes)` | `PdfDocument.load(bytes)` |
| `pdfDoc.getForm()` | `doc.getForm()` |
| `form.getFields()` | `form.getFields()` → plain `FieldInfo[]` (name/type/value/states/options/…) |
| `form.getTextField(n).setText(v)` | `form.getTextField(n).setText(v)` |
| `field.updateAppearances(customFont)` | `form.getTextField(n).setText(v, { font })` — pass the font at fill time; `font` must come from `doc.embedFont(bytes)` (Unicode/CJK), not a standard-14 handle |
| `form.getCheckBox(n).check()` / `.uncheck()` | same — uses the field's real on-state automatically |
| `form.getRadioGroup(n).select(v)` | same — `v` must be a real export value (`field.states`) |
| `form.getDropdown(n).select(v)` | same — `v` must be a real option (`field.options`) |
| `form.getOptionList(n).select(v)` | `form.getListBox(n).select(v)` — or `selectMultiple(values)` for multi-select list boxes |
| `field.acroField.getWidgets()` | `field` info's `widgets` (`{page, rect}` per widget) |
| `form.flatten()` | `form.flatten()` (or `form.flattenField(name)`) |
| `pdfDoc.save()` | `doc.save()` — **incremental, append-only** |
| `form.updateFieldAppearances()` | not needed — appearances are generated on fill |
| `doc.attach(bytes, name, opts)` | `doc.attach(bytes, name, opts)` — same shape; `creationDate`/`modificationDate` are NOT defaulted; duplicates throw `DuplicateAttachmentError` instead of silently appending |
| *(no read API)* | `doc.getAttachments()` returns metadata + bytes |

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
- **Missing glyphs throw, not silently blank.** Filling a field with an
  embedded font that lacks a glyph for some character throws
  `MissingGlyphError` — pdf-lib's `updateAppearances` silently renders a blank
  for unsupported characters instead. Comb, dropdown, and listbox fields still
  reject an embedded font and remain standard-14 only.
- **Scope (as of 0.2.0):** creation, page drawing, and form filling are all
  covered. Encryption is not supported.

## Generating documents

better-pdf 0.2.0 covers pdf-lib's document generation API. The method names are
largely identical; the differences are noted below.

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
| `font.widthOfTextAtSize(text, size)` | same |
| `page.drawText(text, options)` | same |
| `page.drawImage(img, options)` | same |
| `page.drawRectangle(options)` | same |
| `page.drawLine(options)` | same |
| `page.drawEllipse(options)` | same — see note below |
| `rgb(r, g, b)` / `grayscale(v)` | same |
| `StandardFonts.Helvetica` etc. | same enum values |
| `pdfDoc.save()` | `doc.save()` — returns `Promise<Uint8Array>` |

### Differences from pdf-lib

- **Form creation uses a builder.** pdf-lib mutates `form` in place via
  `form.createTextField(...)`; better-pdf accumulates fields through a chainable
  `doc.createForm()` builder (see below). `getForm()` itself is not available on
  a created document until it is saved and reloaded.
- **RGB and grayscale only.** CMYK color is not supported.
- **Ellipse center semantics and option names.** `drawEllipse({ x, y, radiusX, radiusY, … })` uses
  `(x, y)` as the center and `radiusX`/`radiusY` as the x and y radii. pdf-lib used
  `xScale`/`yScale` — rename these when migrating. Fill/stroke options also follow
  the unified naming: `fill`, `stroke`, `strokeWidth` (not pdf-lib's `color`/`borderColor`/`borderWidth`).
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
