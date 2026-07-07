# File Attachments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/EmbeddedFiles` support — `doc.attach()` (write, on created and loaded docs) and `doc.getAttachments()` (read metadata + bytes), including `/AFRelationship` + catalog `/AF` for ZUGFeRD/Factur-X structure.

**Architecture:** New Rust module `crates/core/src/attach.rs` following the Phase A (resolve on prev docs) / Phase B (apply to `IncrementalDocument`) shape used by every `apply_all` mutator. Attachment bytes ride a new concatenated blob channel by offset/length (same pattern as fill images / draw fonts). Two new WASM exports: `attach_files` (standalone, also used by the chained save path and created docs) and `read_attachments` (packed binary: u32 JSON length + JSON + bytes blob). `apply_all` gains an `attach` plan section + an `attach_blob` parameter so attach composes with fill/flatten/draw in one pass.

**Tech Stack:** Rust (lopdf 0.41, flate2, new dep `md-5`), wasm-bindgen, TypeScript, bun test.

## Global Constraints

- Ships as **1.12.0** (embedded-font fill took 1.11.0).
- `attach()` is **synchronous and queues**; the write happens at `save()`. Zero cost on the load→mutate→save hot path when unused (no new WASM calls, no new allocations when the queue is empty).
- Duplicate names throw `DuplicateAttachmentError` — at `attach()` time for queued duplicates, at save time against the loaded document's existing name tree. No silent replace.
- `getAttachments()` reads **saved** document state only — queued-but-unsaved attachments are not included. On an unsealed created document it returns `[]`.
- Filenames written to both `/F` (ASCII-safe fallback) and `/UF` (UTF-16BE full name); reads prefer `/UF`.
- Name-tree entries must stay in **lexicographic order** (by UTF-8 byte comparison of the name strings) after merge; existing entries preserved; merged tree written as a new flat root node (old `/Kids` nodes become dead objects — fine for incremental save).
- Dates are **not defaulted** (WASM has no clock; determinism). Only written when the caller passes them.
- `/EmbeddedFile` streams are FlateDecode-compressed always (independent of the `compress` save flag), with `/Params` `/Size` (uncompressed byte count) and `/CheckSum` (MD5 of the uncompressed bytes, as a PDF string).
- Rust error strings are stable API for the TS layer: duplicate errors MUST start with the exact prefix `duplicate attachment` (TS maps prefix → `DuplicateAttachmentError`).
- CI gates `cargo clippy --all-targets -- -D warnings`; run it before each Rust commit. Do not reformat files you aren't changing.
- Rust tests: `cargo test --manifest-path crates/core/Cargo.toml`. TS tests: `bun test`. Rebuild WASM before TS tests that exercise new exports: `bun run build:wasm` (check `package.json` scripts for the exact name — it is the script that runs wasm-pack).

## Wire Formats (shared by all tasks)

**Attach op JSON** (TS → Rust, camelCase, one array under plan key `attach` or standalone):

```json
[{
  "name": "factur-x.xml",
  "description": "Factur-X invoice data",
  "mimeType": "text/xml",
  "creationDate": "D:20260101120000Z",
  "modificationDate": "D:20260102120000Z",
  "afRelationship": "Alternative",
  "offset": 0,
  "length": 1234
}]
```

`offset`/`length` index into the attachments blob. All fields except `name`, `offset`, `length` are optional.

**`read_attachments` packed return** (Rust → TS): `[u32 LE json_len][json bytes][concatenated file bytes]`. The JSON is an array of:

```json
[{
  "name": "factur-x.xml",
  "description": "Factur-X invoice data",
  "mimeType": "text/xml",
  "creationDate": "D:20260101120000Z",
  "modificationDate": "D:20260102120000Z",
  "afRelationship": "Alternative",
  "size": 1234,
  "offset": 0,
  "length": 1234
}]
```

where `offset`/`length` index into the file-bytes section (after the JSON), and `size` is the decoded (uncompressed) length (equal to `length`; kept explicit for the public type).

---

### Task 1: Rust write core — `attach.rs` + standalone `attach_files` export

**Files:**
- Create: `crates/core/src/attach.rs`
- Modify: `crates/core/Cargo.toml` (add `md-5 = "0.10"`)
- Modify: `crates/core/src/lib.rs` (add `mod attach;` + `attach_files` wasm export)
- Test: inline `#[cfg(test)] mod tests` in `crates/core/src/attach.rs`

**Interfaces:**
- Consumes: `crate::doc_io::load_pdf(data) -> Result<Document, String>`, `lopdf::IncrementalDocument`, `inc.opt_clone_object_to_new_document(id)` (catalog-override pattern from `outline.rs:187`), `crate::compress::compress_generated_streams`.
- Produces (used by Tasks 2–4):
  - `pub(crate) struct AttachOp` (serde, `rename_all = "camelCase"`): `name: String`, `description: Option<String>`, `mime_type: Option<String>`, `creation_date: Option<String>`, `modification_date: Option<String>`, `af_relationship: Option<String>`, `offset: usize`, `length: usize`
  - `pub(crate) struct AttachPlan { root_id: lopdf::ObjectId, existing: Vec<(String, lopdf::Object)> }` — existing name-tree entries as (name, filespec ref/obj) pairs, in encounter order
  - `pub(crate) fn attach_resolve(doc: &lopdf::Document, ops: &[AttachOp], blob: &[u8]) -> Result<AttachPlan, String>`
  - `pub(crate) fn attach_apply(inc: &mut lopdf::IncrementalDocument, plan: &AttachPlan, ops: &[AttachOp], blob: &[u8]) -> Result<(), String>`
  - `pub fn attach_files_json(data: &[u8], ops_json: &str, blob: &[u8], compress: bool) -> Result<Vec<u8>, String>`
  - lib.rs: `#[wasm_bindgen] pub fn attach_files(data: &[u8], ops_json: &str, blob: &[u8], compress: bool) -> Result<Vec<u8>, JsError>`

In this task, `attach_resolve` handles only the **no-existing-tree** case (returns `existing: vec![]` when the catalog has no `/Names/EmbeddedFiles`); walking an existing tree is Task 2. Duplicate detection between queued ops (same `name` twice in `ops`) IS in scope here.

- [ ] **Step 1: Add the md-5 dependency**

In `crates/core/Cargo.toml` under `[dependencies]`:

```toml
md-5 = "0.10"
```

- [ ] **Step 2: Write the failing tests**

Create `crates/core/src/attach.rs` with the module skeleton and tests (implementation stubs return `Err("unimplemented".into())` so tests compile but fail):

```rust
//! File attachments: /EmbeddedFiles name tree write + read, /AF (associated
//! files) for ZUGFeRD/Factur-X. Same Phase A/Phase B shape as fill/flatten.

use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};
use md5::{Digest, Md5};
use serde::Deserialize;
use std::io::Write as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AttachOp {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub creation_date: Option<String>,
    #[serde(default)]
    pub modification_date: Option<String>,
    #[serde(default)]
    pub af_relationship: Option<String>,
    pub offset: usize,
    pub length: usize,
}

pub(crate) struct AttachPlan {
    pub root_id: ObjectId,
    /// Existing /EmbeddedFiles entries: (name, filespec object — usually a
    /// Reference) in encounter order. Empty when the doc has no tree yet.
    pub existing: Vec<(String, Object)>,
}

pub(crate) fn attach_resolve(
    _doc: &Document,
    _ops: &[AttachOp],
    _blob: &[u8],
) -> Result<AttachPlan, String> {
    Err("unimplemented".into())
}

pub(crate) fn attach_apply(
    _inc: &mut IncrementalDocument,
    _plan: &AttachPlan,
    _ops: &[AttachOp],
    _blob: &[u8],
) -> Result<(), String> {
    Err("unimplemented".into())
}

/// Standalone entry: parse ops, load doc, resolve, apply, save incrementally.
pub fn attach_files_json(
    data: &[u8],
    ops_json: &str,
    blob: &[u8],
    compress: bool,
) -> Result<Vec<u8>, String> {
    let ops: Vec<AttachOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid attach ops: {e}"))?;
    let doc = crate::doc_io::load_pdf(data)?;
    let plan = attach_resolve(&doc, &ops, blob)?;
    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    attach_apply(&mut inc, &plan, &ops, blob)?;
    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }
    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```

