---
title: API reference
description: PdfDocument, PdfPage, PdfImage, PdfFont, color helpers, and PdfForm.
---

:::tip[Full type docs]
This page is the hand-written overview. A complete, type-accurate reference is
generated from source with TypeDoc — run `bun run docs` in the repo (output in
`docs/api`).
:::

## `PdfDocument`

- `PdfDocument.load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument>` — open an existing PDF
- `PdfDocument.create(): Promise<PdfDocument>` — create a new empty document
- `doc.addPage(size: [number, number]): PdfPage` — add a page; `PageSizes.A4` etc. are `[width, height]` tuples
- `doc.getPageCount(): number`
- `doc.getPages(): PdfPage[]`
- `doc.getPage(index: number): PdfPage` — throws `PageOutOfRangeError` if out of bounds
- `doc.getFont(font: StandardFonts): PdfFont`
- `doc.embedJpg(bytes: Uint8Array): Promise<PdfImage>`
- `doc.embedPng(bytes: Uint8Array): Promise<PdfImage>`
- `doc.getForm(): PdfForm`
- `doc.createForm(): FormBuilder` — created documents only
- `doc.save(): Promise<Uint8Array>`

`save()` applies queued fills first, then queued flattens. With no queued
operations it returns a byte-identical round trip. It always starts from the
originally loaded bytes (calling it twice returns the same result), and
`FieldInfo.value` reflects queued mutations as soon as they are made.

**`PageSizes`**: `A3`, `A4`, `A5`, `Letter`, `Legal`, `Tabloid` — each a
`[width, height]` tuple in PDF points.

**`StandardFonts`** (12 standard fonts): `Helvetica`, `HelveticaBold`,
`HelveticaOblique`, `HelveticaBoldOblique`, `Courier`, `CourierBold`,
`CourierOblique`, `CourierBoldOblique`, `TimesRoman`, `TimesBold`, `TimesItalic`,
`TimesBoldItalic`. (`Symbol` and `ZapfDingbats` are intentionally omitted.)

## `PdfPage`

- `page.drawText(text, options)` — `{ x, y, size, font?, color?, lineHeight? }`
- `page.drawImage(image, options)` — `{ x, y, width?, height? }`
- `page.drawLine(options)` — `{ start: {x,y}, end: {x,y}, stroke?, strokeWidth?, opacity? }`
- `page.drawRectangle(options)` — `{ x, y, width, height, fill?, stroke?, strokeWidth?, opacity? }`
- `page.drawEllipse(options)` — `{ x, y, radiusX, radiusY, fill?, stroke?, strokeWidth?, opacity? }` (`x`,`y` = center; `radiusX`,`radiusY` = radii)

Available on both loaded pages (`doc.getPage(i)`) and created pages
(`doc.addPage(...)`).

## `PdfImage`

- `image.width: number`
- `image.height: number`
- `image.scale(factor: number): { width: number; height: number }`

## `PdfFont`

- `font.widthOfTextAtSize(text: string, size: number): number`

## Color helpers

```ts
import { rgb, grayscale } from "@ignaciano3/better-pdf";
```

- `rgb(r, g, b)` — values 0–1
- `grayscale(v)` — value 0–1

## `PdfForm`

- `form.getFields(): FieldInfo[]`
- `form.getField(name: string): FieldInfo | undefined`
- `form.getTextField(name).setText(value)`
- `form.getCheckBox(name).check()` / `.uncheck()`
- `form.getRadioGroup(name).options` / `.select(value)`
- `form.getDropdown(name).options` / `.select(value)`
- `form.getListBox(name).options` / `.select(value)`
- `form.getSignature(name).setImage(bytes)`
- `form.flattenField(name)`
- `form.flatten()`

Each `FieldInfo` carries `name`, `type`, `value`, `states`, `options`,
`readOnly`, `required`, `exported` (false when the field has the `NoExport`
flag), `maxLength` (a text field's `/MaxLen`, or `null`), and `widgets` — one
entry per widget annotation giving its 0-based `page` index and `rect`
(`[x0, y0, x1, y1]` in PDF points, origin bottom-left). `setText()` throws if its
value exceeds `maxLength`. Use `listBox.selectMultiple(values)` for multi-select
list boxes (`FieldInfo.multiSelect === true`); `listBox.select(value)` for single-select.
