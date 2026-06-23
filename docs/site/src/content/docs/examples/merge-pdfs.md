---
title: Merge PDFs
description: Combine multiple PDFs into one; merged AcroForm fields stay interactive.
---

Combine whole documents, or assemble an exact page order from several sources.
Both return a fresh `Uint8Array` — no source document is mutated.

## Merge whole documents

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const a = await Bun.file("cover.pdf").bytes();
const b = await Bun.file("body.pdf").bytes();
const c = await Bun.file("annex.pdf").bytes();

const merged = await PdfDocument.merge([a, b, c]);   // all pages, in order
await Bun.write("merged.pdf", merged);
```

## Assemble an exact page order

```ts
const result = await PdfDocument.assemble(
  [cover, body, annex],
  [
    { docIndex: 0, pageIndex: 0 },   // cover p0
    { docIndex: 1, pageIndex: 0 },   // body  p0
    { docIndex: 1, pageIndex: 1 },   // body  p1
    { docIndex: 2, pageIndex: 0 },   // annex p0
  ],
);
await Bun.write("assembled.pdf", result);
```

:::note[Form fields stay fillable]
If any merged/assembled page came from a document with AcroForm fields, those
fields remain **interactive** in the output (0.15.0): a working `/AcroForm` is
rebuilt, merging each source's `/DR` fonts and `/DA` and setting
`/NeedAppearances true`. Names that collide across sources are renamed with a
per-source prefix (`d0_`, `d1_`, …). `/XFA` data is dropped (plain AcroForm),
and a page selected twice in `assemble` shares one field object.
:::

To reorder or extract pages within a single document instead, load it and use
`doc.copyPages([...])` or `doc.splitPages()` — see
[Generating & drawing](/guides/generating/#pages-merge-extract-split).
