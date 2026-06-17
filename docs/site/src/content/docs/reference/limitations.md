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
- Primary test coverage is the bundled fixture corpus (classic-xref PDF 1.3
  forms, plus generated xref-stream/object-stream variants).
- Browser support expects a modern bundler/runtime that can serve the packaged
  `.wasm` asset referenced from the browser entry.