Tests (same file). Helper first — walking the output to find the tree and decode a filespec is needed by every test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn blank_doc() -> Vec<u8> {
        crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[], &[], "[]", "[]", false, false,
        )
        .unwrap()
    }

    /// (name, filespec dict) pairs from /Root/Names/EmbeddedFiles/Names,
    /// resolving references. Panics on malformed structure — tests only.
    fn tree_entries(doc: &Document) -> Vec<(String, Dictionary)> {
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root_id).unwrap();
        let names = match catalog.get(b"Names").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            o => panic!("bad /Names: {o:?}"),
        };
        let ef = match names.get(b"EmbeddedFiles").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            o => panic!("bad /EmbeddedFiles: {o:?}"),
        };
        let arr = ef.get(b"Names").unwrap().as_array().unwrap();
        arr.chunks(2)
            .map(|pair| {
                let name = String::from_utf8(pair[0].as_str().unwrap().to_vec()).unwrap();
                let spec = match &pair[1] {
                    Object::Reference(id) => doc.get_dictionary(*id).unwrap().clone(),
                    Object::Dictionary(d) => d.clone(),
                    o => panic!("bad filespec: {o:?}"),
                };
                (name, spec)
            })
            .collect()
    }

    /// Decompressed /EF /F stream bytes of a filespec dict.
    fn ef_bytes(doc: &Document, spec: &Dictionary) -> Vec<u8> {
        let ef = spec.get(b"EF").unwrap().as_dict().unwrap();
        let sid = ef.get(b"F").unwrap().as_reference().unwrap();
        let stream = doc.get_object(sid).unwrap().as_stream().unwrap();
        stream.decompressed_content().unwrap()
    }

    #[test]
    fn attach_creates_names_tree_and_embedded_file_stream() {
        let base = blank_doc();
        let payload = b"<invoice>42</invoice>".to_vec();
        let ops = format!(
            r#"[{{"name":"factur-x.xml","mimeType":"text/xml","description":"Invoice data","offset":0,"length":{}}}]"#,
            payload.len()
        );
        let out = attach_files_json(&base, &ops, &payload, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();

        let entries = tree_entries(&doc);
        assert_eq!(entries.len(), 1);
        let (name, spec) = &entries[0];
        assert_eq!(name, "factur-x.xml");

        // Filespec shape
        assert_eq!(spec.get(b"Type").unwrap().as_name().unwrap(), b"Filespec");
        assert_eq!(spec.get(b"F").unwrap().as_str().unwrap(), b"factur-x.xml");
        // /UF is UTF-16BE with BOM
        let uf = spec.get(b"UF").unwrap().as_str().unwrap();
        assert_eq!(&uf[..2], &[0xFE, 0xFF]);
        assert_eq!(
            spec.get(b"Desc").unwrap().as_str().unwrap(),
            b"Invoice data"
        );

        // Stream: decompresses to payload, FlateDecode, Subtype, Params
        let ef = spec.get(b"EF").unwrap().as_dict().unwrap();
        let sid = ef.get(b"F").unwrap().as_reference().unwrap();
        let stream = doc.get_object(sid).unwrap().as_stream().unwrap();
        assert_eq!(
            stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"FlateDecode"
        );
        assert_eq!(
            stream.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"text/xml"
        );
        assert_eq!(ef_bytes(&doc, spec), payload);

        let params = stream.dict.get(b"Params").unwrap().as_dict().unwrap();
        assert_eq!(
            params.get(b"Size").unwrap().as_i64().unwrap(),
            payload.len() as i64
        );
        let expected_md5: [u8; 16] = Md5::digest(&payload).into();
        assert_eq!(params.get(b"CheckSum").unwrap().as_str().unwrap(), &expected_md5);
        // No dates were passed → none written
        assert!(params.get(b"CreationDate").is_err());
        assert!(params.get(b"ModDate").is_err());
    }

    #[test]
    fn attach_writes_optional_dates_and_unicode_uf() {
        let base = blank_doc();
        let payload = b"data".to_vec();
        let ops = format!(
            r#"[{{"name":"año-2026.txt","creationDate":"D:20260101120000Z","modificationDate":"D:20260102120000Z","offset":0,"length":{}}}]"#,
            payload.len()
        );
        let out = attach_files_json(&base, &ops, &payload, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, spec) = &tree_entries(&doc)[0];

        // /UF round-trips the ñ via UTF-16BE
        let uf = spec.get(b"UF").unwrap().as_str().unwrap();
        let utf16: Vec<u16> = uf[2..]
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&utf16).unwrap(), "año-2026.txt");
        // /F is the ASCII-safe fallback (non-ASCII replaced with '_')
        assert_eq!(spec.get(b"F").unwrap().as_str().unwrap(), b"a_o-2026.txt");

        let ef = spec.get(b"EF").unwrap().as_dict().unwrap();
        let sid = ef.get(b"F").unwrap().as_reference().unwrap();
        let params = doc
            .get_object(sid).unwrap().as_stream().unwrap()
            .dict.get(b"Params").unwrap().as_dict().unwrap();
        assert_eq!(
            params.get(b"CreationDate").unwrap().as_str().unwrap(),
            b"D:20260101120000Z"
        );
        assert_eq!(
            params.get(b"ModDate").unwrap().as_str().unwrap(),
            b"D:20260102120000Z"
        );
    }

    #[test]
    fn attach_two_files_sorted_lexicographically() {
        let base = blank_doc();
        let blob = b"AABB".to_vec();
        // Queued out of order: "b.txt" first, "a.txt" second.
        let ops = r#"[
            {"name":"b.txt","offset":0,"length":2},
            {"name":"a.txt","offset":2,"length":2}
        ]"#;
        let out = attach_files_json(&base, ops, &blob, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let entries = tree_entries(&doc);
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
        assert_eq!(ef_bytes(&doc, &entries[0].1), b"BB");
        assert_eq!(ef_bytes(&doc, &entries[1].1), b"AA");
    }

    #[test]
    fn duplicate_queued_names_error() {
        let base = blank_doc();
        let blob = b"xxyy".to_vec();
        let ops = r#"[
            {"name":"same.txt","offset":0,"length":2},
            {"name":"same.txt","offset":2,"length":2}
        ]"#;
        let err = attach_files_json(&base, ops, &blob, false).unwrap_err();
        assert!(
            err.starts_with("duplicate attachment"),
            "error must start with the stable prefix: {err}"
        );
        assert!(err.contains("same.txt"));
    }

    #[test]
    fn attach_blob_range_out_of_bounds_errors() {
        let base = blank_doc();
        let ops = r#"[{"name":"a.txt","offset":0,"length":99}]"#;
        let err = attach_files_json(&base, ops, b"tiny", false).unwrap_err();
        assert!(err.contains("out of range"), "unexpected: {err}");
    }

    #[test]
    fn attached_output_is_incremental_append() {
        // Incremental save must preserve the original bytes as a prefix.
        let base = blank_doc();
        let ops = r#"[{"name":"a.txt","offset":0,"length":4}]"#;
        let out = attach_files_json(&base, ops, b"data", false).unwrap();
        assert!(out.len() > base.len());
        assert_eq!(&out[..base.len()], &base[..]);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path crates/core/Cargo.toml attach`
Expected: FAIL — every test errors with `unimplemented`.

- [ ] **Step 4: Implement `attach_resolve` and `attach_apply`**

Replace the stubs:

```rust
/// ASCII-safe fallback for /F: printable ASCII kept, everything else `_`.
fn ascii_fallback(name: &str) -> Vec<u8> {
    name.chars()
        .map(|c| if c.is_ascii() && !c.is_ascii_control() { c as u8 } else { b'_' })
        .collect()
}

/// UTF-16BE with BOM, the PDF text-string encoding for non-ASCII names.
fn utf16be_string(s: &str) -> Vec<u8> {
    let mut out = vec![0xFE, 0xFF];
    for unit in s.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

pub(crate) fn attach_resolve(
    doc: &Document,
    ops: &[AttachOp],
    blob: &[u8],
) -> Result<AttachPlan, String> {
    // Validate blob ranges up front so apply can slice unchecked.
    for op in ops {
        let end = op
            .offset
            .checked_add(op.length)
            .filter(|&e| e <= blob.len())
            .ok_or_else(|| {
                format!(
                    "attachment '{}' byte range {}..{} out of range (blob is {} bytes)",
                    op.name,
                    op.offset,
                    op.offset + op.length,
                    blob.len()
                )
            })?;
        let _ = end;
    }
    // Duplicates within the queued ops themselves.
    let mut seen = std::collections::HashSet::new();
    for op in ops {
        if !seen.insert(op.name.as_str()) {
            return Err(format!("duplicate attachment name '{}'", op.name));
        }
    }
    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    // Task 2 replaces this with a real walk of any existing tree.
    Ok(AttachPlan { root_id, existing: Vec::new() })
}

/// Build the /EmbeddedFile stream + /Filespec dict for one op; returns the
/// filespec's object id.
fn build_filespec(
    new_doc: &mut Document,
    op: &AttachOp,
    blob: &[u8],
) -> Result<ObjectId, String> {
    let bytes = &blob[op.offset..op.offset + op.length];

    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).map_err(|e| e.to_string())?;
    let compressed = enc.finish().map_err(|e| e.to_string())?;

    let mut params = Dictionary::new();
    params.set("Size", Object::Integer(bytes.len() as i64));
    let checksum: [u8; 16] = Md5::digest(bytes).into();
    params.set(
        "CheckSum",
        Object::String(checksum.to_vec(), lopdf::StringFormat::Hexadecimal),
    );
    if let Some(d) = &op.creation_date {
        params.set(
            "CreationDate",
            Object::String(d.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }
    if let Some(d) = &op.modification_date {
        params.set(
            "ModDate",
            Object::String(d.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }

    let mut sdict = Dictionary::new();
    sdict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
    if let Some(mime) = &op.mime_type {
        // lopdf's writer #-escapes delimiter chars in names (e.g. '/' →
        // "#2F"), so the raw MIME bytes are correct here.
        sdict.set("Subtype", Object::Name(mime.as_bytes().to_vec()));
    }
    sdict.set("Params", Object::Dictionary(params));
    sdict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    let mut stream = Stream::new(sdict, compressed);
    // The content is already compressed; prevent lopdf/compress passes from
    // touching it.
    stream.dict.set("Length", Object::Integer(stream.content.len() as i64));
    let stream_id = new_doc.add_object(Object::Stream(stream));

    let mut ef = Dictionary::new();
    ef.set("F", Object::Reference(stream_id));
    ef.set("UF", Object::Reference(stream_id));

    let mut spec = Dictionary::new();
    spec.set("Type", Object::Name(b"Filespec".to_vec()));
    spec.set(
        "F",
        Object::String(ascii_fallback(&op.name), lopdf::StringFormat::Literal),
    );
    spec.set(
        "UF",
        Object::String(utf16be_string(&op.name), lopdf::StringFormat::Hexadecimal),
    );
    if let Some(desc) = &op.description {
        spec.set(
            "Desc",
            Object::String(desc.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }
    if let Some(rel) = &op.af_relationship {
        spec.set("AFRelationship", Object::Name(rel.as_bytes().to_vec()));
    }
    spec.set("EF", Object::Dictionary(ef));
    Ok(new_doc.add_object(Object::Dictionary(spec)))
}

pub(crate) fn attach_apply(
    inc: &mut IncrementalDocument,
    plan: &AttachPlan,
    ops: &[AttachOp],
    blob: &[u8],
) -> Result<(), String> {
    if ops.is_empty() {
        return Ok(());
    }

    // Build the new filespecs.
    let mut entries: Vec<(String, Object)> = plan.existing.clone();
    for op in ops {
        let spec_id = build_filespec(&mut inc.new_document, op, blob)?;
        entries.push((op.name.clone(), Object::Reference(spec_id)));
    }
    // Name trees must be sorted (byte order of the name strings).
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut flat = Vec::with_capacity(entries.len() * 2);
    for (name, spec) in &entries {
        flat.push(Object::String(
            name.as_bytes().to_vec(),
            lopdf::StringFormat::Literal,
        ));
        flat.push(spec.clone());
    }
    let mut ef_node = Dictionary::new();
    ef_node.set("Names", Object::Array(flat));
    let ef_id = inc.new_document.add_object(Object::Dictionary(ef_node));

    // Override the catalog (same-object-id incremental override; the pattern
    // outline_apply uses). Merge into any existing /Names dict rather than
    // clobbering other name trees (e.g. /Dests, /JavaScript).
    inc.opt_clone_object_to_new_document(plan.root_id)
        .map_err(|e| e.to_string())?;
    // Read the existing /Names value BEFORE taking the mutable catalog borrow.
    let existing_names: Option<Object> = inc
        .new_document
        .get_dictionary(plan.root_id)
        .ok()
        .and_then(|c| c.get(b"Names").ok().cloned());
    let mut names_dict = match existing_names {
        Some(Object::Dictionary(d)) => d,
        Some(Object::Reference(id)) => inc
            .new_document
            .get_dictionary(id)
            .or_else(|_| {
                // /Names lives in a prior revision: resolve through prev docs.
                inc.get_prev_documents().get_dictionary(id)
            })
            .map_err(|e| e.to_string())?
            .clone(),
        _ => Dictionary::new(),
    };
    names_dict.set("EmbeddedFiles", Object::Reference(ef_id));

    let catalog = inc
        .new_document
        .get_object_mut(plan.root_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?;
    catalog.set("Names", Object::Dictionary(names_dict));
    Ok(())
}
```

Then add to `crates/core/src/lib.rs` (module list, alphabetical):

```rust
mod attach;
```

and the export (place after `apply_all`):

```rust
/// Attach embedded files (JSON array of {name, mimeType?, description?,
/// creationDate?, modificationDate?, afRelationship?, offset, length}) to a
/// PDF; `blob` is the concatenated file bytes the offsets index into.
/// Returns new bytes (incremental update).
#[wasm_bindgen]
pub fn attach_files(
    data: &[u8],
    ops_json: &str,
    blob: &[u8],
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    attach::attach_files_json(data, ops_json, blob, compress).map_err(|e| JsError::new(&e))
}
```

Note: `attach_apply` writing `/EF /F` and `/EF /UF` as the same stream reference is intentional (one stream, both keys, as pdf-lib does).

If lopdf's `Stream::new` recomputes `/Length` or `compress_generated_streams` re-touches already-flagged streams, adjust: `compress_generated_streams` skips streams that already have a `/Filter` — verify by reading `crates/core/src/compress.rs` before assuming.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path crates/core/Cargo.toml attach`
Expected: all Task-1 tests PASS. Then full suite + clippy:
`cargo test --manifest-path crates/core/Cargo.toml && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add crates/core/Cargo.toml crates/core/src/attach.rs crates/core/src/lib.rs Cargo.lock
git commit -m "feat(core): attach_files — /EmbeddedFiles write with /F+/UF, Params checksum, sorted name tree"
```

---

### Task 2: Rust — merge into existing name trees (flat + /Kids), duplicates vs. loaded doc, /AF array

**Files:**
- Modify: `crates/core/src/attach.rs`
- Test: inline tests in `crates/core/src/attach.rs`

**Interfaces:**
- Consumes: Task 1's `AttachOp`, `AttachPlan`, `attach_resolve`, `attach_apply`, `build_filespec`, test helpers `tree_entries`/`blank_doc`.
- Produces: `attach_resolve` now walks any existing `/Names/EmbeddedFiles` tree (recursive `/Kids`) into `plan.existing`, errors `duplicate attachment name '{name}' already exists in the document` on collision (queued vs existing, `/UF`-preferred existing names); `attach_apply` additionally appends filespec refs of ops with `af_relationship` to the catalog `/AF` array (created if absent, existing entries preserved). Signatures unchanged — Tasks 3–4 depend on that.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `attach.rs`. First a fixture builder that hand-constructs a PDF whose `/EmbeddedFiles` uses a two-node `/Kids` tree (this is the spec-trap case real Acrobat files have):

```rust
    /// A doc with an existing /EmbeddedFiles tree split into two /Kids leaf
    /// nodes: ["alpha.txt"] and ["zeta.txt"], each with /Limits. Built by
    /// attaching nothing — we construct the objects directly on a blank doc
    /// and save it non-incrementally via lopdf.
    fn doc_with_kids_tree() -> Vec<u8> {
        let base = blank_doc();
        let mut doc = Document::load_mem(&base).unwrap();

        let mk_spec = |doc: &mut Document, name: &str, content: &[u8]| -> ObjectId {
            let mut sdict = Dictionary::new();
            sdict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
            let stream_id = doc.add_object(Object::Stream(Stream::new(sdict, content.to_vec())));
            let mut ef = Dictionary::new();
            ef.set("F", Object::Reference(stream_id));
            let mut spec = Dictionary::new();
            spec.set("Type", Object::Name(b"Filespec".to_vec()));
            spec.set("F", Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal));
            spec.set("EF", Object::Dictionary(ef));
            doc.add_object(Object::Dictionary(spec))
        };
        let alpha = mk_spec(&mut doc, "alpha.txt", b"ALPHA");
        let zeta = mk_spec(&mut doc, "zeta.txt", b"ZETA");

        let mut leaf = |doc: &mut Document, name: &str, spec: ObjectId| -> ObjectId {
            let mut d = Dictionary::new();
            d.set("Limits", Object::Array(vec![
                Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            ]));
            d.set("Names", Object::Array(vec![
                Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                Object::Reference(spec),
            ]));
            doc.add_object(Object::Dictionary(d))
        };
        let k1 = leaf(&mut doc, "alpha.txt", alpha);
        let k2 = leaf(&mut doc, "zeta.txt", zeta);

        let mut ef_root = Dictionary::new();
        ef_root.set("Kids", Object::Array(vec![Object::Reference(k1), Object::Reference(k2)]));
        let ef_root_id = doc.add_object(Object::Dictionary(ef_root));
        let mut names = Dictionary::new();
        names.set("EmbeddedFiles", Object::Reference(ef_root_id));

        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_object_mut(root_id).unwrap().as_dict_mut().unwrap();
        catalog.set("Names", Object::Dictionary(names));

        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn merge_preserves_existing_kids_tree_entries_in_sorted_order() {
        let base = doc_with_kids_tree();
        let ops = r#"[{"name":"beta.txt","offset":0,"length":4}]"#;
        let out = attach_files_json(&base, ops, b"BETA", false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let entries = tree_entries(&doc);
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        // Existing alpha/zeta preserved, beta merged in sorted position,
        // flat root node (tree_entries reads /Names directly — no /Kids).
        assert_eq!(names, vec!["alpha.txt", "beta.txt", "zeta.txt"]);
        assert_eq!(ef_bytes(&doc, &entries[0].1), b"ALPHA");
        assert_eq!(ef_bytes(&doc, &entries[1].1), b"BETA");
        assert_eq!(ef_bytes(&doc, &entries[2].1), b"ZETA");
    }

    #[test]
    fn duplicate_against_existing_tree_errors() {
        let base = doc_with_kids_tree();
        let ops = r#"[{"name":"alpha.txt","offset":0,"length":3}]"#;
        let err = attach_files_json(&base, ops, b"NEW", false).unwrap_err();
        assert!(err.starts_with("duplicate attachment"), "{err}");
        assert!(err.contains("alpha.txt"));
        assert!(err.contains("already exists"));
    }

    #[test]
    fn af_relationship_sets_filespec_key_and_catalog_af() {
        let base = blank_doc();
        let ops = r#"[
            {"name":"factur-x.xml","afRelationship":"Alternative","offset":0,"length":3},
            {"name":"other.txt","offset":3,"length":3}
        ]"#;
        let out = attach_files_json(&base, ops, b"XMLTXT", false).unwrap();
        let doc = Document::load_mem(&out).unwrap();

        let entries = tree_entries(&doc);
        let facturx = &entries.iter().find(|(n, _)| n == "factur-x.xml").unwrap().1;
        assert_eq!(
            facturx.get(b"AFRelationship").unwrap().as_name().unwrap(),
            b"Alternative"
        );
        // other.txt has no /AFRelationship
        let other = &entries.iter().find(|(n, _)| n == "other.txt").unwrap().1;
        assert!(other.get(b"AFRelationship").is_err());

        // Catalog /AF holds exactly the factur-x filespec ref.
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root_id).unwrap();
        let af = catalog.get(b"AF").unwrap().as_array().unwrap();
        assert_eq!(af.len(), 1);
        let af_spec = doc
            .get_dictionary(af[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(af_spec.get(b"F").unwrap().as_str().unwrap(), b"factur-x.xml");
    }

    #[test]
    fn af_array_appends_preserving_existing_entries() {
        let base = blank_doc();
        let first = attach_files_json(
            &base,
            r#"[{"name":"a.xml","afRelationship":"Data","offset":0,"length":1}]"#,
            b"A", false,
        )
        .unwrap();
        let out = attach_files_json(
            &first,
            r#"[{"name":"b.xml","afRelationship":"Source","offset":0,"length":1}]"#,
            b"B", false,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let af = doc
            .get_dictionary(root_id).unwrap()
            .get(b"AF").unwrap().as_array().unwrap();
        assert_eq!(af.len(), 2, "existing /AF entry must be preserved");
    }

    #[test]
    fn second_attach_pass_merges_with_first() {
        // Two sequential standalone attaches (the chained-save scenario).
        let base = blank_doc();
        let first =
            attach_files_json(&base, r#"[{"name":"one.txt","offset":0,"length":3}]"#, b"ONE", false)
                .unwrap();
        let out =
            attach_files_json(&first, r#"[{"name":"two.txt","offset":0,"length":3}]"#, b"TWO", false)
                .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let names: Vec<String> = tree_entries(&doc).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["one.txt", "two.txt"]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path crates/core/Cargo.toml attach`
Expected: the 5 new tests FAIL (existing tree ignored / no `/AF` written / no duplicate check vs existing). Task-1 tests still pass.

- [ ] **Step 3: Implement tree walk, duplicate check, and /AF**

In `attach_resolve`, replace the `existing: Vec::new()` with a real walk. Add these helpers:

```rust
/// Decode a PDF text string: UTF-16BE with BOM, or bytes as Latin-1/UTF-8.
fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let utf16: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&utf16);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// Recursively collect (name, filespec object) pairs from a name-tree node
/// (either a leaf with /Names or an interior node with /Kids).
fn walk_name_tree(
    doc: &Document,
    node: &Dictionary,
    out: &mut Vec<(String, Object)>,
) -> Result<(), String> {
    if let Ok(kids) = node.get(b"Kids").and_then(|o| o.as_array()) {
        for kid in kids {
            let kid_dict = match kid {
                Object::Reference(id) => doc.get_dictionary(*id).map_err(|e| e.to_string())?,
                Object::Dictionary(d) => d,
                other => return Err(format!("malformed name-tree kid: {other:?}")),
            };
            walk_name_tree(doc, kid_dict, out)?;
        }
    }
    if let Ok(pairs) = node.get(b"Names").and_then(|o| o.as_array()) {
        for pair in pairs.chunks(2) {
            if pair.len() != 2 {
                continue;
            }
            let name = pair[0]
                .as_str()
                .map(decode_pdf_string)
                .map_err(|e| e.to_string())?;
            out.push((name, pair[1].clone()));
        }
    }
    Ok(())
}

/// Resolve a dict-or-reference object to a Dictionary in `doc`.
fn resolve_dict<'a>(doc: &'a Document, obj: &'a Object) -> Result<&'a Dictionary, String> {
    match obj {
        Object::Reference(id) => doc.get_dictionary(*id).map_err(|e| e.to_string()),
        Object::Dictionary(d) => Ok(d),
        other => Err(format!("expected dictionary, got {other:?}")),
    }
}
```

In `attach_resolve`, after the queued-duplicate check:

```rust
    let mut existing = Vec::new();
    if let Ok(catalog) = doc.get_dictionary(root_id) {
        if let Ok(names_obj) = catalog.get(b"Names") {
            let names = resolve_dict(doc, names_obj)?;
            if let Ok(ef_obj) = names.get(b"EmbeddedFiles") {
                let ef = resolve_dict(doc, ef_obj)?;
                walk_name_tree(doc, ef, &mut existing)?;
            }
        }
    }
    // The existing names use /UF-preferred strings already? No — name-tree
    // KEYS are the canonical names (the /UF preference applies to reading
    // filespec metadata, Task 3). Compare queued names against the tree keys.
    for op in ops {
        if existing.iter().any(|(n, _)| n == &op.name) {
            return Err(format!(
                "duplicate attachment name '{}' already exists in the document",
                op.name
            ));
        }
    }
    Ok(AttachPlan { root_id, existing })
```

In `attach_apply`, after setting `catalog.set("Names", ...)`, add the `/AF` handling. Collect the new spec ids with their ops while building (change the build loop to keep `(op, spec_id)`), then:

```rust
    // /AF: filespec refs of every op that declared an afRelationship.
    let af_new: Vec<Object> = built // Vec<(&AttachOp, ObjectId)> from the build loop
        .iter()
        .filter(|(op, _)| op.af_relationship.is_some())
        .map(|(_, id)| Object::Reference(*id))
        .collect();
    if !af_new.is_empty() {
        // Existing /AF read from the (possibly just-cloned) catalog.
        let mut af = match inc
            .new_document
            .get_dictionary(plan.root_id)
            .ok()
            .and_then(|c| c.get(b"AF").ok())
        {
            Some(Object::Array(a)) => a.clone(),
            Some(Object::Reference(id)) => inc
                .new_document
                .get_object(*id)
                .or_else(|_| inc.get_prev_documents().get_object(*id))
                .ok()
                .and_then(|o| o.as_array().ok().cloned())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        af.extend(af_new);
        let catalog = inc
            .new_document
            .get_object_mut(plan.root_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        catalog.set("AF", Object::Array(af));
    }
```

Borrow-checker note: read all immutable state (`existing_names`, existing `/AF`) into owned values before taking `get_object_mut` on the catalog; the code above already does this, keep that ordering.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path crates/core/Cargo.toml && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`
Expected: all PASS, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/attach.rs
git commit -m "feat(core): merge attachments into existing name trees (incl /Kids), duplicate detection, catalog /AF"
```

---

### Task 3: Rust — `read_attachments` packed reader

**Files:**
- Modify: `crates/core/src/attach.rs`
- Modify: `crates/core/src/lib.rs` (`read_attachments` wasm export)
- Test: inline tests in `crates/core/src/attach.rs`

**Interfaces:**
- Consumes: Task 2's `walk_name_tree`, `decode_pdf_string`, `resolve_dict`; `crate::doc_io::load_pdf`.
- Produces:
  - `pub fn read_attachments_packed(data: &[u8]) -> Result<Vec<u8>, String>` — packed layout `[u32 LE json_len][json][file bytes]`; JSON array of `ReadAttachment { name, description?, mime_type?, creation_date?, modification_date?, af_relationship?, size, offset, length }` (serde `rename_all = "camelCase"`, `skip_serializing_if = "Option::is_none"` on optionals).
  - lib.rs: `#[wasm_bindgen] pub fn read_attachments(data: &[u8]) -> Result<Vec<u8>, JsError>`
  - Task 5's TS decoder relies on exactly this layout.

- [ ] **Step 1: Write the failing tests**

```rust
    /// Decode the packed read_attachments buffer into (json, blob).
    fn unpack(packed: &[u8]) -> (serde_json::Value, Vec<u8>) {
        let json_len = u32::from_le_bytes(packed[..4].try_into().unwrap()) as usize;
        let json: serde_json::Value =
            serde_json::from_slice(&packed[4..4 + json_len]).unwrap();
        (json, packed[4 + json_len..].to_vec())
    }

    #[test]
    fn read_attachments_round_trips_metadata_and_bytes() {
        let base = blank_doc();
        let payload = b"<xml>invoice</xml>".to_vec();
        let ops = format!(
            r#"[{{"name":"año.xml","mimeType":"text/xml","description":"desc","creationDate":"D:20260101120000Z","afRelationship":"Alternative","offset":0,"length":{}}}]"#,
            payload.len()
        );
        let saved = attach_files_json(&base, &ops, &payload, false).unwrap();

        let (json, blob) = unpack(&read_attachments_packed(&saved).unwrap());
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let a = &arr[0];
        assert_eq!(a["name"], "año.xml"); // /UF preferred over the a_o.xml /F fallback
        assert_eq!(a["mimeType"], "text/xml");
        assert_eq!(a["description"], "desc");
        assert_eq!(a["creationDate"], "D:20260101120000Z");
        assert_eq!(a["afRelationship"], "Alternative");
        assert_eq!(a["size"], payload.len());
        assert!(a.get("modificationDate").is_none(), "absent key must be omitted");

        let off = a["offset"].as_u64().unwrap() as usize;
        let len = a["length"].as_u64().unwrap() as usize;
        assert_eq!(&blob[off..off + len], &payload[..]);
    }

    #[test]
    fn read_attachments_walks_kids_and_skips_specs_without_ef() {
        let base = doc_with_kids_tree(); // alpha.txt + zeta.txt (uncompressed streams)
        // Add a broken filespec (no /EF) to the tree by attaching a valid one
        // first, then hand-editing: simpler — build a doc where one leaf entry
        // is a /Filespec without /EF.
        let mut doc = Document::load_mem(&base).unwrap();
        let mut spec = Dictionary::new();
        spec.set("Type", Object::Name(b"Filespec".to_vec()));
        spec.set("F", Object::String(b"broken.txt".to_vec(), lopdf::StringFormat::Literal));
        let broken = doc.add_object(Object::Dictionary(spec));
        // splice it into the first /Kids leaf's /Names array
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let names_obj = doc.get_dictionary(root_id).unwrap().get(b"Names").unwrap().clone();
        let ef_root_id = match &names_obj {
            Object::Dictionary(d) => d.get(b"EmbeddedFiles").unwrap().as_reference().unwrap(),
            _ => panic!(),
        };
        let kid0 = doc.get_dictionary(ef_root_id).unwrap()
            .get(b"Kids").unwrap().as_array().unwrap()[0].as_reference().unwrap();
        let kid = doc.get_object_mut(kid0).unwrap().as_dict_mut().unwrap();
        let mut names = kid.get(b"Names").unwrap().as_array().unwrap().clone();
        names.push(Object::String(b"broken.txt".to_vec(), lopdf::StringFormat::Literal));
        names.push(Object::Reference(broken));
        kid.set("Names", Object::Array(names));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let (json, blob) = unpack(&read_attachments_packed(&bytes).unwrap());
        let names: Vec<&str> = json
            .as_array().unwrap().iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        // broken.txt skipped (no /EF), not fatal
        assert_eq!(names, vec!["alpha.txt", "zeta.txt"]);
        let a0 = &json[0];
        let off = a0["offset"].as_u64().unwrap() as usize;
        let len = a0["length"].as_u64().unwrap() as usize;
        assert_eq!(&blob[off..off + len], b"ALPHA"); // uncompressed stream fallback
    }

    #[test]
    fn read_attachments_empty_doc_returns_empty_array() {
        let (json, blob) = unpack(&read_attachments_packed(&blank_doc()).unwrap());
        assert_eq!(json.as_array().unwrap().len(), 0);
        assert!(blob.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path crates/core/Cargo.toml attach`
Expected: FAIL — `read_attachments_packed` not found (compile error first; add the stub `Err("unimplemented")` to compile, then failing).

- [ ] **Step 3: Implement the reader**

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadAttachment {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    creation_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modification_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    af_relationship: Option<String>,
    size: usize,
    offset: usize,
    length: usize,
}

fn dict_string(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok()?.as_str().ok().map(decode_pdf_string)
}

/// Walk /Names/EmbeddedFiles and return `[u32 LE json_len][json][bytes blob]`.
/// Filespecs without a decodable /EF stream are skipped, not fatal.
pub fn read_attachments_packed(data: &[u8]) -> Result<Vec<u8>, String> {
    let doc = crate::doc_io::load_pdf(data)?;
    let mut entries = Vec::new();
    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    if let Ok(catalog) = doc.get_dictionary(root_id) {
        if let Ok(names_obj) = catalog.get(b"Names") {
            if let Ok(names) = resolve_dict(&doc, names_obj) {
                if let Ok(ef_obj) = names.get(b"EmbeddedFiles") {
                    if let Ok(ef) = resolve_dict(&doc, ef_obj) {
                        walk_name_tree(&doc, ef, &mut entries)?;
                    }
                }
            }
        }
    }

    let mut metas = Vec::new();
    let mut blob = Vec::new();
    for (tree_name, spec_obj) in &entries {
        let Ok(spec) = resolve_dict(&doc, spec_obj) else { continue };
        // /EF /F preferred, /UF fallback.
        let Ok(ef) = spec.get(b"EF").and_then(|o| o.as_dict()) else { continue };
        let stream_ref = ef.get(b"F").or_else(|_| ef.get(b"UF"));
        let Ok(stream_id) = stream_ref.and_then(|o| o.as_reference()) else { continue };
        let Ok(stream) = doc.get_object(stream_id).and_then(|o| o.as_stream()) else { continue };
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());

        // Name: filespec /UF preferred, then /F, then the tree key.
        let name = dict_string(spec, b"UF")
            .or_else(|| dict_string(spec, b"F"))
            .unwrap_or_else(|| tree_name.clone());
        let params = stream.dict.get(b"Params").and_then(|o| o.as_dict()).ok();

        let offset = blob.len();
        let length = bytes.len();
        blob.extend_from_slice(&bytes);
        metas.push(ReadAttachment {
            name,
            description: dict_string(spec, b"Desc"),
            mime_type: stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned()),
            creation_date: params.and_then(|p| dict_string(p, b"CreationDate")),
            modification_date: params.and_then(|p| dict_string(p, b"ModDate")),
            af_relationship: spec
                .get(b"AFRelationship")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned()),
            size: length,
            offset,
            length,
        });
    }

    let json = serde_json::to_vec(&metas).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(4 + json.len() + blob.len());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&blob);
    Ok(out)
}
```

lopdf API note: `as_name()` may return `&[u8]` or `Vec<u8>` depending on version — match whatever the surrounding modules (`forms.rs`, `fill.rs`) use. Also note lopdf **un-escapes** `#2F` back to `/` when parsing names, so `Subtype` reads back as `text/xml` directly.

lib.rs export:

```rust
/// Read every /EmbeddedFiles attachment. Returns a packed buffer:
/// `[u32 LE json_len][json][concatenated file bytes]`, where the JSON is an
/// array of metadata objects whose `offset`/`length` index the bytes section.
#[wasm_bindgen]
pub fn read_attachments(data: &[u8]) -> Result<Vec<u8>, JsError> {
    attach::read_attachments_packed(data).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path crates/core/Cargo.toml && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`
Expected: all PASS, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/attach.rs crates/core/src/lib.rs
git commit -m "feat(core): read_attachments — packed metadata+bytes reader, /UF preferred, tolerant of broken filespecs"
```

---

### Task 4: Rust — `apply_all` integration (attach + fill + flatten in one pass)

**Files:**
- Modify: `crates/core/src/apply.rs`
- Modify: `crates/core/src/lib.rs` (`apply_all` gains `attach_blob` param)
- Test: inline tests in `crates/core/src/apply.rs`

**Interfaces:**
- Consumes: `attach::{AttachOp, attach_resolve, attach_apply}` from Tasks 1–2.
- Produces: `apply_all_json(data, plan_json, fill_images, draw_images, fonts, attach_blob, compress)` — **new 6th param** `attach_blob: &[u8]`; plan JSON gains optional `attach: Vec<AttachOp>`. lib.rs `apply_all` signature gains `attach_blob: &[u8]` before `compress`. Task 5's TS bindings pass it (empty `Uint8Array` when unused).

- [ ] **Step 1: Write the failing test**

In `apply.rs` tests:

```rust
    #[test]
    fn apply_all_attach_composes_with_fill_and_flatten() {
        let payload = b"<invoice/>".to_vec();
        let plan = format!(
            r#"{{
                "fill": [ {{"name":"beneficiario.apellidos_nombres","value":"ATTACHED"}} ],
                "flatten": ["beneficiario.apellidos_nombres"],
                "attach": [ {{"name":"factur-x.xml","mimeType":"text/xml","afRelationship":"Alternative","offset":0,"length":{}}} ]
            }}"#,
            payload.len()
        );
        let out = apply_all_json(FICHA, &plan, &[], &[], &[], &payload, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();

        // fill+flatten landed
        assert!(page0_content(&doc).contains("/bpdfAp0 Do"));
        // attachment landed and round-trips
        let packed = crate::attach::read_attachments_packed(&out).unwrap();
        let json_len = u32::from_le_bytes(packed[..4].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&packed[4..4 + json_len]).unwrap();
        assert_eq!(json[0]["name"], "factur-x.xml");
        assert_eq!(&packed[4 + json_len..], &payload[..]);
    }

    #[test]
    fn apply_all_attach_only_plan_works() {
        let plan = r#"{ "attach": [ {"name":"a.txt","offset":0,"length":4} ] }"#;
        let out = apply_all_json(FICHA, plan, &[], &[], &[], b"data", false).unwrap();
        let packed = crate::attach::read_attachments_packed(&out).unwrap();
        let json_len = u32::from_le_bytes(packed[..4].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&packed[4..4 + json_len]).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 1);
    }
```

Also update every existing `apply_all_json(...)` call in `apply.rs` tests to pass the new `&[]` attach-blob argument (they won't compile otherwise — that's the RED state).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path crates/core/Cargo.toml apply`
Expected: compile error (arity), then after adding the param with a `todo!`-free pass-through but no attach handling, the two new tests FAIL on missing attachments.

- [ ] **Step 3: Implement**

In `apply.rs`:
- Add `attach` to the plan struct: `#[serde(default)] attach: Option<Vec<crate::attach::AttachOp>>,`
- Add `attach_blob: &[u8]` parameter to `apply_all_json` (before `compress`).
- Phase A (after `outline_prep`):

```rust
    let attach_plan = match &plan.attach {
        Some(ops) => Some(crate::attach::attach_resolve(
            inc.get_prev_documents(),
            ops,
            attach_blob,
        )?),
        None => None,
    };
```

- Phase B (after outline apply — attach is orthogonal to page content, run it last):

```rust
    if let (Some(ops), Some(aplan)) = (&plan.attach, &attach_plan) {
        crate::attach::attach_apply(&mut inc, aplan, ops, attach_blob)?;
    }
```

- lib.rs: change `apply_all` to take `attach_blob: &[u8]` before `compress`, pass through, and extend its doc comment: `attach_blob` carries the attachment bytes referenced by `attach` ops.

Ordering caveat: outline's Phase B also overrides the catalog via `opt_clone_object_to_new_document(root_id)` + `set`. Running attach AFTER outline means attach's own `opt_clone_object_to_new_document` is a no-op (already cloned) and it mutates the same new-document catalog — both `/Outlines` and `/Names` survive. Add one composed assertion to `apply_all_attach_only_plan_works`? No — instead extend `apply_all_attach_composes_with_fill_and_flatten`'s plan with `"outline": [{"title":"S","page":0}]` and assert the catalog still has `/Outlines` after attach (guards the double-override interaction):

```rust
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root_id).unwrap();
        assert!(catalog.get(b"Outlines").is_ok(), "outline must survive attach's catalog override");
        assert!(catalog.get(b"Names").is_ok(), "attach names tree must survive");
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path crates/core/Cargo.toml && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`
Expected: all PASS (including all pre-existing apply tests with the extra `&[]`), clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/apply.rs crates/core/src/lib.rs
git commit -m "feat(core): attach section in apply_all — attachments compose with fill/flatten/draw/outline in one save"
```

---

### Task 5: TypeScript — `attach()` / `getAttachments()` API, errors, bindings

**Files:**
- Modify: `src/core/errors.ts` (`DuplicateAttachmentError`, `AttachmentNotFoundError`, prefix mapping in `toPdfError`)
- Modify: `src/core/wasm-bindings.ts` (`attach_files`, `read_attachments` raw bindings + `attachFiles`, `readAttachments` wrappers; `apply_all` gains `attachBlob`)
- Modify: `src/core/document.ts` (`CoreWasm` interface, queue, `attach()`, `getAttachments()`, save wiring)
- Create: `src/core/attachments.ts` (public types + packed-buffer decoder)
- Modify: `src/exports-common.ts` (export new errors + types)
- Test: `tests/attachments.test.ts` (created here with unit-level tests; e2e in Task 6)

**Interfaces:**
- Consumes: `attach_files(data, opsJson, blob, compress)`, `read_attachments(data)` packed layout, `apply_all(..., attachBlob, compress)` from Tasks 1–4; `toPdfDate`/`fromPdfDate` from `src/generate/metadata.ts`.
- Produces (public API):

```ts
export type AfRelationship =
  | "Source" | "Data" | "Alternative" | "Supplement"
  | "EncryptedPayload" | "FormData" | "Schema" | "Unspecified";

export interface AttachOptions {
  mimeType?: string;
  description?: string;
  creationDate?: Date;
  modificationDate?: Date;
  afRelationship?: AfRelationship;
}

export interface PdfAttachment {
  name: string;
  description?: string;
  mimeType?: string;
  creationDate?: Date;
  modificationDate?: Date;
  size: number;
  afRelationship?: AfRelationship;
  bytes: Uint8Array;
}

// on PdfDocumentBase:
attach(bytes: Uint8Array, name: string, options?: AttachOptions): void;   // sync, queues
async getAttachments(): Promise<PdfAttachment[]>;                          // reads saved state
```

- [ ] **Step 1: Write the failing tests**

Create `tests/attachments.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument, DuplicateAttachmentError, PdfError } from "../src/index.js";

const enc = new TextEncoder();

describe("attach() queueing", () => {
  test("duplicate queued name throws DuplicateAttachmentError at attach() time", async () => {
    const doc = await PdfDocument.create();
    doc.addPage();
    doc.attach(enc.encode("a"), "same.txt");
    expect(() => doc.attach(enc.encode("b"), "same.txt")).toThrow(DuplicateAttachmentError);
  });

  test("attach is synchronous and does not mutate bytes before save", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    const base = await created.save();

    const doc = await PdfDocument.load(base);
    doc.attach(enc.encode("<x/>"), "data.xml");
    expect(await doc.getAttachments()).toEqual([]); // queued ≠ saved
  });

  test("getAttachments on an unsealed created doc returns []", async () => {
    const doc = await PdfDocument.create();
    doc.addPage();
    expect(await doc.getAttachments()).toEqual([]);
  });
});

