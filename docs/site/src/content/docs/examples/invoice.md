---
title: Generate an invoice
description: Create a PDF document from scratch — text, rules, and a simple line-item layout.
---

Build a one-page invoice with no source document. Shows `PdfDocument.create`,
`addPage`, standard-14 fonts, `drawText`, and `drawLine` for layout rules.

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);          // [595, 842] pt, origin bottom-left
const [W, H] = PageSizes.A4;

const bold = doc.getFont(StandardFonts.HelveticaBold);
const body = doc.getFont(StandardFonts.Helvetica);
const ink = rgb(0.1, 0.1, 0.12);
const muted = rgb(0.45, 0.45, 0.5);

// Header
page.drawText("INVOICE", { x: 40, y: H - 64, size: 28, font: bold, color: ink });
page.drawText("#2026-0042", { x: 40, y: H - 86, size: 11, font: body, color: muted });
page.drawText("Due 2026-07-23", { x: W - 160, y: H - 86, size: 11, font: body, color: muted });

// Table header + rule
let y = H - 150;
page.drawText("Description", { x: 40, y, size: 11, font: bold, color: ink });
page.drawText("Amount", { x: W - 120, y, size: 11, font: bold, color: ink });
page.drawLine({ start: { x: 40, y: y - 8 }, end: { x: W - 40, y: y - 8 }, strokeWidth: 1, stroke: muted });

// Line items
const items = [
  ["PDF tooling — June retainer", "$4,000.00"],
  ["Form automation add-on", "$1,200.00"],
  ["Support hours (6 @ $150)", "$900.00"],
];
y -= 32;
for (const [desc, amount] of items) {
  page.drawText(desc, { x: 40, y, size: 11, font: body, color: ink });
  page.drawText(amount, { x: W - 120, y, size: 11, font: body, color: ink });
  y -= 24;
}

// Total
page.drawLine({ start: { x: 40, y: y - 4 }, end: { x: W - 40, y: y - 4 }, strokeWidth: 1, stroke: muted });
page.drawText("Total", { x: 40, y: y - 28, size: 13, font: bold, color: ink });
page.drawText("$6,100.00", { x: W - 120, y: y - 28, size: 13, font: bold, color: ink });

const output = await doc.save();
await Bun.write("invoice.pdf", output);
```

Standard-14 fonts cover WinAnsi only. For non-Latin text (CJK, accented Latin
beyond WinAnsi), embed a TTF/OTF with `doc.embedFont(bytes)`. See
[Generating & drawing](/guides/generating/) for images, rectangles, ellipses,
SVG paths, and text measurement.
