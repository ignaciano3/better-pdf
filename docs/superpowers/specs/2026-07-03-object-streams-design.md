# PDF Object Streams (Structural Compression) — Design

**Status:** Approved, ready for implementation planning.

**Goal:** Add opt-in PDF **object streams + cross-reference streams** to `better-pdf`'s
full-document save paths, packing the many small indirect objects (dictionaries,
page-tree nodes, field/annotation dicts) into compressed `/ObjStm` containers.
This is the structural-compression axis `pdf-lib` has by default and `better-pdf`
currently lacks — complementary to the content-stream deflate added earlier in
the same (still unreleased) 1.10.0 cycle. Both ship together as 1.10.0.

## Context

`better-pdf` 1.10.0 deflate-compresses the **content, appearance, and font
streams** it generates (`doc.save({ compress })`, default `true`). That covers
stream bodies but not the *object collection* — the dictionaries and indirect
objects that make up the PDF's structure.

`pdf-lib`, by contrast, defaults to `useObjectStreams: true`: it packs non-stream
indirect objects into a compressed `/Type /ObjStm` object stream plus a
cross-reference stream. Empirically (measured against the installed `pdf-lib`
1.17.1), pdf-lib compresses **both** content streams *and* the object collection.
`better-pdf` matches it on content streams as of 1.10.0 but does no object-level
packing. For object-heavy documents — large forms, many annotations, merges of
several source PDFs — that structural packing is a real size difference.

This feature closes that gap on the paths where it is structurally possible.

## Key architectural constraint (verified in lopdf 0.41 source)

Object streams are only available on **full-document** serialization:

- `Document::save_with_options(target, SaveOptions { use_object_streams, use_xref_streams, linearize, .. })`
  and `Document::save_modern` live on `impl Document` (`writer.rs:10–264`).
- `impl IncrementalDocument` (`writer.rs:265–338`) exposes only plain
  `save`/`save_to` — **no object-stream option**.
- Objects packed into an object stream are addressed by *compressed* xref entries
  (`XrefEntry::Compressed`), which can only be represented in a **cross-reference
  stream**, never a classic xref table. Object streams are therefore fundamentally
  incompatible with append-only incremental updates.

`better-pdf`'s save paths split along exactly this line:

| Path family | Rust entry | Mechanism | Object streams? |
| --- | --- | --- | --- |
| Create from scratch | `create_document_json` | full `Document` → `save_to` | **Yes** |
| Merge / assemble / copyPages / splitPages | `manipulate_pages_json` | full `Document` → `save_to` | **Yes** |
| Loaded-doc edits (fill, flatten, draw, inject, metadata, outline, insert-pages) | `apply_all_json` + per-op fns | `IncrementalDocument` (append-only) → `save_to` | **No** |

Loaded-document incremental saves are a deliberate core feature (append-only,
original bytes preserved, existing digital signatures stay valid). This feature
does **not** disturb them.

## Scope decisions (settled during brainstorming)

1. **Full-document paths only.** `create()` and the four `manipulate_pages`-backed
   operations (`merge`, `assemble`, `copyPages`, `splitPages`). Loaded-document
   incremental saves are unchanged and byte-identical to 1.10.0.
2. **Opt-in, default `false`.** With the flag off, output is byte-identical to
   today. Object streams are a more invasive structural change than the lossless
   content deflate (they force PDF ≥1.5 + xref streams, are non-conformant with
   PDF/A-1, and some older/stricter tooling handles them poorly), so they are a
   deliberate choice, not a default.
3. **One user-facing boolean.** Object streams always imply xref streams; the
   pairing is an internal detail, not two knobs.
4. **Ignored, not errored, on incremental saves.** Passing `objectStreams: true`
   to a loaded-document `save()` is a documented no-op.

## API surface

### TypeScript

Extend the existing `SaveOptions` (in `src/core/document.ts`):

```ts
export interface SaveOptions {
  compress?: boolean;        // existing (1.10.0), default true
  objectStreams?: boolean;   // new, default false
}
```

`doc.save({ objectStreams })` is honored in **create** mode and ignored for
loaded-document (incremental) saves.

A new dedicated type for the four `manipulate_pages`-backed operations (so
`compress`, which is not wired through those paths, does not appear where it does
nothing):

```ts
export interface ManipulateOptions {
  objectStreams?: boolean;   // default false
}
```

Threaded through (each `options` param defaults to `{}`):

- `PdfDocument.merge(docs, options?: ManipulateOptions)`
- `PdfDocument.assemble(docs, selections, options?: ManipulateOptions)`
- `doc.copyPages(indices, options?: ManipulateOptions)`
- `doc.splitPages(options?: ManipulateOptions)`

All four already funnel through `PdfDocumentBase.runAssemble` → `wasm.manipulatePages`,
so it is a single thread-through. `ManipulateOptions` is exported from the package
entry points alongside `SaveOptions`.

### Rust

Add a trailing `object_streams: bool` to exactly the two full-document entry
functions and their `#[wasm_bindgen]` exports:

