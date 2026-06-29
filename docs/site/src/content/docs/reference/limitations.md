---
title: Limitations
description: What better-pdf does not (yet) support, and what it will never support.
---

**Limitations** are gaps we intend to close. **[Non-Goals](#non-goals)** are
deliberately unsupported and not planned. The two are listed separately below.

## Limitations

- **Encrypted PDFs are decrypted on load** (RC4, AES-128, AES-256) when you pass a
  password: `PdfDocument.load(bytes, { password })`. Use `{ password: "" }` for
  owner-locked / empty-user-password files. (Decryption is opt-in — bare
  `load(bytes)` does not decrypt, so an encrypted file loaded without a password
  throws `EncryptedPdfError` telling you to pass one; a wrong password throws
  `IncorrectPasswordError`.) Modifying an encrypted PDF produces a **decrypted**
  output. **Still unsupported:** producing encrypted output (re-encryption) and
  encrypting documents you create.
- No cryptographic signing (the API leaves room to add PAdES later).
- **Appearance-affecting form-field flags are set at creation only.** The
  `multiline`, `comb`, and `password` flags can be set when [creating a
  field](/better-pdf/guides/creating-form-fields/) but **not toggled on a loaded
  field**, because changing them requires regenerating the field's appearance
  stream rather than just flipping a bit. (The ReadOnly / Required / NoExport
  field flags and the Hidden / Print / NoView widget flags *can* be changed on a
  loaded field — see [Filling forms](/better-pdf/guides/filling-forms/).)
- **Rich-text fields are not supported.** The PDF Rich Text flag (`/Ff` bit 26)
  and `/RV` value are ignored; field values are read and written as plain text.
- Drawing APIs support standard-14 fonts and custom TTF/OTF font embedding via
  `doc.embedFont(bytes)`. Embedded fonts render as Unicode-capable Type0/CIDFontType2
  composites with a ToUnicode CMap — full Unicode (including CJK and accented Latin)
  is selectable and searchable.
  - **Caveat:** glyph subsetting uses the `subsetter` crate which supports TrueType
    (`glyf`) outlines; OpenType-CFF (`.otf` files that use CFF outlines rather than
    `glyf`) may fail to subset. Pass `{ subset: false }` to skip subsetting for those
    fonts.
  - Characters with no glyph in the font are silently skipped.
- **Multi-line text:** `drawText` honors `\n` as hard line breaks, and the
  `maxWidth` option word-wraps text to fit a given width (added in 0.14.0).
  Filling a form text field that carries the Multiline flag also produces a
  wrapped, top-aligned multi-line appearance with per-line quadding (added in
  0.16.0). In both cases a single word wider than the available width overflows
  onto its own line; mid-word breaking is not performed.
- Appearance metrics cover the standard 14 text fonts (with Arial / Times New
  Roman / Courier New aliases and subset-prefix handling) and any simple font
  carrying a `/Widths` array; unrecognized fonts fall back to Helvetica metrics.
- **Form-field text appearance:** field values render in a **standard-14 font**
  — selectable per field via the builder `font` option (Helvetica / Times /
  Courier families), with `fontSize`, `textColor`, and `align` also
  configurable (and `checkStyle` for the selected mark of checkboxes and
  radios). **Embedded / non-Latin (CJK) fonts are not supported for
  form-field values** — only the standard-14 WinAnsi fonts.
- **Form-field format / calculation actions (AcroForm JavaScript)** are not
  supported — there is no API for `/AA` additional-action scripts such as date
  pickers, number/currency masks, validation, or calculated fields. These rely
  on viewer-side JavaScript (Acrobat) that most viewers (Chrome, Preview, etc.)
  do not run, so no equivalent appearance is generated.
- Color: RGB and grayscale only; CMYK is not supported.
- **Document metadata (Info dictionary):** Title, Author, Subject, Keywords, Creator,
  Producer, CreationDate, and ModDate are **supported** via `doc.setTitle()` /
  `doc.getMetadata()` etc. on both created and loaded documents. Dates round-trip to
  JS `Date`. Non-ASCII text (Japanese, accented Latin, etc.) is encoded as **UTF-16BE**
  for correct round-trip fidelity (added in 0.13.0). — **XMP metadata streams are not
  written or modified** (only the Info dictionary is updated).
- **Page operations (merge, copy, reorder, split)** — `PdfDocument.merge`,
  `PdfDocument.assemble`, `doc.copyPages`, and `doc.splitPages` — are **supported**.
  - **Interactive form fields survive merge/assemble (0.15.0):** Pages merged
    or assembled from documents with AcroForm fields keep those fields
    **interactive** in the output — `/AcroForm` is rebuilt with the kept
    fields, merged `/DR` fonts, and `/NeedAppearances true`. Field names that
    collide across source documents are renamed with a per-source prefix
    (`d0_`, `d1_`, …) so each stays independently fillable.
    - **Caveat:** the same page selected twice (`assemble` with a duplicate
      `{docIndex, pageIndex}`) shares one field object, so its fields are
      linked rather than renamed. `/XFA` data is dropped (output is a plain
      AcroForm).
  - **Page rotation and resize are now supported** via `page.setRotation(degrees)`,
    `page.setSize(width, height)`, and `page.setMediaBox(x0, y0, x1, y1)` on both
    loaded and created pages (added in 0.7.0).
    - **Caveat — page rotation must be a multiple of 90°** (`/Rotate` is a
      quarter-turn value per the PDF spec); other angles throw
      `UnsupportedRotationError`. Free-angle rotation is only available for
      `drawText({ rotate })`, which rotates the drawn glyphs, not the page.
  - **Page insertion/removal/move are now supported** via `doc.addPage(size?)` (appends; drawable in the same save), `doc.insertPage(index, size?)`, `doc.removePage(index)`, and `doc.movePage(from, to)` on loaded documents (added in 0.13.0). Incremental — forms and content are preserved.
    - **Caveat — nested page trees:** `insertPage`/`removePage`/`movePage` require a flat (single-level) page tree. PDFs with nested `Pages` nodes are rejected; use `PdfDocument.merge` or `PdfDocument.assemble` instead.
    - **Handles track their page (0.13.1):** a `PdfPage` handle follows its page across later `insertPage`/`removePage`/`movePage`; draws land on the right page regardless of call order. Drawing on a page you later remove throws at `save()`.
- **Link annotations** — `page.drawLink({ x, y, width, height, url })` (external
  URI) and `page.drawLink({ x, y, width, height, goToPage })` (internal
  page-index jump) are **supported** on both loaded and created PDFs (added in
  0.10.0).
  - **Border styling is minimal** — the annotation border is suppressed by
    default, producing an invisible clickable region. Custom border colors and
    widths are not exposed.
  - **Named destinations are not exposed** — internal links jump by 0-based page
    index only; PDF named destinations and `GoToR` cross-document jumps are not
    supported.
- **Embedding pages from other PDFs** — `doc.embedPdfPage(src, pageIndex)` +
  `page.drawPage(embedded, {x, y, width?, height?})` — is **supported** (added
  in 0.9.0). Works on both loaded and created documents.
  - **Caveat — interactive content flattened:** Only the page's visual content
    and resources are copied into the Form XObject. AcroForm fields,
    annotations, and hyperlinks on the embedded page are **not carried over** —
    they appear as their static visual appearance only. Flatten fields in the
    source PDF before embedding if their visual state must be exact.
- **Vector paths** — `page.drawSvgPath(d, options)` (SVG path-data string) and
  `page.drawPolygon(points, options)` with fill/stroke/opacity — are **supported**
  on both loaded and created PDFs (added in 0.11.0).
  - **Supported SVG commands:** `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`,
    `S`/`s`, `Q`/`q`, `T`/`t`, `Z`/`z`, and `A`/`a` (elliptical arcs are
    converted to cubic béziers).
  - **Coordinates are PDF user space (y-up).** SVG path data authored for the web
    (y-down) will appear vertically flipped unless you negate y values or apply a
    transform before calling `drawSvgPath`.
- **Text rotation and opacity** — `drawText({ rotate, opacity })` — are **supported**
  on both loaded and created PDFs (added in 0.12.0). `rotate` is free-angle (degrees,
  counter-clockwise about the text anchor); `opacity` is 0–1.
- **Document outlines / bookmarks** — `doc.setOutline(items)` — are **supported**
  on both loaded and created PDFs (added in 0.12.0). Items are
  `{ title: string; page: number; children?: OutlineItem[] }` at arbitrary depth.
- **PNG images:** RGBA and gray+alpha PNGs are **supported** — the alpha channel
  is preserved as a soft mask (`/SMask`) on the embedded image XObject (added in
  0.8.0). Opaque RGB / grayscale PNGs are also supported.
  - **Palette (indexed-color) PNGs with `tRNS` transparency are now supported** (added in 0.13.0) — the palette index is resolved and transparency is stored as a soft mask (`/SMask`).
  - **Still unsupported:** interlaced and 16-bit-per-channel PNGs.
- **Image formats are limited to PNG and JPEG** (`embedPng` / `embedJpg`). GIF,
  WebP, TIFF, and BMP are not supported.
  - **CMYK JPEGs are rejected** — JPEGs with 4 color components throw on embed;
    convert to RGB first.
- Primary test coverage is the bundled fixture corpus (classic-xref PDF 1.3
  forms, plus generated xref-stream/object-stream variants).
- Browser support expects a modern bundler/runtime that can serve the packaged
  `.wasm` asset referenced from the browser entry.

## Non-Goals

Deliberately unsupported. **Not planned** — legacy, rare, or better served by
another tool.

- **XFA forms** — Adobe's XML-based form format, deprecated and removed in
  PDF 2.0. Detected and rejected on fill/flatten; reading the static AcroForm
  fields still works.
- **Lenient recovery of malformed / off-spec PDFs** — the parser is strict by
  design and rejects broken structure rather than guessing at it.
