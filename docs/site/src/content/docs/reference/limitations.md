---
title: Limitations
description: What better-pdf does not (yet) support.
---

- XFA forms are detected and rejected on fill/flatten (reading fields still works).
- No encrypted PDF support.
- No lenient recovery for malformed PDFs.
- No cryptographic signing.
- List boxes are single-select; multi-select list boxes are not yet supported.
- Text fields are single-line; multi-line wrapping is not yet generated.
- Drawing APIs support standard-14 fonts and custom TTF/OTF font embedding via
  `doc.embedFont(bytes)`. Embedded fonts render as Unicode-capable Type0/CIDFontType2
  composites with a ToUnicode CMap — full Unicode (including CJK and accented Latin)
  is selectable and searchable.
  - **Caveat:** glyph subsetting uses the `subsetter` crate which supports TrueType
    (`glyf`) outlines; OpenType-CFF (`.otf` files that use CFF outlines rather than
    `glyf`) may fail to subset. Pass `{ subset: false }` to skip subsetting for those
    fonts.
  - Characters with no glyph in the font are silently skipped.
- Appearance metrics cover the standard 14 text fonts (with Arial / Times New
  Roman / Courier New aliases and subset-prefix handling) and any simple font
  carrying a `/Widths` array; unrecognized fonts fall back to Helvetica metrics.
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
  - **SVG arc commands (`A`/`a`) are not yet supported** — they throw at call time.
    Supported commands: `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`, `S`/`s`,
    `Q`/`q`, `T`/`t`, `Z`/`z`.
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
- Primary test coverage is the bundled fixture corpus (classic-xref PDF 1.3
  forms, plus generated xref-stream/object-stream variants).
- Browser support expects a modern bundler/runtime that can serve the packaged
  `.wasm` asset referenced from the browser entry.
