# File attachments (/EmbeddedFiles, ZUGFeRD/Factur-X structure)

**Date:** 2026-07-05
**Status:** Approved design, pending implementation plan
**Ships as:** 1.11.0 (or 1.12.0 if it slips behind embedded-font fill)

## Problem

The library has no `/EmbeddedFiles` support at all — no write, no read. pdf-lib has
`doc.attach()`. The biggest real-world driver is e-invoicing (ZUGFeRD/Factur-X:
embedded XML with `/AFRelationship` and a catalog `/AF` array).

## Scope

- **In:** attach files on created and loaded documents; read/extract attachments
  (metadata + bytes) from loaded documents; `/AFRelationship` + catalog `/AF`.
- **Out:** `removeAttachment` / `replaceAttachment` (later, on demand); lazy-bytes
  reads; PDF/A-3 XMP conformance metadata (XMP is a known library-wide gap — README
  will state "ZUGFeRD/Factur-X structure supported; PDF/A-3 conformance metadata is
  your responsibility").

## API (TypeScript)

```ts
doc.attach(xmlBytes, "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  creationDate: new Date(...),      // optional; NOT defaulted (WASM has no clock; determinism)
  modificationDate: new Date(...),
  afRelationship: "Alternative",    // 'Source'|'Data'|'Alternative'|'Supplement'|
                                    // 'EncryptedPayload'|'FormData'|'Schema'|'Unspecified'
});

const list: PdfAttachment[] = await doc.getAttachments();
// { name, description?, mimeType?, creationDate?, modificationDate?,
//   size, afRelationship?, bytes: Uint8Array }
```

- `attach()` is synchronous and queues; the write happens at `save()` in the batched
  pipeline. Zero cost on the load→fill→save hot path when unused.
- **Duplicate names throw `DuplicateAttachmentError`** — at `attach()` time for
  queued duplicates, at save time against the loaded document's name tree. No silent
  replace.
- `getAttachments()` returns metadata **and** bytes in one call and reads the
  *saved* document state — queued-but-unsaved attachments are not included
  (consistent with how `getForm()` reads relate to queued fills).
- Filenames written to both `/F` (ASCII-safe fallback) and `/UF` (UTF-16BE full
  name); reads prefer `/UF`.
- New errors: `DuplicateAttachmentError` (ships now), `AttachmentNotFoundError`
  (reserved for later get-by-name/remove APIs).

## Rust core (`crates/core/src/attach.rs`)

New module invoked from the `apply_all` pipeline (plus a standalone path so attach
works with no other queued ops). Bytes arrive via the existing blob channel by
offset/length. Per attachment, written as **appended objects** (incremental-save
friendly):

1. `/EmbeddedFile` stream — FlateDecode-compressed; `/Subtype` from MIME
   (name-encoded, e.g. `text#2Fxml`); `/Params` with `/Size` (uncompressed),
   optional `/CreationDate` / `/ModDate`, and `/CheckSum` (MD5 of the file bytes,
   per spec convention).
2. Filespec dict — `/Type /Filespec`, `/F` + `/UF`, `/Desc`,
   `/EF << /F <stream> /UF <stream> >>`, optional `/AFRelationship`.

**Name tree merge:** no `/Names` in the catalog → create
`/Names << /EmbeddedFiles << /Names [...] >> >>`. Existing tree → read all entries
(walking `/Kids` recursively), merge new names in **lexicographic order** (name
trees must be sorted — spec trap), write a new flat root node. Old nodes become dead
objects, which incremental save tolerates. Re-nesting for huge trees is YAGNI.

**`/AF` array:** any attachment with `afRelationship` appends its filespec ref to
the catalog `/AF` (created if absent, existing entries preserved).

**Reader** (`read_attachments` WASM fn): walks `/Names/EmbeddedFiles` including
`/Kids`, decodes filespecs (prefer `/UF`), decompresses `/EF /F` (fallback `/UF`
stream), returns JSON metadata + bytes via the shared binary channel. Missing
optional keys tolerated; a filespec without `/EF` is skipped, not fatal.

Duplicate detection at save compares against the merged existing-name set (exact
string match, `/UF`-preferred).

## Testing

- Attach on created and loaded docs; round-trip via `getAttachments()` (bytes, MIME,
  dates, description, checksum).
- Merge into a fixture that already has attachments in a `/Kids` name tree —
  existing entries preserved, sort order correct.
- `afRelationship` → filespec `/AFRelationship` + catalog `/AF`; one manual
  validation of a Factur-X-shaped fixture with an external tool (veraPDF/mustang).
- Duplicate name throws; unicode filename via `/UF`; attach + fill + flatten
  coexisting in one save.
- Benchmark: no measurable hot-path change when no attachments are queued.

## Docs

README features + limitations (remove "no attachments"; add the PDF/A-3 metadata
caveat), `docs/migrating-from-pdf-lib.md` gains the `attach()` mapping, CHANGELOG
entry.
