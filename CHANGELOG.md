# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the version is `0.x`, the public API may change between minor releases.

## [Unreleased]

## [0.13.0] - 2026-06-19

### Added

- Page insertion on loaded documents: `doc.addPage(size?)` now works on loaded PDFs (appends a blank, drawable page); `doc.insertPage(index, size?)`, `doc.removePage(index)`, `doc.movePage(from, to)`. Incremental — existing forms and content are preserved. Appended pages are drawable in the same save; inserted/removed/moved pages are reflected after save + reload.

### Fixed

- Non-ASCII document metadata: `setTitle`/`setAuthor`/etc. now correctly encode non-Latin text (Japanese, accented Latin, etc.) as UTF-16BE, ensuring round-trip fidelity.
- Palette (indexed-color) PNG embedding: color-type-3 PNGs with `tRNS` transparency now embed correctly (transparency stored as a soft mask, same as RGBA PNGs).

## [0.12.0] - 2026-06-19

### Added

- Text rotation & opacity (`drawText({rotate, opacity})`) and document outlines/bookmarks (`doc.setOutline()`), on loaded and created PDFs.

## [0.11.0] - 2026-06-18

### Added

- Vector paths: `page.drawSvgPath()` (SVG path data) and `page.drawPolygon()` with fill/stroke/opacity, on loaded and created PDFs. SVG arcs (A/a) not yet supported.

## [0.10.0] - 2026-06-18

### Added

- Link annotations: `page.drawLink()` adds clickable external-URI and internal page-jump links on loaded and created PDFs.

## [0.9.0] - 2026-06-18

### Added

- Embed PDF pages: `doc.embedPdfPage(src, pageIndex)` + `page.drawPage(embedded, {x, y, width?, height?})` stamp a page from another PDF as a Form XObject, on loaded and created PDFs. `width`/`height` default to the source page's intrinsic MediaBox size. Interactive form fields and annotations on the embedded page are flattened to static visual appearance only.

## [0.8.0] - 2026-06-18

### Added

- PNG transparency: the alpha channel of RGBA and gray+alpha PNGs is preserved
  as a soft mask (`/SMask`) on embedded images. `embedPng` + `drawImage` just
  work — no API change. Opaque RGB/grayscale PNGs are unaffected.

## [0.7.0] - 2026-06-18

### Added

- Page rotate/resize: `page.setRotation()`, `page.setSize()`, `page.setMediaBox()`
  on loaded and created PDFs.
- `page.setRotation(degrees)` — rotate a page by any multiple of 90; value is
  normalised to 0 / 90 / 180 / 270. Non-multiples of 90 throw `InvalidRotationError`.
- `page.setSize(width, height)` — resize a page (convenience wrapper around
  `setMediaBox(0, 0, width, height)`).
- `page.setMediaBox(x0, y0, x1, y1)` — set the PDF `/MediaBox` directly; useful
  when the page has a non-zero origin.
- All three methods work on pages from both `doc.getPage(i)` (loaded documents)
  and `doc.addPage(...)` (created documents).

## [0.6.0] - 2026-06-17

### Added

- Page operations: merge multiple PDFs, extract/copy/reorder pages, and split
  into single-page PDFs — `PdfDocument.merge` / `PdfDocument.assemble`,
  `doc.copyPages` / `doc.splitPages`.
- `PdfDocument.merge(docs: Uint8Array[]): Promise<Uint8Array>` — combine an
  array of PDFs into one document (all pages, in order).
- `PdfDocument.assemble(docs, selections): Promise<Uint8Array>` — build a new
  PDF from an explicit ordered selection of `{docIndex, pageIndex}` entries
  across multiple source documents; supports reorder, cross-doc copy, and
  page removal by omission.
- `doc.copyPages(indices: number[]): Promise<Uint8Array>` — extract the given
  pages (0-based) from a loaded document into a new PDF.
- `doc.splitPages(): Promise<Uint8Array[]>` — split a loaded document into one
  single-page PDF per page.
- Inherited page attributes (MediaBox, Resources, Rotate, CropBox) are resolved
  so extracted pages stand alone.

### Notes

- Form fields on assembled or merged pages keep their **visual appearance** (the
  field appearance stream is baked onto the page) but are **not interactive** —
  no AcroForm dictionary is reconstructed in the output. Flatten fields before
  merging to produce a flat, printable result.
- In-place page rotation/resize and blank-page insertion are not yet available
  (rotation/resize is planned for M29).

## [0.5.0] - 2026-06-17

### Added

