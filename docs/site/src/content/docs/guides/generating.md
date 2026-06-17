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
