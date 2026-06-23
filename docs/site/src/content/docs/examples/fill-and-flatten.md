---
title: Fill & flatten a form
description: Load an AcroForm PDF, set text and checkbox fields, then flatten to a static document.
---

Load an existing AcroForm PDF, fill its fields, and flatten so the values are
baked into the page content (no longer editable). Appearances are generated on
fill, so flattening works even on PDFs where pdf-lib leaves blank fields.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

// 1. Load the source PDF (any Uint8Array — fs, fetch, upload, …)
const input = await Bun.file("application.pdf").bytes();
const doc = await PdfDocument.load(input);
const form = doc.getForm();

// 2. Inspect what's there (optional)
console.log(form.getFieldNames());

// 3. Fill fields
form.getTextField("name").setText("GARCIA, IGNACIO");
form.getTextField("date").setText("2026-06-23");
form.getCheckBox("agree").check();   // uses the field's real on-state

// 4. Flatten — bake values into static content
form.flatten();                      // all fields; or form.flattenField("name")

// 5. Save
const output = await doc.save();      // Promise<Uint8Array>
await Bun.write("application.filled.pdf", output);
```

`save()` applies queued fills first, then queued flattens. Drop the
`form.flatten()` call to keep the output editable.

See the [Filling & flattening forms](/guides/filling-forms/) guide for the full
field API (radio groups, dropdowns, list boxes, max-length handling).
