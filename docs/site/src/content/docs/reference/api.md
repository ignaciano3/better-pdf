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

- `PdfDocument.load(input: Uint8Array | ArrayBuffer, options?: { password?: string }): Promise<PdfDocument>` — open an existing PDF; pass `{ password }` to decrypt an encrypted PDF (use `""` for owner-locked / empty-user-password files)
- `PdfDocument.isEncrypted(input: Uint8Array | ArrayBuffer): Promise<boolean>` — report whether a PDF is encrypted, without decrypting or needing a password
- `PdfDocument.passwordType(input: Uint8Array | ArrayBuffer, password: string): Promise<"owner" | "user" | null>` — classify how a password authorizes an encrypted PDF (`"owner"` = full access, `"user"` = restricted); `null` when it authenticates neither role or the file is not an encrypted classic-`trailer` PDF (xref-stream encrypted files return `null`)
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
- `doc.save(options?: SaveOptions): Promise<Uint8Array>` — `SaveOptions = { compress?: boolean; objectStreams?: boolean }`; `compress` defaults to `true`, `objectStreams` to `false` (full-document/created saves only)

`save()` applies queued fills first, then queued flattens. With no queued
operations it returns a byte-identical round trip. It always starts from the
originally loaded bytes (calling it twice returns the same result), and
`FieldInfo.value` reflects queued mutations as soon as they are made.

By default `save()` deflate-compresses the content, appearance, and font streams
it generates, producing smaller PDFs. Pass `{ compress: false }` for plaintext
output (e.g. debugging or byte-level assertions). Already-compressed streams
(images, embedded fonts) are left untouched, and incremental saves only compress
the newly appended section, so existing signatures on the original revision stay
valid.

`doc.save({ objectStreams: true })` additionally packs non-stream objects into
PDF object streams (+ cross-reference streams) for smaller output. It defaults
to `false` and only applies to created documents saved via `save()`; it is
ignored on loaded-document (incremental) saves.

The full-document assembly operations — `PdfDocument.merge(docs, options?)`,
`PdfDocument.assemble(...)`, `doc.copyPages(indices, options?)`, and
`doc.splitPages(options?)` — accept an optional `ManipulateOptions = {
objectStreams?: boolean }` as their trailing argument, with the same
`objectStreams` semantics as `SaveOptions` (defaults to `false`).

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
- `form.getTextField(name).setText(value)` / `.setDefaultText(value)`
- `form.getCheckBox(name).check()` / `.uncheck()` / `.setDefaultChecked(checked)`
- `form.getRadioGroup(name).options` / `.select(value)` / `.setDefaultSelected(value)`
- `form.getDropdown(name).options` / `.select(value)` / `.setDefaultSelected(value)`
- `form.getListBox(name).options` / `.select(value)` / `.setDefaultSelected(value)`
- `form.getSignature(name).setImage(bytes)`
- `form.flattenField(name)` / `form.flatten()`
- `form.resetField(name)` / `form.reset()`

Each `FieldInfo` carries `name`, `type`, `value`, `defaultValue` (the `/DV`
reset value, or `null`), `states`, `options`, `readOnly`, `required`, `exported`
(false when the field has the `NoExport` flag), `maxLength` (a text field's
`/MaxLen`, or `null`), `multiline` (text-area fields), `password` (masked text
fields), `comb` (fixed-pitch per-character text fields), `editable` (combo boxes
that accept custom values), `align` (`"left"`/`"center"`/`"right"`, from `/Q`),
`tooltip` (the `/TU` descriptive name, or `null`), `fontName` / `fontSize` (the
effective `/DA` font resource name and size for variable-text fields, else
`null`), `multiSelect` (multi-select list boxes), and `widgets` — one entry per
widget annotation giving its 0-based `page` index, `rect` (`[x0, y0, x1, y1]`
in PDF points, origin bottom-left), and the annotation visibility flags
`hidden` / `print` / `noView` (from `/F`).
`setText()` throws if its value exceeds `maxLength`. Use
`listBox.selectMultiple(values)` for multi-select list boxes
(`FieldInfo.multiSelect === true`); `listBox.select(value)` for single-select.
