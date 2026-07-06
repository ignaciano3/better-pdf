---
title: How PDFs work
description: A short, byte-level tour of the PDF file format — objects, xref tables, incremental updates, content streams, forms, fonts, and compression — and how better-pdf puts each piece to work.
---

PDF looks opaque from the outside — a binary blob that opens in a viewer and
otherwise resists inspection. It's more legible than it looks. The skeleton
of every PDF is text: a flat list of numbered objects, a lookup table that
finds them, and a handful of small, well-defined sub-languages for drawing
pages, describing form fields, and encoding text. In real-world files the
flesh on that skeleton — page content, fonts, often the lookup table itself —
is usually deflate-compressed into unreadable bytes, so opening one in a text
editor shows readable structure interleaved with binary noise (the
[compression article](/better-pdf/internals/compression/) shows both sides).
This section is a byte-level tour of that structure, written for developers
who use PDFs every day but have never had a reason to read the ISO 32000
specification. Each article opens a real file, points at the bytes that
matter, and ends by showing how better-pdf reads, builds, or edits that exact
piece of structure.

You don't need to read straight through — the articles are independent, and
each one names the concept it depends on so you can jump to whichever problem
you're debugging. Read the whole section and you'll come away with a mental
model of a PDF file that survives contact with a hex editor: what's stored,
where, and why the format is shaped the way it is.

## Read in order, or jump in

- **[Anatomy of a PDF file](/better-pdf/internals/file-anatomy/)** — the four
  parts every PDF is built from, shown on a complete six-object file small
  enough to read end to end.
- **[Objects & the xref table](/better-pdf/internals/objects-and-xref/)** —
  why a PDF behaves like a random-access database of numbered objects rather
  than a linear document.
- **[Incremental updates](/better-pdf/internals/incremental-updates/)** — why
  editing a PDF usually means appending new bytes to the end of the file, not
  rewriting the ones already there.
- **[Content streams & operators](/better-pdf/internals/content-streams/)** —
  the tiny postfix language, built from a couple dozen operators, that paints
  every page you've ever viewed.
- **[AcroForms: how fields really work](/better-pdf/internals/forms/)** —
  field dictionaries, the widget annotations that draw them, and what
  "flatten" actually removes.
- **[Fonts & text encoding](/better-pdf/internals/fonts/)** — why extracting
  text from a PDF is harder than it looks, and what a Type0 font is doing in
  the middle of it.
- **[Compression & object streams](/better-pdf/internals/compression/)** —
  the stream filters PDFs use to shrink content, and how modern files pack
  whole groups of objects into a single compressed stream.

Every article decodes real bytes from a real file. The offsets and object
numbers in the examples are illustrative — yours will differ — but the
structures they point at are exactly what you'll find if you open any PDF
yourself.
