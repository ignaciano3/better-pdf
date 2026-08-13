---
title: File attachments
description: Embed files into a PDF (/EmbeddedFiles) and read them back — including ZUGFeRD/Factur-X e-invoices.
---

`doc.attach(bytes, name, options)` embeds an arbitrary file into a PDF's
`/EmbeddedFiles` tree. Attachments work on both created and loaded documents;
the file is **queued** and written at `save()`.

```ts
const doc = await PdfDocument.load(bytes);

doc.attach(xmlBytes, "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  afRelationship: "Alternative",
});

const saved = await doc.save();
```

Read them back later with `getAttachments()`:

```ts
const loaded = await PdfDocument.load(saved);
const attachments = await loaded.getAttachments();
// [{ name: "factur-x.xml", mimeType: "text/xml", bytes: Uint8Array, … }]
```

## Options

| Option | Type | Notes |
| --- | --- | --- |
| `name` | `string` | File name; written to `/Names/EmbeddedFiles`. Must be unique. |
| `mimeType` | `string` | Written as the embedded stream's `/Subtype` (e.g. `"text/xml"`). |
| `description` | `string` | Human-readable label, written as the filespec `/Desc`. |
| `creationDate` | `Date` | Written to `/Params /CreationDate`. Not defaulted — leave unset for deterministic output. |
| `modificationDate` | `Date` | Written to `/Params /ModDate`. Not defaulted. |
| `afRelationship` | `"Source" \| "Data" \| "Alternative" \| "Supplement" \| "EncryptedPayload" \| "FormData" \| "Schema" \| "Unspecified"` | Marks the file as an **associated file** — sets the filespec `/AFRelationship` and appends it to the catalog `/AF` array. |

`mimeType`, `description`, and the dates are optional; the embedded stream's
bytes are stored verbatim.

## ZUGFeRD / Factur-X e-invoices

`afRelationship` writes the `/AFRelationship` + catalog `/AF` structure that
ZUGFeRD/Factur-X e-invoices require. To build a compliant invoice PDF, embed the
XML data file with `afRelationship: "Alternative"` (the standard's required
value):

```ts
const doc = await PdfDocument.create();
// … draw the invoice with drawText / drawLine / drawRectangle …

const xml = new Uint8Array(await Bun.file("zugferd-invoice.xml").arrayBuffer());
doc.attach(xml, "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  afRelationship: "Alternative",
});

const output = await doc.save();
```

## Reading attachments

`getAttachments()` returns every embedded file with its metadata and raw bytes:

```ts
const attachments = await doc.getAttachments();

for (const a of attachments) {
  console.log(a.name, a.mimeType, a.size);
  if (a.name === "factur-x.xml") {
    const xml = new TextDecoder().decode(a.bytes);
  }
}
```

Each `PdfAttachment` carries `name`, `description`, `mimeType`, `creationDate`,
`modificationDate`, `afRelationship`, `size` (uncompressed bytes), and `bytes`.
Attachments queued with `attach()` but **not yet saved** are not included — call
`save()` first.

## Behavior and errors

- **Duplicate names throw.** `attach()` throws `DuplicateAttachmentError`
  when the name is already queued; a name that already exists in the loaded
  document throws at `save()` instead. See
  [Errors](/better-pdf/reference/errors/).
- **Incremental saves preserve existing attachments.** On a loaded document,
  attachments already present are kept and new ones are appended in the update
  section.
- **Queued bytes are snapshotted.** Mutating the caller's buffer after
  `attach()` does not change the embedded content.

## See also

- [`PdfDocument` API reference](/better-pdf/api-reference/classes/pdfdocument/) —
  `attach` and `getAttachments`.
- [Errors](/better-pdf/reference/errors/) — `DuplicateAttachmentError`.
