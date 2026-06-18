---
title: Generating & drawing
description: Create PDFs from scratch or stamp text, images, and vector graphics onto pages.
---

Import from the package root or the `./generate` subpath — both export the same
classes:

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";
```

:::note[Coordinate system]
Origin is bottom-left, y increases upward — same as pdf-lib and raw PDF.
:::

## Create a document, draw text

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);               // 595 × 842 pt

const font = doc.getFont(StandardFonts.Helvetica);
const text = "Hello, world!";
const textWidth = font.widthOfTextAtSize(text, 24);   // centre-align helper

page.drawText(text, {
  x: (PageSizes.A4[0] - textWidth) / 2,
  y: 750,
  size: 24,
  font,
  color: rgb(0.1, 0.2, 0.8),
});

const output = await doc.save();   // returns Uint8Array
await Bun.write("hello.pdf", output);
```

:::note[Fonts]
Standard-14 fonts (Helvetica, HelveticaBold, Courier, TimesRoman, …) are the
default. For Unicode text — including CJK, accented Latin, and any script not
covered by WinAnsi — embed a TTF or OTF font with `doc.embedFont()`. See
[Custom fonts](#custom-fonts) below.
:::

## Stamp onto an existing PDF

```ts
import { PdfDocument, rgb } from "@ignaciano3/better-pdf";

const bytes = new Uint8Array(await Bun.file("existing.pdf").arrayBuffer());
const doc = await PdfDocument.load(bytes);

const imgBytes = new Uint8Array(await Bun.file("logo.png").arrayBuffer());
const img = await doc.embedPng(imgBytes);             // PdfImage with .width / .height
const scaled = img.scale(0.5);                        // { width, height }

const page = doc.getPage(0);
page.drawImage(img, { x: 40, y: 700, width: scaled.width, height: scaled.height });
page.drawText("Confidential", { x: 40, y: 680, size: 12, color: rgb(0.8, 0, 0) });

const output = await doc.save();
```

`embedJpg` works the same way for JPEG files. Both methods are available on
loaded and created documents.

## Vector graphics

```ts
// filled + bordered rectangle with transparency
page.drawRectangle({
  x: 50, y: 50, width: 200, height: 100,
  color: rgb(0.9, 0.95, 1),
  borderColor: rgb(0.2, 0.4, 0.8),
  borderWidth: 2,
  opacity: 0.85,
});

// line
page.drawLine({
  start: { x: 50, y: 40 },
  end:   { x: 250, y: 40 },
  thickness: 1.5,
  color: rgb(0.5, 0.5, 0.5),
});

// ellipse — (x, y) is the centre; xScale/yScale are the x and y radii
page.drawEllipse({ x: 150, y: 200, xScale: 60, yScale: 30, color: rgb(1, 0.8, 0) });
```

## Text layout with `widthOfTextAtSize`

```ts
const font = doc.getFont(StandardFonts.HelveticaBold);
const label = "Invoice #1234";
const w = font.widthOfTextAtSize(label, 16);
page.drawText(label, { x: pageWidth - w - 40, y: pageHeight - 60, size: 16, font });
```

## Custom fonts

Embed any TTF or OTF font file to render Unicode text — including CJK characters,
accented Latin, and any script not covered by WinAnsi. The embedded font is a PDF
Type0/CIDFontType2 composite with a ToUnicode CMap, so text is selectable and
searchable in PDF viewers.

```ts
import { PdfDocument, PageSizes } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);

// Load your TTF/OTF file as bytes
const fontBytes = new Uint8Array(await Bun.file("NotoSansCJK-Regular.ttf").arrayBuffer());

// embedFont returns a PdfFont — subset: true (default) keeps only used glyphs
const font = await doc.embedFont(fontBytes, { subset: true });

const text = "日本語テキスト — Héllo Wörld";
const textWidth = font.widthOfTextAtSize(text, 18);

page.drawText(text, {
  x: (PageSizes.A4[0] - textWidth) / 2,
  y: 700,
  size: 18,
  font,
});

const output = await doc.save();
await Bun.write("unicode.pdf", output);
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `subset` | `boolean` | `true` | Subset the font to only the glyphs used in the document. Reduces file size significantly for large CJK fonts. Pass `false` to embed the full font. |

### Measuring embedded font text

`widthOfTextAtSize` works for embedded fonts exactly as it does for standard-14
fonts:

```ts
const w = font.widthOfTextAtSize("Ünïcödé", 14);
page.drawText("Ünïcödé", { x: 100, y: 400, size: 14, font });
```

### Caveats

- **OpenType-CFF:** The subsetter supports TrueType (`glyf`) outlines. `.otf`
  files that use CFF outlines instead of `glyf` may fail to subset — use
  `{ subset: false }` for those fonts.
- **Missing glyphs:** Characters with no glyph in the font are silently skipped.
- **Standard-14 default:** If you omit `font` from `drawText`, Helvetica is used
  as the default (WinAnsi, standard-14 behavior unchanged).

## Document metadata

Read and write the PDF Info dictionary — title, author, subject, keywords, creator,
producer, and dates — on both created and loaded documents.

```ts
import { PdfDocument, PageSizes, StandardFonts } from "@ignaciano3/better-pdf";

// Created document
const doc = await PdfDocument.create();
doc.addPage(PageSizes.A4);

doc.setTitle("Q2 Report");
doc.setAuthor("Ignacio Garcia P");
doc.setSubject("Quarterly financials");
doc.setKeywords(["finance", "Q2", "2026"]);
doc.setCreator("Acme Report Generator");
doc.setProducer("better-pdf");
doc.setCreationDate(new Date("2026-06-01T00:00:00Z"));
doc.setModificationDate(new Date());

const output = await doc.save();

// Read back on a loaded document
const loaded = await PdfDocument.load(output);
const meta = await loaded.getMetadata();

console.log(meta.title);        // "Q2 Report"
console.log(meta.author);       // "Ignacio Garcia P"
console.log(meta.keywords);     // ["finance", "Q2", "2026"]
console.log(meta.creationDate); // Date object
```

### API

| Method | Type | Description |
|--------|------|-------------|
| `doc.setTitle(s)` | `string` | Set /Title |
| `doc.setAuthor(s)` | `string` | Set /Author |
| `doc.setSubject(s)` | `string` | Set /Subject |
| `doc.setKeywords(arr)` | `string[]` | Set /Keywords (joined with `, ` in the PDF) |
| `doc.setCreator(s)` | `string` | Set /Creator |
| `doc.setProducer(s)` | `string` | Set /Producer |
| `doc.setCreationDate(d)` | `Date` | Set /CreationDate (PDF date syntax) |
| `doc.setModificationDate(d)` | `Date` | Set /ModDate (PDF date syntax) |
| `await doc.getMetadata()` | `Promise<DocumentMetadata>` | Read the Info dictionary |

`DocumentMetadata` fields are all optional (`string | undefined`, `string[] | undefined`, `Date | undefined`).
On a loaded PDF, keys not set by your code are preserved as-is in the incremental update.

:::note[XMP metadata]
Only the PDF Info dictionary is written. XMP metadata streams embedded in the
document are not modified or generated.
:::

## Pages: merge, extract, split

Page operations work across multiple loaded documents (static methods) and on a
single loaded document (instance methods). All four methods return a new
`Uint8Array` PDF — no source document is mutated.

### Merge multiple PDFs

Combine an array of PDFs into one, preserving the page order:

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const a = new Uint8Array(await Bun.file("part-a.pdf").arrayBuffer());
const b = new Uint8Array(await Bun.file("part-b.pdf").arrayBuffer());
const c = new Uint8Array(await Bun.file("part-c.pdf").arrayBuffer());

const merged = await PdfDocument.merge([a, b, c]);
await Bun.write("merged.pdf", merged);
```

### Extract / copy pages

Extract a subset of pages from a loaded document:

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const bytes = new Uint8Array(await Bun.file("report.pdf").arrayBuffer());
const doc = await PdfDocument.load(bytes);

// Extract pages 0, 2, and 4 (0-based) into a new PDF
const extracted = await doc.copyPages([0, 2, 4]);
await Bun.write("pages-0-2-4.pdf", extracted);
```

Omitting indices leaves them out — a practical way to remove pages.

### Split into single-page PDFs

Produce one PDF per page:

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const bytes = new Uint8Array(await Bun.file("report.pdf").arrayBuffer());
const doc = await PdfDocument.load(bytes);

const pages = await doc.splitPages();   // Promise<Uint8Array[]>
for (const [i, page] of pages.entries()) {
  await Bun.write(`page-${i + 1}.pdf`, page);
}
```

### Assemble pages from multiple sources

`PdfDocument.assemble` gives you full control over the output order and source:
each selection names a document (by index into the input array) and a page
(0-based index within that document). Pages may be reordered, repeated, or drawn
from any of the source documents.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const cover  = new Uint8Array(await Bun.file("cover.pdf").arrayBuffer());
const body   = new Uint8Array(await Bun.file("body.pdf").arrayBuffer());
const annex  = new Uint8Array(await Bun.file("annex.pdf").arrayBuffer());

// Build: cover p0, body p0–p2, annex p0, body p3
const result = await PdfDocument.assemble(
  [cover, body, annex],
  [
    { docIndex: 0, pageIndex: 0 },   // cover p0
    { docIndex: 1, pageIndex: 0 },   // body  p0
    { docIndex: 1, pageIndex: 1 },   // body  p1
    { docIndex: 1, pageIndex: 2 },   // body  p2
    { docIndex: 2, pageIndex: 0 },   // annex p0
    { docIndex: 1, pageIndex: 3 },   // body  p3
  ],
);
await Bun.write("assembled.pdf", result);
```

:::caution[Form fields on assembled pages]
Pages that originated from documents with AcroForm fields keep their **visual
appearance** (the field appearance stream is drawn on the page), but the fields
are **not interactive** — no AcroForm dictionary is reconstructed in the output.
If you need interactive forms, fill and flatten the fields before merging.
:::

## Rotate & resize pages

`page.setRotation`, `page.setSize`, and `page.setMediaBox` work on both loaded
(`doc.getPage(i)`) and created (`doc.addPage(...)`) pages.

### Rotate a loaded page

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

const bytes = new Uint8Array(await Bun.file("report.pdf").arrayBuffer());
const doc = await PdfDocument.load(bytes);

const page = doc.getPage(0);
page.setRotation(90);   // clockwise 90° — must be a multiple of 90

const output = await doc.save();
await Bun.write("rotated.pdf", output);
```

`setRotation` accepts any multiple of 90 (positive or negative) and normalises
it to 0 / 90 / 180 / 270. Non-multiples of 90 throw `InvalidRotationError`.

### Resize a created page

```ts
import { PdfDocument, PageSizes } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);          // 595 × 842 pt

// Resize to US Letter after creation
page.setSize(612, 792);                          // sugar for setMediaBox(0, 0, 612, 792)

// Or set the MediaBox explicitly (lower-left x0/y0, upper-right x1/y1)
page.setMediaBox(0, 0, 612, 792);

const output = await doc.save();
await Bun.write("letter.pdf", output);
```

`setSize(width, height)` is a convenience wrapper that calls
`setMediaBox(0, 0, width, height)`. Use `setMediaBox` directly when you need a
non-zero origin (e.g. an already-cropped page).

### API

| Method | Signature | Notes |
|--------|-----------|-------|
| `page.setRotation(degrees)` | `(degrees: number) => void` | Multiple of 90; normalised to 0/90/180/270 |
| `page.setSize(width, height)` | `(width: number, height: number) => void` | Sugar for `setMediaBox(0,0,w,h)` |
| `page.setMediaBox(x0, y0, x1, y1)` | `(x0: number, y0: number, x1: number, y1: number) => void` | Sets the PDF `/MediaBox` directly |

All three methods are available on loaded and created pages and take effect on
the next `doc.save()`.

:::note[Not yet available]
Blank-page insertion is not yet available.
:::