- `create::create_document_json(..., compress, object_streams)` / `create_document`
- `pageops::manipulate_pages_json(..., compress, object_streams)` / `manipulate_pages`

The eight incremental entry functions (`apply_all_json`, `apply_draw_ops_json`,
`fill_fields_json`, `flatten_fields_json`, `inject_fields_json`, `set_outline_json`,
`set_metadata_json`, `insert_pages_json`) are **untouched** — they never see the
flag.

### WASM bindings (TS)

`src/core/wasm-bindings.ts` (`RawBindings` + `makeBindings`) and the `CoreWasm`
interface gain the trailing `objectStreams` argument on `createDocument` and
`manipulatePages` only, mirroring the 1.10.0 `compress` thread-through.

## Rust internals

Introduce a single serialization-policy helper in `crates/core/src/compress.rs`,
so content-compression + object-stream serialization has one home (the same
rationale that gave `compress_generated_streams` its own function):

```rust
/// Serialize a freshly-built full Document, applying the two output-size
/// policies: content-stream deflation (`compress`) and object-stream packing
/// (`object_streams`). Object streams always imply cross-reference streams.
pub fn serialize_document(
    doc: &mut Document,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
    if compress {
        compress_generated_streams(doc); // content/appearance/font streams (existing)
    }
    let mut out = Vec::new();
    if object_streams {
        let options = lopdf::SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .build();
        doc.save_with_options(&mut out, options)
            .map_err(|e| e.to_string())?;
    } else {
        doc.save_to(&mut out).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
```

`create.rs` and `pageops.rs` replace their existing
`if compress { compress_generated_streams(&mut doc); } … save_to(…)` blocks with
one `crate::compress::serialize_document(&mut doc, compress, object_streams)` call.
The incremental sites keep `compress_generated_streams(&mut inc.new_document);
inc.save_to(…)` unchanged.

Verified properties (lopdf 0.41 source):

- **Complementary, not redundant.** `save_with_object_streams` writes *stream
  objects* (page content) directly and packs only *non-stream* objects into
  `/ObjStm`. Content deflate and object packing act on disjoint object sets, so
  both are meaningful together. `compress: false, objectStreams: true` is valid
  (raw content bodies, packed dictionaries).
- **Version floor.** lopdf raises the document to PDF 1.5 when lower. `better-pdf`'s
  created/merged documents are already 1.7, so there is no visible version change.
- **Packing guard.** `ObjectStream::can_be_compressed` already excludes objects
  that must not be packed (existing `/ObjStm`, encryption dicts, etc.); we do not
  reimplement it.

## Testing

**Rust unit** (`create.rs`, `pageops.rs` inline `#[cfg(test)]`):

- `create_document_json(..., object_streams = true)` output is smaller than
  `false`, contains a `/ObjStm`, and round-trips via `Document::load_mem`
  (page count intact).
- `manipulate_pages_json(..., object_streams = true)` on a merge of two fixtures
  is smaller than `false` and round-trips — the object-heavy case where the win
  is largest.
- `(compress, object_streams)` ∈ {(f,f), (t,f), (f,t), (t,t)} all produce loadable
  PDFs.

**TS integration** (extend `tests/compression.test.ts`):

- `save({ objectStreams: true })` on a created doc is smaller than the default;
  both are valid `%PDF-`; the result reloads via `PdfDocument.load`.
- `PdfDocument.merge([a, b], { objectStreams: true })` is smaller than
  `merge([a, b])`, and the result **loads and re-edits cleanly** — a round-trip
  through `better-pdf`'s own parser, since lopdf must read back the object/xref
  streams it wrote.
- `objectStreams: true` on a **loaded-document** `save()` is a no-op (documents
  the ignored-flag behavior).

## Documentation & versioning

- **README**: Features bullet + expand the Compression section with the
  `objectStreams` opt-in note (created/merged documents only).
- **`docs/site` `guides/generating.mdx` + `reference/api.md`**: document the new
  option and the full-doc-only scope. The generated API reference and the
  changelog page regenerate automatically on site build.
- **CHANGELOG**: **extend the existing (unreleased) 1.10.0 entry** — object
  streams ship in the same release as content-stream compression, not a separate
  version. Add the caveats there: applies to create/merge/assemble/copyPages/splitPages
  only; forces PDF ≥1.5 + cross-reference streams; output is not PDF/A-1 conformant;
  incremental (loaded-document) saves are unaffected.
- **Version**: **no bump** — stays at **1.10.0**. 1.10.0 is committed but not yet
  published, so this feature folds into that same release alongside content-stream
  compression. `package.json` and `crates/core/Cargo.toml` remain `1.10.0`.

## Out of scope

- **Linearization** (`SaveOptions.linearize`) — separate concern, not wired.
- **Object streams on incremental/loaded-document saves** — structurally
  impossible without abandoning append-only; explicitly excluded (see constraint).
- **Changing the default** — remains opt-in; revisiting the default is a future
  decision, not part of this feature.
- **PDF/A conformance modes** — unrelated.