- Document metadata: read/write the PDF Info dictionary (Title, Author, Subject, Keywords,
  Creator, Producer, CreationDate, ModDate) on both loaded and created PDFs via
  `doc.setTitle()` / `doc.setAuthor()` / `doc.setSubject()` / `doc.setKeywords(arr)` /
  `doc.setCreator()` / `doc.setProducer()` / `doc.setCreationDate(d)` /
  `doc.setModificationDate(d)`, and `await doc.getMetadata()` → `DocumentMetadata`.
- On a **loaded** PDF, metadata setters write an incremental update; Info-dict keys that are
  not set are preserved from the original document.
- Dates round-trip: setters accept a JS `Date`; `getMetadata()` returns `Date` objects.

### Notes

- XMP metadata streams are not written or modified (only the Info dictionary is updated).

## [0.4.0] - 2026-06-17

### Added

- `doc.embedFont(bytes, { subset? })` — embed any TTF or OTF font and get back a
  `PdfFont` for use with `drawText`. Fonts are embedded as Type0/CIDFontType2
  composite fonts with a ToUnicode CMap, enabling full Unicode text (including CJK
  and accented Latin) that is selectable and searchable in PDF viewers.
- `{ subset: boolean }` option (default `true`) — strips the embedded font to only
  the glyphs actually used in the document, keeping output file sizes small.
- `widthOfTextAtSize` works on embedded fonts, enabling layout calculations.
- Both `PdfDocument.create()` and `PdfDocument.load()` paths support `embedFont`;
  embedded fonts can be used on both created and existing pages.

### Known caveats

- The subsetter supports TrueType (`glyf`) outlines. OpenType-CFF (`.otf` files
  whose outlines are CFF rather than `glyf`) may fail to subset — pass
  `{ subset: false }` for those fonts.
- Characters with no glyph in the embedded font are silently skipped.

## [0.3.0] - 2026-06-13

### Added

- `doc.createForm()` — a chainable `FormBuilder` for adding AcroForm fields to a
  document created with `PdfDocument.create()` (throws on loaded documents).
- Field creation methods: `addTextField`, `addCheckBox`, `addRadioGroup`,
  `addDropdown`, `addListBox`, and `addSignatureField`, covering all six field
  types: text, checkbox, radio, dropdown, listbox, and signature.
- Per-field options: `value`, `readOnly`, `required`, `tooltip`, `maxLength`,
  and `multiline` (text fields), plus `checked`/`onValue` (checkboxes) and
  `selected` (radio/choice fields).
- Per-field `border` (`{ color, width? }`) and `background` (a `Color`)
  appearance options, using `rgb`/`grayscale` color helpers.
- Typed field-name accumulation: each `add*` call refines the builder's schema,
  so `getFieldNames()` returns the declared names typed. Generated forms are
  normal fillable AcroForms — after `save()` and reload via `PdfDocument.load`
  they can be filled with `getForm()` and flattened by this library.

## [0.2.0] - 2026-06-13

### Added

- Page access on loaded documents: `doc.getPageCount()`, `doc.getPages()`, and
  `doc.getPage(i)` returning a `PdfPage`; throws `PageOutOfRangeError` for
  out-of-range indices.
- `drawText` on both loaded and created pages, with `x`, `y`, `size`, `font`,
  `color`, and `lineHeight` options.
- `PdfDocument.create()` — create a new empty document without a source PDF.
- `doc.addPage(size)` — append a page given a `[width, height]` tuple;
  `PageSizes` constants (`A3`, `A4`, `A5`, `Letter`, `Legal`, `Tabloid`)
  provide the standard sizes.
- `doc.embedJpg(bytes)` / `doc.embedPng(bytes)` — embed images into the
  document, returning a `PdfImage` with `.width`, `.height`, and `.scale(f)`.
- `page.drawImage(img, options)` — draw an embedded image on a page with
  `x`, `y`, `width`, and `height`.
- `page.drawLine(options)` — draw a line segment with `start`, `end`,
  `thickness`, `color`, and `opacity`.
- `page.drawRectangle(options)` — draw a filled and/or bordered rectangle with
  `x`, `y`, `width`, `height`, `color`, `borderColor`, `borderWidth`, and
  `opacity`.
- `page.drawEllipse(options)` — draw a filled and/or bordered ellipse; `(x, y)`
  is the center and `xScale`/`yScale` are the x and y radii.
- `rgb(r, g, b)` and `grayscale(v)` color helpers.
- `StandardFonts` enum: `Helvetica`, `HelveticaBold`, `HelveticaOblique`,
  `HelveticaBoldOblique`, `Courier`, `CourierBold`, `CourierOblique`,
  `CourierBoldOblique`, `TimesRoman`, `TimesBold`, `TimesItalic`,
  `TimesBoldItalic`.
