---
title: Introduction
description: What better-pdf is, what it covers, and its current status.
---

`@ignaciano3/better-pdf` is a maintained, fast alternative to `pdf-lib` for PDF
AcroForms and document generation.

It exposes a TypeScript API backed by a Rust core compiled to WebAssembly, and
covers two workflows:

1. **AcroForm-first** — load an existing PDF, inspect fields, fill / flatten /
   sign, and save an append-only incremental update.
2. **Generate & draw** — create new PDFs from scratch or stamp text, images, and
   vector graphics onto existing pages.

## Features

- Read AcroForm fields with fully-qualified names, types, values, options, and
  button states.
- Fill text fields and text areas.
- Check/uncheck checkboxes using the real on-state value.
- Select radio options using real export values.
- Select dropdown and list-box options.
- Add visual-only signature images from JPEG or supported PNG bytes.
- Flatten one field or all fields after filling.
- Save append-only incremental PDF updates.
- Create new PDFs with `PdfDocument.create()` and standard page sizes.
- Draw text, images, lines, rectangles, and ellipses on new and existing pages.
- Create fillable AcroForm fields (text, checkbox, radio, dropdown, listbox,
  signature) on generated documents with `doc.createForm()`.

## Status

Pre-1.0. The core AcroForm workflows — reading, filling, flattening, visual
signatures, and typed form-type generation — are implemented and tested against
the bundled PDF 1.3 fixture corpus. PDF generation (create, addPage, drawText,
drawImage, drawRectangle, drawLine, drawEllipse) and form-field creation are
included. The public API may still change before 1.0.

Coming from pdf-lib? See the [migration guide](/better-pdf/migrating/from-pdf-lib/).
