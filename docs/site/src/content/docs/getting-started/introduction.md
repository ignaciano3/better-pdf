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
  signature) with `doc.createForm()` — on generated documents and on documents
  opened with `PdfDocument.load()` (added fields must precede the first
  `getForm()`).
- Decrypt and modify encrypted PDFs (RC4 / AES-128 / AES-256) with
  `PdfDocument.load(bytes, { password })`, and detect/classify passwords with
  `PdfDocument.isEncrypted` / `PdfDocument.passwordType`.
- Deflate-compress generated streams on save (on by default) and optionally pack
  objects into PDF object streams for smaller output.

## Status

Stable 1.x (currently 1.14.x). The public API is frozen as of 1.0.0 and the
package follows Semantic Versioning — breaking changes only in major releases.
The full feature set — AcroForm reading/filling/flattening/visual-signatures/
typed-form generation, PDF generation and drawing, custom TTF/OTF font embedding
with Unicode/CJK, metadata, outlines, page operations, rotation/resize, PNG
transparency and palette, PDF page embedding, file attachments, stream
compression/object streams, and encrypted-PDF decryption — is implemented and
tested against the bundled PDF 1.3 fixture corpus (classic xref) plus
xref-stream/object-stream and malformed-PDF corpora.

Coming from pdf-lib? See the [migration guide](/better-pdf/migrating/from-pdf-lib/).