- `doc.getFont(StandardFonts.X)` — returns a `PdfFont` with
  `font.widthOfTextAtSize(text, size)` for layout calculations.
- `./forms` and `./generate` subpath exports for tree-shaking-friendly imports.
- `PageOutOfRangeError` and `InvalidImageError` added to the `PdfError` family.

## [0.1.2] - 2026-06-11

### Changed

- Expanded TypeScript API documentation with pdf-lib-style JSDoc examples,
  parameters, return values, and error notes for document, form, field, and type
  generation APIs.
- Aligned the Rust/WASM core package version with the npm package version.

### Fixed

- Filled text values with accented and other non-ASCII characters are now stored
  as proper PDF text strings and decode correctly when fields are read back.

## [0.1.1] - 2026-06-11

### Added

- `FieldInfo.maxLength` (text field `/MaxLen`) and `FieldInfo.exported`
  (false when the `NoExport` flag is set); both also emitted by the type
  generator. `setText()` now throws `MaxLengthExceededError` past `/MaxLen`.
- `getListBox(name).select(value)` write accessor for single-select list-box
  fields, including the typed `doc.getForm<typeof schema>()` overlay.
- Typed error classes: `PdfError` base plus `UnknownFieldError`,
  `FieldTypeError`, `InvalidOptionError`, `MaxLengthExceededError`, and
  `MissingOnStateError`, all exported from the package root and browser entry.

### Changed

- Package renamed to `@ignaciano3/better-pdf` (the unscoped npm name is taken).
- Ships a single WASM binary (web target); Node loads it synchronously from disk.
- Signature images cross the JS↔WASM boundary as binary, not JSON number arrays.
- `FieldInfo.value` now reflects queued mutations immediately; `save()` always
  starts from the originally loaded bytes.
- Tooling: TypeDoc API reference (`bun run docs`), a real headless-Chromium
  browser test (`bun run test:browser`) wired into CI, and a `LICENSE` shipped
  with the published WASM package.

### Added

- `PdfCoreError`: core (WASM) failures from `save()` are part of the `PdfError` family.
- Standard-14 font metrics with Arial/Times New Roman/Courier New aliases,
  `/Widths`-array fallback, and full WinAnsi text encoding (€, smart quotes, …).
- XFA-backed forms are detected and rejected on fill/flatten with a clear error.
- CMYK JPEG signature images are rejected instead of being mislabeled RGB.
- Validation: `qpdf --check` in CI, a pdf.js render regression check, fuzz
  targets for the PDF/image/DA parsers, and xref-stream/object-stream fixtures.

## [0.1.0]

First public pre-release. Fill and flatten AcroForm fields in existing PDFs,
from both the browser and server runtimes, via a Rust core compiled to
WebAssembly with a fully-typed TypeScript API.

### Added

- Load a PDF and read AcroForm fields: fully-qualified `name`, `type`, `value`,
  `states`, `options`, `readOnly`, `required`, and per-widget `widgets`
  (0-based `page` index and `rect` in PDF points).
- Typed mutation accessors: `getTextField`, `getCheckBox`, `getRadioGroup`,
  `getDropdown`, and `getSignature`, using a field's real export values.
- Self-generated appearance streams so filled and flattened fields render
  without relying on `/NeedAppearances`.
- Flatten one field (`flattenField`) or all fields (`flatten`).
- Visual-only signature images from JPEG and supported PNG inputs.
- Append-only incremental saves (`save()`); a no-op save is a byte-exact
  round trip.
- Form type generator (`generateFormTypes` and the
  `better-pdf-generate-types` CLI) plus a type-only narrowed
  `doc.getForm<typeof schema>()` that turns unknown field names, wrong-type
  access, and invalid option/state values into compile errors at zero runtime
  cost.
- Browser entry (`better-pdf/browser`) backed by the `--target web` WASM build.
- Agent skill shipped in the package for AI-driven usage.
- `pdf-lib` comparison benchmark harness (`bun run bench`).

### Known limitations

- Existing PDFs only; no PDF creation, encryption, or malformed-PDF recovery.
- No cryptographic/PAdES signing — signatures are appearances only.
- Text fields are single-line; multi-line wrapping is not generated.
- PNG alpha is dropped rather than preserved as a soft mask.

[Unreleased]: https://github.com/ignaciano3/better-pdf/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/ignaciano3/better-pdf/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/ignaciano3/better-pdf/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/ignaciano3/better-pdf/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/ignaciano3/better-pdf/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ignaciano3/better-pdf/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/ignaciano3/better-pdf/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ignaciano3/better-pdf/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ignaciano3/better-pdf/releases/tag/v0.1.0
