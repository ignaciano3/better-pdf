---
title: Embed a file attachment
description: Attach an XML data file to a PDF and read it back — the Factur-X/ZUGFeRD e-invoice pattern.
---

Attach a file (an XML data sheet, a scan, a manifest) to a PDF, then read it
back after save. Works on created and loaded documents.

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

// 1. Load a document (or create one with PdfDocument.create())
const bytes = new Uint8Array(await Bun.file("invoice.pdf").arrayBuffer());
const doc = await PdfDocument.load(bytes);

// 2. Attach a file — queued and written at save()
const xml = new Uint8Array(await Bun.file("factur-x.xml").arrayBuffer());
doc.attach(xml, "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  afRelationship: "Alternative",   // marks it as an associated file (/AF)
});

// 3. Save, reload, and read the attachment back
const output = await doc.save();
await Bun.write("invoice-factur-x.pdf", output);

const reloaded = await PdfDocument.load(output);
const attachments = await reloaded.getAttachments();

for (const a of attachments) {
  console.log(a.name, a.mimeType, a.size);
}
```

A second `attach()` call with the same `name` throws `DuplicateAttachmentError`.

See the [File attachments](/guides/attachments/) guide for the full option set
and the ZUGFeRD/Factur-X details.