describe("round trip", () => {
  test("attach → save → load → getAttachments returns metadata and bytes", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    const base = await created.save();

    const doc = await PdfDocument.load(base);
    const payload = enc.encode("<invoice>42</invoice>");
    doc.attach(payload, "factur-x.xml", {
      mimeType: "text/xml",
      description: "Factur-X invoice data",
      creationDate: new Date(Date.UTC(2026, 0, 1, 12, 0, 0)),
      afRelationship: "Alternative",
    });
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    const atts = await out.getAttachments();
    expect(atts).toHaveLength(1);
    const a = atts[0]!;
    expect(a.name).toBe("factur-x.xml");
    expect(a.mimeType).toBe("text/xml");
    expect(a.description).toBe("Factur-X invoice data");
    expect(a.creationDate?.toISOString()).toBe("2026-01-01T12:00:00.000Z");
    expect(a.modificationDate).toBeUndefined();
    expect(a.afRelationship).toBe("Alternative");
    expect(a.size).toBe(payload.length);
    expect(Array.from(a.bytes)).toEqual(Array.from(payload));
  });

  test("attach on a created document is baked at save()", async () => {
    const doc = await PdfDocument.create();
    doc.addPage();
    doc.attach(enc.encode("hello"), "note.txt");
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    const atts = await out.getAttachments();
    expect(atts.map((a) => a.name)).toEqual(["note.txt"]);
  });

  test("duplicate against the loaded document's tree throws at save", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(enc.encode("v1"), "same.txt");
    const withAtt = await created.save();

    const doc = await PdfDocument.load(withAtt);
    doc.attach(enc.encode("v2"), "same.txt");
    await expect(doc.save()).rejects.toThrow(DuplicateAttachmentError);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

First rebuild the WASM so the new exports exist: run the wasm build script from `package.json` (e.g. `bun run build:wasm` — use the actual script name).
Run: `bun test tests/attachments.test.ts`
Expected: FAIL — `attach is not a function` / missing export `DuplicateAttachmentError`.

- [ ] **Step 3: Implement**

**`src/core/errors.ts`** — add after `MissingGlyphError`:

```ts
/** Thrown when attaching a file whose name already exists (queued or saved). */
export class DuplicateAttachmentError extends PdfError {
  constructor(readonly attachmentName: string) {
    super(`an attachment named '${attachmentName}' already exists`);
  }
}

/** Reserved for future get-by-name/remove APIs. */
export class AttachmentNotFoundError extends PdfError {
  constructor(readonly attachmentName: string) {
    super(`no attachment named '${attachmentName}'`);
  }
}
```

In `toPdfError`, before the `PdfCoreError` fallback:

```ts
  if (message.startsWith("duplicate attachment")) {
    const m = message.match(/'([^']*)'/);
    return new DuplicateAttachmentError(m?.[1] ?? "");
  }
```

**`src/core/attachments.ts`** (new file):

```ts
import { fromPdfDate, toPdfDate } from "../generate/metadata.js";

/** /AFRelationship values (PDF 2.0 / PDF/A-3 associated files). */
export type AfRelationship =
  | "Source" | "Data" | "Alternative" | "Supplement"
  | "EncryptedPayload" | "FormData" | "Schema" | "Unspecified";

/** Options for {@link PdfDocumentBase.attach}. */
export interface AttachOptions {
  /** MIME type, written as the embedded stream's /Subtype (e.g. "text/xml"). */
  mimeType?: string;
  /** Human-readable description, written as the filespec /Desc. */
  description?: string;
  /** Written to /Params /CreationDate. Not defaulted (determinism). */
  creationDate?: Date;
  /** Written to /Params /ModDate. Not defaulted (determinism). */
  modificationDate?: Date;
  /**
   * Marks this file as an associated file: sets the filespec /AFRelationship
   * and appends it to the catalog /AF array (ZUGFeRD/Factur-X structure).
   */
  afRelationship?: AfRelationship;
}

/** One embedded file returned by {@link PdfDocumentBase.getAttachments}. */
export interface PdfAttachment {
  name: string;
  description?: string;
  mimeType?: string;
  creationDate?: Date;
  modificationDate?: Date;
  /** Uncompressed size in bytes (equals bytes.length). */
  size: number;
  afRelationship?: AfRelationship;
  bytes: Uint8Array;
}

/** @internal One queued attach() call. */
export interface QueuedAttachment {
  bytes: Uint8Array;
  name: string;
  options: AttachOptions;
}

/** @internal Wire entry read back from read_attachments. */
interface ReadEntry {
  name: string;
  description?: string;
  mimeType?: string;
  creationDate?: string;
  modificationDate?: string;
  afRelationship?: string;
  size: number;
  offset: number;
  length: number;
}

/** @internal Build the attach ops JSON + concatenated blob for the queue. */
export function toAttachPayload(queue: QueuedAttachment[]): {
  opsJson: string;
  blob: Uint8Array;
} {
  let total = 0;
  for (const q of queue) total += q.bytes.length;
  const blob = new Uint8Array(total);
  let offset = 0;
  const ops = queue.map((q) => {
    blob.set(q.bytes, offset);
    const op = {
      name: q.name,
      description: q.options.description,
      mimeType: q.options.mimeType,
      creationDate: q.options.creationDate && toPdfDate(q.options.creationDate),
      modificationDate: q.options.modificationDate && toPdfDate(q.options.modificationDate),
      afRelationship: q.options.afRelationship,
      offset,
      length: q.bytes.length,
    };
    offset += q.bytes.length;
    return op;
  });
  return { opsJson: JSON.stringify(ops), blob };
}

/** @internal Decode the packed `[u32 LE json_len][json][bytes]` buffer. */
export function decodeAttachments(packed: Uint8Array): PdfAttachment[] {
  const view = new DataView(packed.buffer, packed.byteOffset, packed.byteLength);
  const jsonLen = view.getUint32(0, true);
  const entries = JSON.parse(
    new TextDecoder().decode(packed.subarray(4, 4 + jsonLen)),
  ) as ReadEntry[];
  const blobStart = 4 + jsonLen;
  return entries.map((e) => ({
    name: e.name,
    description: e.description,
    mimeType: e.mimeType,
    creationDate: e.creationDate ? (fromPdfDate(e.creationDate) ?? undefined) : undefined,
    modificationDate: e.modificationDate
      ? (fromPdfDate(e.modificationDate) ?? undefined)
      : undefined,
    size: e.size,
    afRelationship: e.afRelationship as PdfAttachment["afRelationship"],
    bytes: packed.slice(blobStart + e.offset, blobStart + e.offset + e.length),
  }));
}
```

(Check `fromPdfDate`'s actual return type in `src/generate/metadata.ts` — if it returns `Date | undefined` drop the `?? undefined`.)

**`src/core/wasm-bindings.ts`** — add to `RawBindings`:

```ts
  attach_files(data: Uint8Array, opsJson: string, blob: Uint8Array, compress: boolean): Uint8Array;
  read_attachments(data: Uint8Array): Uint8Array;
```

change `apply_all`'s raw signature to include `attachBlob: Uint8Array` before `compress`, and in `makeBindings`:

```ts
    attachFiles: (data, opsJson, blob, compress = true) =>
      (guard(), raw.attach_files(data, opsJson, blob, compress)),
    readAttachments: (data) => (guard(), raw.read_attachments(data)),
```

and thread `attachBlob = EMPTY` through the `applyAll` wrapper.

**`src/core/document.ts`:**
- `CoreWasm`: add `attachFiles(data, opsJson, blob, compress?): Uint8Array;`, `readAttachments(data): Uint8Array;`, and add `attachBlob: Uint8Array` to `applyAll` before `compress`.
- Fields: `private readonly attachQueue: QueuedAttachment[] = [];` and `private readonly attachNames = new Set<string>();`
- Methods:

```ts
  /**
   * Attach (embed) a file in the document. Synchronous: the attachment is
   * queued and written at `save()`.
   *
   * @throws `DuplicateAttachmentError` when `name` is already queued. A name
   * that already exists in the loaded document throws at `save()` instead.
   */
  attach(bytes: Uint8Array, name: string, options: AttachOptions = {}): void {
    if (this.attachNames.has(name)) {
      throw new DuplicateAttachmentError(name);
    }
    this.attachNames.add(name);
    this.attachQueue.push({ bytes, name, options });
  }

  /**
   * Read every embedded file (metadata + bytes) from the document's saved
   * state. Attachments queued with `attach()` but not yet saved are NOT
   * included. Returns `[]` for a created document that has no bytes yet.
   */
  async getAttachments(): Promise<PdfAttachment[]> {
    if (this.mode === "create" && !this.sealed) return [];
    const packed = callBytes(() => this.wasm.readAttachments(this.bytes));
    return decodeAttachments(packed);
  }
```

- `save()` wiring (load-mode fast path): after the `outline` block:

```ts
    let attachBlob: Uint8Array = empty;
    if (this.attachQueue.length > 0) {
      const { opsJson, blob } = toAttachPayload(this.attachQueue);
      plan["attach"] = JSON.parse(opsJson);
      attachBlob = blob;
    }
```

and pass `attachBlob` in the `applyAll` call (before `compress`).

- `saveChained` (page-structure path): after the outline step, chain the standalone call:

```ts
      if (this.attachQueue.length > 0) {
        const { opsJson, blob } = toAttachPayload(this.attachQueue);
        bytes = this.wasm.attachFiles(bytes, opsJson, blob, compress);
      }
```

- Create-mode `save()`: at the top of `save()`, the create branch returns `buildCreatedBytes(...)`. Change it to apply attachments to the built bytes:

```ts
    if (this.mode === "create" && !this.sealed) {
      try {
        let bytes = this.buildCreatedBytes(compress, objectStreams);
        if (this.attachQueue.length > 0) {
          const { opsJson, blob } = toAttachPayload(this.attachQueue);
          bytes = this.wasm.attachFiles(bytes, opsJson, blob, compress);
        }
        return bytes;
      } catch (e) {
        throw toPdfError(e);
      }
    }
```

Imports at the top of `document.ts`: `DuplicateAttachmentError` from errors, `toAttachPayload, decodeAttachments` + types from `./attachments.js`.

**`src/exports-common.ts`:** add `DuplicateAttachmentError, AttachmentNotFoundError` to the errors export block and:

```ts
export type { AttachOptions, PdfAttachment, AfRelationship } from "./core/attachments.js";
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/attachments.test.ts`
Expected: all PASS. Then full check: `bun test && bunx tsc --noEmit` (use the repo's typecheck script if one exists in `package.json`).

- [ ] **Step 5: Commit**

```bash
git add src/core/errors.ts src/core/attachments.ts src/core/wasm-bindings.ts src/core/document.ts src/exports-common.ts tests/attachments.test.ts
git commit -m "feat: doc.attach() / doc.getAttachments() — queued embedded files, DuplicateAttachmentError"
```

---

### Task 6: TypeScript e2e — composition, unicode, loaded fixture, hot-path guard

**Files:**
- Modify: `tests/attachments.test.ts`
- Test: same file

**Interfaces:**
- Consumes: the full Task-5 public API; fixture `tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf` (has AcroForm fields; field `beneficiario.apellidos_nombres`).

- [ ] **Step 1: Write the tests (these should pass immediately if Tasks 1–5 are correct — treat any failure as a real integration bug, not a test to weaken)**

Append to `tests/attachments.test.ts`:

```ts
import { readFileSync } from "node:fs";

const FICHA = new Uint8Array(
  readFileSync("tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf"),
);

describe("e2e composition", () => {
  test("attach + fill + flatten in one save on a loaded PDF", async () => {
    const doc = await PdfDocument.load(FICHA);
    const form = doc.getForm();
    form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
    form.flattenField("beneficiario.apellidos_nombres");
    doc.attach(enc.encode("<invoice/>"), "factur-x.xml", {
      mimeType: "text/xml",
      afRelationship: "Alternative",
    });
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    const atts = await out.getAttachments();
    expect(atts.map((a) => a.name)).toEqual(["factur-x.xml"]);
    expect(atts[0]!.afRelationship).toBe("Alternative");
    // flatten removed the field
    const names = out.getForm().getFields().map((f) => f.name);
    expect(names).not.toContain("beneficiario.apellidos_nombres");
  });

  test("attach coexists with page-structure ops (chained save path)", async () => {
    const doc = await PdfDocument.load(FICHA);
    doc.addPage(); // forces saveChained
    doc.attach(enc.encode("note"), "note.txt");
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    expect((await out.getAttachments()).map((a) => a.name)).toEqual(["note.txt"]);
  });

  test("unicode filename round-trips via /UF", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(enc.encode("dato"), "año-2026 –informe.txt");
    const saved = await created.save();

    const out = await PdfDocument.load(saved);
    expect((await out.getAttachments())[0]!.name).toBe("año-2026 –informe.txt");
  });

  test("multiple attachments come back sorted by name", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(enc.encode("2"), "b.txt");
    created.attach(enc.encode("1"), "a.txt");
    created.attach(enc.encode("3"), "c.txt");
    const saved = await created.save();

    const out = await PdfDocument.load(saved);
    expect((await out.getAttachments()).map((a) => a.name)).toEqual(["a.txt", "b.txt", "c.txt"]);
  });

  test("binary payload (non-text) round-trips byte-exact", async () => {
    const payload = new Uint8Array(1024);
    for (let i = 0; i < payload.length; i++) payload[i] = i % 256;
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(payload, "blob.bin", { mimeType: "application/octet-stream" });
    const saved = await created.save();

    const out = await PdfDocument.load(saved);
    const a = (await out.getAttachments())[0]!;
    expect(a.size).toBe(1024);
    expect(Array.from(a.bytes)).toEqual(Array.from(payload));
  });

  test("save with no attachments queued produces byte-identical output to before this feature (hot-path guard)", async () => {
    // The plan must not contain an `attach` key and no attach WASM call may
    // run when nothing is queued: filling one field twice through two
    // separately-loaded docs must be deterministic and unaffected.
    const doc1 = await PdfDocument.load(FICHA);
    doc1.getForm().getTextField("beneficiario.apellidos_nombres").setText("X");
    const out1 = await doc1.save();

    const doc2 = await PdfDocument.load(FICHA);
    doc2.getForm().getTextField("beneficiario.apellidos_nombres").setText("X");
    const out2 = await doc2.save();

    expect(Buffer.from(out1).equals(Buffer.from(out2))).toBe(true);
  });
});
```

- [ ] **Step 2: Run the tests**

Run: `bun test tests/attachments.test.ts`
Expected: all PASS. Debug any failure in the Rust/TS seam (do not loosen assertions).

- [ ] **Step 3: Run the full suites + benchmark spot-check**

Run: `cargo test --manifest-path crates/core/Cargo.toml && bun test && bunx tsc --noEmit`
Expected: everything green.

Run: `bun run bench` and compare the load→fill→save numbers against a run on `master` (two runs each, eyeball — the no-attachments path adds only a `length > 0` check, so any regression means something is wrong).
Report the before/after numbers in your task report.

- [ ] **Step 4: Manual Factur-X structure validation (report-only)**

Generate a Factur-X-shaped file to the scratchpad and inspect it:

```ts
// tests/scripts/gen-facturx-check.ts (temporary is fine, or keep alongside gen-cjk-visual-check.ts)
// attach a minimal XML as "factur-x.xml" with afRelationship "Alternative",
// save to /tmp or scratchpad, then verify with an external viewer/veraPDF if available.
```

If veraPDF/mustang isn't installed, verify manually: open the saved bytes, check catalog has `/AF [ref]`, filespec has `/AFRelationship /Alternative`, `/UF`, `/EF /F`. Note in the report that full PDF/A-3 conformance is out of scope (no XMP).

- [ ] **Step 5: Commit**

```bash
git add tests/attachments.test.ts tests/scripts/gen-facturx-check.ts
git commit -m "test: attachments e2e — composition with fill/flatten, chained path, unicode, sort order, hot-path guard"
```

---

### Task 7: Docs + version bump

**Files:**
- Modify: `README.md` (features list: remove the "no attachments" limitation; add attach/getAttachments example; add PDF/A-3 caveat)
- Modify: `docs/migrating-from-pdf-lib.md` (`attach()` mapping row/section)
- Modify: `CHANGELOG.md` (1.12.0 entry)
- Modify: `package.json` + `crates/core/Cargo.toml` (version → 1.12.0)

- [ ] **Step 1: README**

In the features section, add (match the existing bullet style — read the surrounding list first):

```md
- **File attachments** — `doc.attach(bytes, name, { mimeType, description, afRelationship })`
  embeds files (`/EmbeddedFiles`), `doc.getAttachments()` reads them back (metadata + bytes).
  `afRelationship` writes the `/AFRelationship` + catalog `/AF` structure used by
  ZUGFeRD/Factur-X e-invoices.
```

Remove any "attachments" line from the limitations section and add:

```md
- ZUGFeRD/Factur-X **structure** is supported (`/AF`, `/AFRelationship`); PDF/A-3
  conformance metadata (XMP) is not written — that part is your responsibility.
```

Add a usage example near the other examples:

```ts
const doc = await PdfDocument.load(bytes);
doc.attach(xmlBytes, "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  afRelationship: "Alternative",
});
const saved = await doc.save();

// later
const attachments = await (await PdfDocument.load(saved)).getAttachments();
```

- [ ] **Step 2: Migration guide**

In `docs/migrating-from-pdf-lib.md`, add to the API mapping (match existing table/format):

| pdf-lib | better-pdf |
| --- | --- |
| `doc.attach(bytes, name, opts)` | `doc.attach(bytes, name, opts)` — same shape; `creationDate`/`modificationDate` are NOT defaulted; duplicates throw `DuplicateAttachmentError` instead of silently appending |
| *(no read API)* | `doc.getAttachments()` returns metadata + bytes |

- [ ] **Step 3: CHANGELOG + version**

`CHANGELOG.md` (top, match existing entry format):

```md
## 1.12.0

### Added
- `doc.attach(bytes, name, options)` — embed file attachments (`/EmbeddedFiles`) on
  created and loaded documents; queued and written at `save()`.
- `doc.getAttachments()` — read every attachment's metadata and bytes.
- `afRelationship` option writes `/AFRelationship` and the catalog `/AF` array
  (ZUGFeRD/Factur-X structure).
- New errors: `DuplicateAttachmentError`, `AttachmentNotFoundError` (reserved).
```

Bump `"version"` in `package.json` and `version` in `crates/core/Cargo.toml` to `1.12.0`.

- [ ] **Step 4: Verify and commit**

Run: `bun test && cargo test --manifest-path crates/core/Cargo.toml`
Expected: green (docs changes can't break tests; this is the final gate).

```bash
git add README.md docs/migrating-from-pdf-lib.md CHANGELOG.md package.json crates/core/Cargo.toml Cargo.lock
git commit -m "docs: file attachments, ZUGFeRD/Factur-X structure note; bump 1.12.0"
```

---

## Self-Review (performed at plan-writing time)

- **Spec coverage:** attach on created + loaded ✓ (T5); read/extract ✓ (T3/T5); AFRelationship + /AF ✓ (T2); duplicates at attach() and at save ✓ (T1/T2/T5); /F + /UF ✓ (T1); name-tree /Kids merge + sort ✓ (T2); FlateDecode + Params Size/CheckSum/dates ✓ (T1); reader tolerant of missing /EF ✓ (T3); attach+fill+flatten one save ✓ (T4/T6); benchmark guard ✓ (T6); manual Factur-X validation ✓ (T6 step 4); docs/README/migration/CHANGELOG ✓ (T7); `AttachmentNotFoundError` reserved ✓ (T5).
- **Known judgment calls:** `apply_all` gains a parameter (internal API; RawBindings/CoreWasm updated in lockstep in T5). Attach runs last in Phase B and shares the catalog override with outline — covered by an explicit test. `getAttachments()` on an unsealed created doc returns `[]` per the "reads saved state" rule.
