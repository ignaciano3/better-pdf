---
title: Creating form fields
description: Declare fillable AcroForm fields on a generated document with a typed builder.
---

On a document created with `PdfDocument.create()`, call `doc.createForm()` to get
a chainable `FormBuilder` and declare AcroForm fields. There are six field
types — `addTextField`, `addCheckBox`, `addRadioGroup`, `addDropdown`,
`addListBox`, and `addSignatureField` — each placed by `page` index plus a
position/size in PDF points. The fields are serialized into the document on
`save()`.

```ts
import { PdfDocument, PageSizes, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
doc.addPage(PageSizes.A4);

const form = doc
  .createForm()
  .addTextField("applicant.name", {
    page: 0, x: 56, y: 740, width: 240, height: 22,
    value: "GARCIA, IGNACIO",
    maxLength: 64,
    border: { color: rgb(0.1, 0.1, 0.4), width: 1 },
    background: rgb(0.97, 0.97, 1),
  })
  .addTextField("applicant.notes", {
    page: 0, x: 56, y: 660, width: 240, height: 60, multiline: true,
  })
  .addCheckBox("applicant.agree", {
    page: 0, x: 56, y: 620, size: 14, checked: true, required: true,
  })
  .addRadioGroup("applicant.kind", {
    selected: "primary",
    options: [
      { value: "primary", page: 0, x: 56, y: 590, size: 14 },
      { value: "dependent", page: 0, x: 120, y: 590, size: 14 },
    ],
  })
  .addDropdown("applicant.status", {
    page: 0, x: 56, y: 550, width: 160, height: 22,
    options: ["single", "married"], selected: "married",
  })
  .addListBox("applicant.plan", {
    page: 0, x: 56, y: 500, width: 160, height: 48,
    options: ["basic", "plus", "premium"],
  })
  .addSignatureField("applicant.signature", {
    page: 0, x: 56, y: 440, width: 200, height: 48,
  });

console.log(form.getFieldNames()); // typed array of the declared names

const output = await doc.save();
await Bun.write("form.pdf", output);
```

:::caution[Created documents only]
`createForm()` throws on documents opened with `PdfDocument.load()`. The field
names are accumulated into the builder's type, so `getFieldNames()` is statically
typed.
:::

The result is a standard AcroForm: reload it with `PdfDocument.load(output)` and
you can fill it (`getForm().getTextField(...)`, `.getCheckBox(...).check()`, …)
and flatten it with this same library.

## Common options

Every field supports `required`, `readOnly`, `tooltip`, and the optional
appearance:

- `border` — `{ color, width? }`
- `background` — a `Color`

Colors come from `rgb(r, g, b)` and `grayscale(v)` (each 0–1).
