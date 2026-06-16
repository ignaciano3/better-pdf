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

:::caution[Fonts]
Standard-14 only (Helvetica, HelveticaBold, Courier, TimesRoman, …). Custom font
embedding is not yet supported. Character set is WinAnsi — accented Latin
characters work; CJK does not.
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
