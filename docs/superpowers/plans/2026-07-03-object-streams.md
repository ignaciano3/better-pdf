# PDF Object Streams (Structural Compression) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in `objectStreams` option that packs the non-stream objects of `better-pdf`'s **full-document** saves (`create()`, `merge()`/`assemble()`/`copyPages()`/`splitPages()`) into compressed PDF object streams + cross-reference streams, on top of the existing content-stream deflate.

**Architecture:** Introduce one Rust helper, `serialize_document(&mut Document, compress, object_streams)`, that is the single home for output-size policy: it runs the existing `compress_generated_streams` (content deflate) then serializes via either `Document::save_to` (default) or lopdf's `Document::save_with_options` with `use_object_streams + use_xref_streams` (when `object_streams`). The two full-document entry functions (`create_document_json`, `manipulate_pages_json`) and their wasm exports gain a trailing `object_streams: bool`; the eight incremental (loaded-document) entry functions are untouched because lopdf's `IncrementalDocument` cannot emit object streams. TypeScript surfaces it as `SaveOptions.objectStreams` (create-mode `save()`) and a new `ManipulateOptions.objectStreams` on `merge`/`assemble`/`copyPages`/`splitPages`; both default `false`.

**Tech Stack:** Rust (`lopdf 0.41`, `flate2`) → WebAssembly via `wasm-bindgen`; TypeScript API layer; `cargo test` (Rust unit) + `bun test` (TS integration); Astro docs site.

## Global Constraints

- Public API is frozen and follows SemVer; this feature is **additive only** (new optional option fields). No existing signature changes meaning.
- **No version bump.** This ships in the **same, still-unreleased 1.10.0** as the content-stream compression feature. `package.json` and `crates/core/Cargo.toml` stay `1.10.0`. Extend the existing 1.10.0 CHANGELOG entry — do **not** add a new version section.
- Default is `objectStreams: false`. With it off, byte output is identical to current `master`.
- Object streams are honored **only on full-document paths** (`create_document_json`, `manipulate_pages_json`). Passing the flag on an incremental (loaded-document) save is a documented **no-op**, never an error.
- Object streams always imply cross-reference streams — one user-facing boolean, set both lopdf options together.
- `lopdf` stays pinned at `0.41`. It re-exports `lopdf::SaveOptions` and `lopdf::SaveOptionsBuilder` at the crate root; `Document::save_with_options(&mut self, target, SaveOptions) -> std::io::Result<()>` lives on `impl Document` (full save) only.
- `object_streams` is threaded as a trailing `bool` positional parameter, mirroring how `compress` was threaded in 1.10.0.

### Shared tool: the trailing-arg transformer

Tasks 2 and 3 add a trailing `object_streams: bool` to a function that has **many** existing test callers. Appending the argument by hand is error-prone, so use this balanced-paren transformer. Save it once to `scratchpad/append_arg.py`:

```python
#!/usr/bin/env python3
"""Append a trailing argument to every CALL of a named function in a Rust file,
matching each call's balanced closing paren. Skips the `fn <name>(` definition.
Usage: append_arg.py <file> <fn_name> <arg_literal>"""
import sys
path, fn, arg = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path).read()
out, i, needle, n, count = [], 0, fn + "(", len(src), 0
while i < n:
    j = src.find(needle, i)
    if j == -1:
        out.append(src[i:]); break
    is_def = src[max(0, j - 4):j].endswith("fn ")
    open_paren = j + len(needle) - 1
    out.append(src[i:open_paren + 1])
    depth, k = 1, open_paren + 1
    while k < n and depth > 0:
        c = src[k]
        if c == '(': depth += 1
        elif c == ')':
            depth -= 1
            if depth == 0: break
        k += 1
    args = src[open_paren + 1:k]
    if is_def:
        out.append(args); out.append(')')
    else:
        stripped = args.rstrip(); trailing = args[len(stripped):]
        sep = ' ' if stripped.endswith(',') else ', '
        out.append(stripped + sep + arg + trailing); out.append(')'); count += 1
    i = k + 1
open(path, 'w').write(''.join(out))
print(f"patched {count} call(s) of {fn}")
```

---

## File Structure

**Modified files (Rust):**
- `crates/core/src/compress.rs` — add `serialize_document` helper + its unit tests (single responsibility: apply output-size policy to a full `Document`).
- `crates/core/src/create.rs` — `create_document_json` gains `object_streams`; its serialize tail calls `serialize_document`.
- `crates/core/src/pageops.rs` — `manipulate_pages_json` gains `object_streams`; its serialize tail calls `serialize_document`.
- `crates/core/src/lib.rs` — `create_document` and `manipulate_pages` wasm exports gain `object_streams`.

**Modified files (TS):**
- `src/core/wasm-bindings.ts` — `RawBindings` + `makeBindings` for `create_document` / `manipulate_pages` gain `objectStreams`.
- `src/core/document.ts` — `CoreWasm` interface; `SaveOptions.objectStreams`; new `ManipulateOptions`; `save`/`buildCreatedBytes`/`runAssemble`/`assembleImpl`/`mergeImpl`/`copyPages`/`splitPages`.
- `src/index.ts`, `src/index.browser.ts` — `merge`/`assemble` static methods gain an options param.
- `src/exports-common.ts` — export `ManipulateOptions`.

**Modified files (tests):**
- `tests/compression.test.ts` — add object-stream integration tests.

**Docs:**
- `README.md`, `docs/site/src/content/docs/guides/generating.mdx`, `docs/site/src/content/docs/reference/api.md`, `CHANGELOG.md` (extend the 1.10.0 entry).

---

### Task 1: The `serialize_document` helper

**Files:**
- Modify: `crates/core/src/compress.rs`
- Test: `crates/core/src/compress.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `compress_generated_streams` (existing), `lopdf::SaveOptions`.
- Produces: `pub fn serialize_document(doc: &mut lopdf::Document, compress: bool, object_streams: bool) -> Result<Vec<u8>, String>` — deflates content streams when `compress`, then serializes; when `object_streams`, packs non-stream objects into object streams + xref streams; otherwise a classic `save_to`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `crates/core/src/compress.rs` (the module already has `use super::*;` and `use lopdf::{Object, Stream, dictionary};`). This hand-builds a valid multi-page doc so the test has no dependency on other entry functions:

```rust
/// A valid `n`-page document: `n` page dicts + `n` content streams + a /Pages
/// node + a /Catalog. Object streams pack the non-stream dicts (pages, catalog,
/// pages-node); content streams stay direct.
fn many_page_doc(n: usize) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.add_object(dictionary! { "Type" => "Pages" });
    let mut kids = Vec::new();
    for _ in 0..n {
        let content = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b"BT ET".to_vec())));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content),
        });
        kids.push(Object::Reference(page));
    }
    let count = kids.len() as i64;
    if let Ok(p) = doc.get_object_mut(pages_id).and_then(Object::as_dict_mut) {
        p.set("Kids", Object::Array(kids));
        p.set("Count", Object::Integer(count));
    }
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog));
    doc
}

#[test]
fn serialize_document_object_streams_packs_and_roundtrips() {
    let plain = serialize_document(&mut many_page_doc(40), false, false).unwrap();
    let packed = serialize_document(&mut many_page_doc(40), false, true).unwrap();

    // Object streams appear and shrink the object-heavy document.
    assert!(
        packed.windows(6).any(|w| w == b"ObjStm"),
        "expected an /ObjStm object stream in packed output"
    );
    assert!(
        packed.len() < plain.len(),
        "packed {} should be smaller than plain {}",
        packed.len(),
        plain.len()
    );

    // Packed output is a valid PDF that round-trips with all pages intact.
    let reloaded = Document::load_mem(&packed).unwrap();
    assert_eq!(reloaded.get_pages().len(), 40);
}

#[test]
fn serialize_document_plain_has_no_object_stream() {
    let plain = serialize_document(&mut many_page_doc(5), true, false).unwrap();
    assert!(
        !plain.windows(6).any(|w| w == b"ObjStm"),
        "plain serialization must not emit object streams"
    );
    assert_eq!(Document::load_mem(&plain).unwrap().get_pages().len(), 5);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd crates/core && cargo test serialize_document`
Expected: FAIL — `serialize_document` is not defined (compile error).

- [ ] **Step 3: Implement the helper**

In `crates/core/src/compress.rs`, change the top import and add the helper below `compress_generated_streams`:

```rust
use lopdf::{Document, SaveOptions};
```

```rust
/// Serialize a freshly-built full `Document`, applying the two output-size
/// policies. `compress` deflates generated content/appearance/font stream
/// bodies (see `compress_generated_streams`). `object_streams` packs non-stream
/// objects into PDF object streams, which always imply cross-reference streams.
/// The two axes act on disjoint objects, so any combination is valid. Only
/// callable on a full `Document` — `IncrementalDocument` cannot emit object
/// streams.
pub fn serialize_document(
    doc: &mut Document,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
    if compress {
        compress_generated_streams(doc);
    }
    let mut out = Vec::new();
    if object_streams {
        let options = SaveOptions::builder()
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

- [ ] **Step 4: Run to verify it passes**

Run: `cd crates/core && cargo test serialize_document`
Expected: PASS (2 tests).

- [ ] **Step 5: Run the full Rust suite + clippy**

Run: `cd crates/core && cargo test && cargo clippy --all-targets`
Expected: all tests pass; no clippy warnings. (CI gates clippy, not fmt.)

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/compress.rs
git commit -m "feat(core): add serialize_document helper with object-stream support"
```

---

### Task 2: Wire the create (full-document) path

**Files:**
- Modify: `crates/core/src/create.rs` — `create_document_json` (signature at `:545`; serialize tail at `:742-748`)
- Modify: `crates/core/src/lib.rs` — `create_document` export (`:99`)
- Modify: `crates/core/src/fill.rs`, `crates/core/src/inject.rs` — test callers of `create_document_json`
- Test: `crates/core/src/create.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `serialize_document` (Task 1).
- Produces:
  - `pub fn create_document_json(ops_json, images, fonts, fonts_json, fields_json, compress, object_streams: bool) -> Result<Vec<u8>, String>`
  - wasm export `create_document(..., compress, object_streams: bool)`.

- [ ] **Step 1: Add the trailing param and swap in `serialize_document`**

In `crates/core/src/create.rs`, change the signature (`:545`) to add `object_streams: bool` after `compress: bool`:

```rust
pub fn create_document_json(
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    fields_json: &str,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
```

Replace the serialize tail (currently `:742-748`):

```rust
    if compress {
        crate::compress::compress_generated_streams(&mut doc);
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```

with:

```rust
    crate::compress::serialize_document(&mut doc, compress, object_streams)
}
```

- [ ] **Step 2: Update the wasm export**

In `crates/core/src/lib.rs`, update `create_document` (`:99`):

```rust
#[wasm_bindgen]
pub fn create_document(
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    fields_json: &str,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, JsError> {
    create::create_document_json(
        ops_json, images, fonts, fonts_json, fields_json, compress, object_streams,
    )
    .map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 3: Sweep every existing `create_document_json` test caller to add `false`**

Every current caller passes 6 args (the last being `false` for `compress`); they must all gain a 7th `false` for `object_streams`. Use the transformer from Global Constraints (write it to `scratchpad/append_arg.py` first):

```bash
python3 scratchpad/append_arg.py crates/core/src/create.rs create_document_json false
python3 scratchpad/append_arg.py crates/core/src/fill.rs   create_document_json false
python3 scratchpad/append_arg.py crates/core/src/inject.rs create_document_json false
```

Expected: it reports patching ~83 calls in `create.rs`, 1 in `fill.rs`, 1 in `inject.rs`. The transformer skips the `pub fn create_document_json(` definition.

- [ ] **Step 4: Write the object-streams create test**

Add to `#[cfg(test)] mod tests` in `crates/core/src/create.rs`. A **multi-page** doc gives object streams many page dicts to pack:

```rust
#[test]
fn create_document_object_streams_shrinks_multipage() {
    // 30 blank pages => 30 page dicts + a pages node + catalog to pack.
    let mut ops = String::from("[");
    for i in 0..30 {
        if i > 0 {
            ops.push(',');
        }
        ops.push_str(r#"{"op":"addPage","width":595,"height":842}"#);
    }
    ops.push(']');

    let empty = Vec::new();
    let packed = create_document_json(&ops, &empty, &empty, "[]", "[]", true, true).unwrap();
    let plain = create_document_json(&ops, &empty, &empty, "[]", "[]", true, false).unwrap();

    assert!(
        packed.len() < plain.len(),
        "object-stream output {} should be smaller than {}",
        packed.len(),
        plain.len()
    );
    assert!(
        packed.windows(6).any(|w| w == b"ObjStm"),
        "expected an /ObjStm in object-stream output"
    );
    // Round-trips through the parser with all pages intact.
    assert_eq!(Document::load_mem(&packed).unwrap().get_pages().len(), 30);
}
```

- [ ] **Step 5: Run the new test, then the full suite + clippy**

Run: `cd crates/core && cargo test create_document_object_streams_shrinks_multipage`
Expected: PASS.
Run: `cd crates/core && cargo test && cargo clippy --all-targets`
Expected: all pass; no clippy warnings. (`cargo build` first if you want the caller-sweep errors surfaced early — expected: none remain.)

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/create.rs crates/core/src/lib.rs crates/core/src/fill.rs crates/core/src/inject.rs
git commit -m "feat(core): object-stream option on the create path"
```

---

### Task 3: Wire the merge (full-document) path

**Files:**
- Modify: `crates/core/src/pageops.rs` — `manipulate_pages_json` (signature at `:255`; serialize tail at `:373-379`)
- Modify: `crates/core/src/lib.rs` — `manipulate_pages` export (`:131`)
- Test: `crates/core/src/pageops.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `serialize_document` (Task 1).
- Produces:
  - `pub fn manipulate_pages_json(docs_blob, docs_json, plan_json, compress, object_streams: bool) -> Result<Vec<u8>, String>`
  - wasm export `manipulate_pages(..., compress, object_streams: bool)`.

- [ ] **Step 1: Add the trailing param and swap in `serialize_document`**

In `crates/core/src/pageops.rs`, change the signature (`:255`):

```rust
pub fn manipulate_pages_json(
    docs_blob: &[u8],
    docs_json: &str,
    plan_json: &str,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
```

Replace the serialize tail (currently `:373-379`, note `merged.prune_objects();` precedes it and stays):

```rust
    if compress {
        crate::compress::compress_generated_streams(&mut merged);
    }

    let mut out = Vec::new();
    merged.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```

with:

```rust
    crate::compress::serialize_document(&mut merged, compress, object_streams)
}
```

- [ ] **Step 2: Update the wasm export**

In `crates/core/src/lib.rs`, update `manipulate_pages` (`:131`):

```rust
#[wasm_bindgen]
pub fn manipulate_pages(
    docs_blob: &[u8],
    docs_json: &str,
    plan_json: &str,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, JsError> {
    pageops::manipulate_pages_json(docs_blob, docs_json, plan_json, compress, object_streams)
        .map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 3: Sweep every existing `manipulate_pages_json` test caller to add `false`**

```bash
python3 scratchpad/append_arg.py crates/core/src/pageops.rs manipulate_pages_json false
```

Expected: patches ~10 calls (all currently ending in `false)` for `compress`); the definition is skipped.

- [ ] **Step 4: Write the object-streams merge test**

Add to `#[cfg(test)] mod tests` in `crates/core/src/pageops.rs`. The module already defines a `FICHA` fixture const and builds `docs_blob`/`docs_json`/`plan_json` in neighboring tests — mirror the smallest existing merge test's setup for the blob/table/plan, then:

```rust
#[test]
fn manipulate_pages_object_streams_shrinks_merge() {
    // Merge two copies of the FICHA form (many field/annotation dicts to pack).
    // Build the concatenated blob + offset table exactly as the existing merge
    // tests do (two entries, both FICHA), and a plan selecting every page of
    // both docs. Reuse whatever helper/inline shape the neighbouring merge test
    // uses; the assertion below is size + /ObjStm + round-trip, not exact bytes.
    let blob = [FICHA, FICHA].concat();
    let docs_json = format!(
        r#"[{{"offset":0,"length":{}}},{{"offset":{},"length":{}}}]"#,
        FICHA.len(),
        FICHA.len(),
        FICHA.len()
    );
    // Select page 0 of each doc (both fixtures have at least one page).
    let plan_json = r#"[{"doc":0,"page":0},{"doc":1,"page":0}]"#;

    let packed =
        manipulate_pages_json(&blob, &docs_json, plan_json, true, true).unwrap();
    let plain =
        manipulate_pages_json(&blob, &docs_json, plan_json, true, false).unwrap();

    assert!(
        packed.len() < plain.len(),
        "object-stream merge {} should be smaller than {}",
        packed.len(),
        plain.len()
    );
    assert!(
        packed.windows(6).any(|w| w == b"ObjStm"),
        "expected an /ObjStm in object-stream merge output"
    );
    assert_eq!(Document::load_mem(&packed).unwrap().get_pages().len(), 2);
}
```

Before running, open `pageops.rs` and confirm: (a) the `FICHA` const name and that it is imported in the test module (if the merge tests use a different fixture const, use that one); (b) `Document` is imported in the test module (add `use lopdf::Document;` if not). Adjust the blob/table/plan construction to match the exact shape the existing merge test uses if it differs.

- [ ] **Step 5: Run the new test, then the full suite + clippy**

Run: `cd crates/core && cargo test manipulate_pages_object_streams_shrinks_merge`
Expected: PASS.
Run: `cd crates/core && cargo test && cargo clippy --all-targets`
Expected: all pass; no clippy warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/pageops.rs crates/core/src/lib.rs
git commit -m "feat(core): object-stream option on the merge path"
```

---

### Task 4: Rebuild the wasm artifact and thread `objectStreams` through the TS bindings

**Files:**
- Build: `bun run build:wasm`
- Modify: `src/core/wasm-bindings.ts` (`RawBindings` + `makeBindings`)
- Modify: `src/core/document.ts` (`CoreWasm` interface only, `:125` and `:138`)

**Interfaces:**
- Consumes: rebuilt wasm exports `create_document(..., compress, objectStreams)` and `manipulate_pages(..., compress, objectStreams)`.
- Produces: TS binding wrappers accept and forward `objectStreams`:
  - `createDocument(opsJson, images, fonts, fontsJson, fieldsJson, compress, objectStreams)`
  - `manipulatePages(docsBlob, docsJson, planJson, compress, objectStreams)`

- [ ] **Step 1: Rebuild the wasm bindings**

Run: `bun run build:wasm`
Then confirm the generated declaration shows the new params:
Run: `grep -nE "create_document|manipulate_pages" pkg-web/better_pdf_core.d.ts`
Expected: both now end with `compress: boolean, object_streams: boolean): Uint8Array`.

- [ ] **Step 2: Update `RawBindings` in `src/core/wasm-bindings.ts`**

Change the `create_document` and `manipulate_pages` members (currently ending at `:32` and `:45`) to add the trailing arg:

```ts
  create_document(
    opsJson: string,
    images: Uint8Array,
    fonts: Uint8Array,
    fontsJson: string,
    fieldsJson: string,
    compress: boolean,
    objectStreams: boolean,
  ): Uint8Array;
```

```ts
  manipulate_pages(
    docsBlob: Uint8Array,
    docsJson: string,
    planJson: string,
    compress: boolean,
    objectStreams: boolean,
  ): Uint8Array;
```

- [ ] **Step 3: Update the `makeBindings` wrappers in `src/core/wasm-bindings.ts`**

Replace the `createDocument` wrapper (`:88-95`) and `manipulatePages` wrapper (`:103-104`):

```ts
    createDocument: (
      opsJson,
      images = EMPTY,
      fonts = EMPTY,
      fontsJson = "[]",
      fieldsJson = "[]",
      compress = true,
      objectStreams = false,
    ) =>
      (guard(),
      raw.create_document(opsJson, images, fonts, fontsJson, fieldsJson, compress, objectStreams)),
```

```ts
    manipulatePages: (docsBlob, docsJson, planJson, compress = true, objectStreams = false) =>
      (guard(), raw.manipulate_pages(docsBlob, docsJson, planJson, compress, objectStreams)),
```

- [ ] **Step 4: Update the `CoreWasm` interface in `src/core/document.ts`**

Add the trailing optional param to `createDocument` (`:125`) and `manipulatePages` (`:138`):

```ts
  createDocument(
    opsJson: string,
    images?: Uint8Array,
    fonts?: Uint8Array,
    fontsJson?: string,
    fieldsJson?: string,
    compress?: boolean,
    objectStreams?: boolean,
  ): Uint8Array;
```

```ts
  manipulatePages(
    docsBlob: Uint8Array,
    docsJson: string,
    planJson: string,
    compress?: boolean,
    objectStreams?: boolean,
  ): Uint8Array;
```

- [ ] **Step 5: Type-check**

Run: `bunx tsc --noEmit -p tsconfig.json`
Expected: no errors (the new params are optional with defaults, so existing call sites still type-check).

- [ ] **Step 6: Commit**

```bash
git add pkg-web src/core/wasm-bindings.ts src/core/document.ts
git commit -m "build(wasm): thread objectStreams through create/manipulate bindings"
```

---

### Task 5: Public `objectStreams` option + integration tests

**Files:**
- Modify: `src/core/document.ts` — `SaveOptions` (`:164`), `save` (`:206`), `buildCreatedBytes` (`:315`), `runAssemble` (`:798`), `assembleImpl` (`:838`), `mergeImpl` (`:847`), `copyPages` (`:758`), `splitPages` (`:774`); add `ManipulateOptions`
- Modify: `src/index.ts` (`assemble` `:80`, `merge` `:98`), `src/index.browser.ts` (`assemble` `:71`, `merge` `:80`)
- Modify: `src/exports-common.ts` — export `ManipulateOptions`
- Test: `tests/compression.test.ts`

**Interfaces:**
- Consumes: the Task 4 bindings.
- Produces:
  - `SaveOptions.objectStreams?: boolean` (default false); honored only in create mode.
  - `export interface ManipulateOptions { objectStreams?: boolean }`.
  - `merge(docs, options?)`, `assemble(docs, selections, options?)`, `copyPages(indices, options?)`, `splitPages(options?)`.

- [ ] **Step 1: Write the failing integration tests**

Append to `tests/compression.test.ts`:

```ts
describe("object streams", () => {
  async function manyPages(objectStreams: boolean) {
    const doc = await PdfDocument.create();
    for (let p = 0; p < 30; p++) {
      const page = doc.addPage();
      page.drawText("page " + p, { x: 40, y: 700, size: 12 });
    }
    return doc.save({ objectStreams });
  }

  test("create: objectStreams shrinks a multi-page doc and reloads", async () => {
    const packed = await manyPages(true);
    const plain = await manyPages(false);
    expect(packed.length).toBeLessThan(plain.length);
    expect(new TextDecoder().decode(packed.slice(0, 5))).toBe("%PDF-");
    const reloaded = await PdfDocument.load(packed);
    expect(reloaded.getPageCount()).toBe(30);
  });

  test("default is off (byte-identical to no option)", async () => {
    const a = await manyPages(false);
    const doc = await PdfDocument.create();
    for (let p = 0; p < 30; p++) doc.addPage().drawText("page " + p, { x: 40, y: 700, size: 12 });
    const b = await doc.save();
    expect(b.length).toBe(a.length);
  });

  test("merge: objectStreams shrinks and reloads", async () => {
    const base = await (async () => {
      const d = await PdfDocument.create();
      for (let p = 0; p < 10; p++) d.addPage().drawText("x", { x: 10, y: 10, size: 8 });
      return d.save();
    })();
    const packed = await PdfDocument.merge([base, base], { objectStreams: true });
    const plain = await PdfDocument.merge([base, base]);
    expect(packed.length).toBeLessThan(plain.length);
    const reloaded = await PdfDocument.load(packed);
    expect(reloaded.getPageCount()).toBe(20);
  });

  test("objectStreams is a no-op on loaded-document (incremental) save", async () => {
    const base = await (await PdfDocument.create()).save();
    async function stamp(objectStreams: boolean) {
      const doc = await PdfDocument.load(base);
      doc.getPage(0).drawText("stamp", { x: 50, y: 50, size: 12 });
      return doc.save({ objectStreams });
    }
    const withFlag = await stamp(true);
    const without = await stamp(false);
    expect(withFlag.length).toBe(without.length);
  });
});
```

Note: `PdfDocument.create()` with 0 pages then `save()` produces a valid base PDF; if `addPage()` returns the page handle in this codebase (it does), the chained `.drawText` is fine.

- [ ] **Step 2: Run to verify it fails**

Run: `bun test tests/compression.test.ts`
Expected: FAIL — `save`/`merge` don't accept `objectStreams` yet (type error or the size assertions fail because the option is ignored).

- [ ] **Step 3: Add `objectStreams` to `SaveOptions` and thread it into create-mode save**

In `src/core/document.ts`, extend `SaveOptions` (`:164`):

```ts
export interface SaveOptions {
  /** Deflate-compress generated streams. Defaults to `true`. */
  compress?: boolean;
  /**
   * Pack non-stream objects into PDF object streams (+ cross-reference streams)
   * for smaller output. Defaults to `false`. Honored only for documents created
   * with `PdfDocument.create()`; ignored for incremental (loaded-document) saves.
   */
  objectStreams?: boolean;
}
```

In `save` (`:206`), read the flag and pass it to `buildCreatedBytes`:

```ts
  async save(options: SaveOptions = {}): Promise<Uint8Array> {
    const compress = options.compress ?? true;
    const objectStreams = options.objectStreams ?? false;

    if (this.mode === "create" && !this.sealed) {
      try {
        return this.buildCreatedBytes(compress, objectStreams);
      } catch (e) {
        throw toPdfError(e);
      }
    }
    // ... rest unchanged (incremental paths ignore objectStreams) ...
```

Update `buildCreatedBytes` (`:315`) to accept and forward it:

```ts
  private buildCreatedBytes(compress = true, objectStreams = false): Uint8Array {
    if (this.meta.dirty) {
      this.drawQueue.pushMetadata(this.meta.wire);
    }
    if (this.outlineItems !== undefined) {
      this.drawQueue.pushOutline(this.outlineItems);
    }
    const { opsJson, images, fonts, fontsJson } = this.drawQueue.toCreatePayload();
    return this.wasm.createDocument(
      opsJson,
      images,
      fonts,
      fontsJson,
      JSON.stringify(this.fieldDefs),
      compress,
      objectStreams,
    );
  }
```

(The other `buildCreatedBytes()` caller — `materializeCreatedForm` at `:692` — keeps calling it with no args, correctly defaulting to `objectStreams = false`.)

- [ ] **Step 4: Add `ManipulateOptions` and thread it through the assemble/merge/copy/split paths**

In `src/core/document.ts`, add the type near `SaveOptions`:

```ts
/** Options for the full-document assembly operations (merge/assemble/copy/split). */
export interface ManipulateOptions {
  /**
   * Pack non-stream objects into PDF object streams (+ cross-reference streams)
   * for smaller output. Defaults to `false`.
   */
  objectStreams?: boolean;
}
```

Update `runAssemble` (`:798`) to accept and forward the flag (compress stays `true` as today via the wrapper default — pass it explicitly):

```ts
  protected static runAssemble(
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
    wasmBinding: CoreWasm,
    objectStreams = false,
  ): Uint8Array {
    // ... blob/table/plan construction unchanged ...
    return callBytes(() =>
      wasmBinding.manipulatePages(blob, docsJson, planJson, true, objectStreams),
    );
  }
```

Update `assembleImpl` (`:838`) and `mergeImpl` (`:847`):

```ts
  protected static assembleImpl(
    wasmBinding: CoreWasm,
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
    objectStreams = false,
  ): Uint8Array {
    return PdfDocumentBase.runAssemble(docs, selections, wasmBinding, objectStreams);
  }

  protected static mergeImpl(
    wasmBinding: CoreWasm,
    docs: Uint8Array[],
    objectStreams = false,
  ): Uint8Array {
    const selections: { docIndex: number; pageIndex: number }[] = [];
    for (let docIndex = 0; docIndex < docs.length; docIndex++) {
      const pageInfos = callJson<PageInfo[]>(() => wasmBinding.readPages(docs[docIndex]!));
      // ... existing loop body unchanged ...
    }
    return PdfDocumentBase.runAssemble(docs, selections, wasmBinding, objectStreams);
  }
```

Update `copyPages` (`:758`) and `splitPages` (`:774`) to accept options:

```ts
  async copyPages(indices: number[], options: ManipulateOptions = {}): Promise<Uint8Array> {
    if (this.mode !== "load") {
      throw new PdfError("copyPages is only available on documents opened with PdfDocument.load()");
    }
    const selections = indices.map((i) => ({ docIndex: 0, pageIndex: i }));
    return PdfDocumentBase.runAssemble([this.bytes], selections, this.wasm, options.objectStreams ?? false);
  }
```

```ts
  async splitPages(options: ManipulateOptions = {}): Promise<Uint8Array[]> {
    if (this.mode !== "load") {
      throw new PdfError(
        "splitPages is only available on documents opened with PdfDocument.load()",
      );
    }
    const objectStreams = options.objectStreams ?? false;
    const count = this.getPageCount();
    const results: Uint8Array[] = [];
    for (let i = 0; i < count; i++) {
      results.push(
        await PdfDocumentBase.runAssemble(
          [this.bytes],
          [{ docIndex: 0, pageIndex: i }],
          this.wasm,
          objectStreams,
        ),
      );
    }
    return results;
  }
```

- [ ] **Step 5: Thread the option through the `merge`/`assemble` static methods in both entry barrels**

In `src/index.ts`, update `assemble` (`:80`) and `merge` (`:98`):

```ts
  static async assemble(
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
    options?: ManipulateOptions,
  ): Promise<Uint8Array> {
    return PdfDocumentBase.assembleImpl(wasm, docs, selections, options?.objectStreams ?? false);
  }
```

```ts
  static async merge(docs: Uint8Array[], options?: ManipulateOptions): Promise<Uint8Array> {
    return PdfDocumentBase.mergeImpl(wasm, docs, options?.objectStreams ?? false);
  }
```

Add `ManipulateOptions` to the existing import from `./core/document.js` at the top of `src/index.ts` (it already imports `PdfDocumentBase` from there — extend that import, or add `import type { ManipulateOptions } from "./core/document.js";`).

Apply the identical edits in `src/index.browser.ts` (`assemble` `:71`, `merge` `:80`, and the type import).

- [ ] **Step 6: Export `ManipulateOptions`**

In `src/exports-common.ts`, next to the existing `SaveOptions` export, add:

```ts
export type { ManipulateOptions } from "./core/document.js";
```

- [ ] **Step 7: Run the compression test file**

Run: `bun test tests/compression.test.ts`
Expected: PASS (the new `object streams` describe block plus the existing `stream compression` block).

- [ ] **Step 8: Type-check and run the whole suite**

Run: `bunx tsc --noEmit -p tsconfig.json && bun test`
Expected: no type errors; all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/core/document.ts src/index.ts src/index.browser.ts src/exports-common.ts tests/compression.test.ts
git commit -m "feat: add objectStreams option to save() and merge/assemble/copy/split"
```

---

### Task 6: Documentation (fold into the 1.10.0 release)

**Files:**
- Modify: `README.md`
- Modify: `docs/site/src/content/docs/guides/generating.mdx`
- Modify: `docs/site/src/content/docs/reference/api.md`
- Modify: `CHANGELOG.md` (extend the existing 1.10.0 entry — **no** new version section, **no** version bump)

**Interfaces:**
- Consumes: nothing.
- Produces: user-facing docs for `objectStreams`.

- [ ] **Step 1: README — extend the Compression Features bullet**

In `README.md`, immediately after the existing compression Features bullet ("Deflate-compress generated content, appearance, and font streams on save …"), add:

```markdown
- Optionally pack non-stream objects into PDF object streams for even smaller files on full-document saves — opt in with `doc.save({ objectStreams: true })` (created docs) or `PdfDocument.merge(docs, { objectStreams: true })`. Off by default; not applied to incremental (loaded-document) saves.
```

- [ ] **Step 2: README — extend the Compression usage section**

In `README.md`, at the end of the `### Compression` section (after the `compress: false` example), append:

````markdown
For created and merged documents you can also pack the object structure into
object streams (PDF 1.5+), shrinking object-heavy files further:

```ts
const doc = await PdfDocument.create();
// ... add pages / fields ...
const small = await doc.save({ objectStreams: true });      // + object streams
const merged = await PdfDocument.merge([a, b], { objectStreams: true });
```

`objectStreams` defaults to `false`. It applies only to full-document saves
(`create()`, `merge`, `assemble`, `copyPages`, `splitPages`); it is ignored on
incremental (loaded-document) saves, which stay append-only.
````

- [ ] **Step 3: docs site — generating guide**

In `docs/site/src/content/docs/guides/generating.mdx`, at the end of the `## Compression` section, append:

````markdown
### Object streams

For created and merged documents, `objectStreams` additionally packs the PDF's
object structure into compressed object streams (PDF 1.5+) — a further size win
on object-heavy files (large forms, many pages, big merges):

```ts
const small = await doc.save({ objectStreams: true });
const merged = await PdfDocument.merge([a, b], { objectStreams: true });
```

It is **off by default** and applies only to full-document saves — `create()`,
`merge`, `assemble`, `copyPages`, `splitPages`. Incremental (loaded-document)
saves ignore it and remain append-only, so existing signatures stay valid.
````

- [ ] **Step 4: docs site — API reference page**

In `docs/site/src/content/docs/reference/api.md`, update the `doc.save(...)` line to mention the new option, and add a note. Replace:

```markdown
- `doc.save(options?: SaveOptions): Promise<Uint8Array>` — `SaveOptions = { compress?: boolean }`; `compress` defaults to `true`
```

with:

```markdown
- `doc.save(options?: SaveOptions): Promise<Uint8Array>` — `SaveOptions = { compress?: boolean; objectStreams?: boolean }`; `compress` defaults to `true`, `objectStreams` to `false` (full-document/created saves only)
```

Then, in the same file, update the merge/assemble/copyPages/splitPages entries (search for `merge(`, `assemble(`, `copyPages(`, `splitPages(`) to show the optional `options?: ManipulateOptions` (`{ objectStreams?: boolean }`) parameter. If those methods are not individually listed, add one line documenting `ManipulateOptions = { objectStreams?: boolean }` and that it is accepted by `merge`/`assemble`/`copyPages`/`splitPages`.

- [ ] **Step 5: CHANGELOG — extend the existing 1.10.0 entry**

In `CHANGELOG.md`, inside the existing `## [1.10.0] - 2026-07-02` → `### Added` section (do **not** create a new version heading), add a new bullet after the stream-compression bullet:

```markdown
- **Object streams (opt-in structural compression).** On full-document saves you
  can now pack non-stream objects into PDF object streams + cross-reference
  streams for smaller output: `doc.save({ objectStreams: true })` (created
  documents) and `PdfDocument.merge` / `assemble` / `copyPages` / `splitPages`
  via a new `ManipulateOptions` (`{ objectStreams?: boolean }`). New
  `SaveOptions.objectStreams`, default `false`.
  - Applies only to full-document paths (create/merge/assemble/copyPages/splitPages).
    Incremental (loaded-document) saves ignore the flag and remain append-only.
  - Object streams require and enable cross-reference streams and raise the
    output to PDF 1.5+. The result is **not** PDF/A-1 conformant; leave the option
    off (the default) if you need PDF/A-1 or maximum consumer compatibility.
```

- [ ] **Step 6: Verify the docs site builds**

Run: `cd docs/site && (test -d node_modules || bun install) && bun run build`
Expected: build completes; the generated API reference picks up `ManipulateOptions` / `objectStreams` from source, and the changelog page reflects the extended 1.10.0 entry.

Confirm: `grep -rl "objectStreams" docs/site/src/content/docs/api-reference/ 2>/dev/null` returns at least one file (the generated `SaveOptions` / `ManipulateOptions` interface pages). Note the generated `api-reference/` dir and `reference/changelog.md` are gitignored — do not commit them.

- [ ] **Step 7: Commit**

```bash
git add README.md CHANGELOG.md docs/site/src/content/docs/guides/generating.mdx docs/site/src/content/docs/reference/api.md
git commit -m "docs: document objectStreams option (folds into 1.10.0)"
```

---

## Self-Review

**Spec coverage:**
- `serialize_document` helper (content deflate + object/xref streams, single policy home) → Task 1. ✓
- Object streams on create path, opt-in → Task 2. ✓
- Object streams on merge/assemble/copyPages/splitPages path (shared `manipulate_pages`) → Task 3 (Rust) + Task 5 (TS surface). ✓
- wasm rebuild + binding thread-through, create + manipulate only → Task 4. ✓
- Public `SaveOptions.objectStreams` (create-mode) + `ManipulateOptions` type + all four assembly methods + no-op on incremental → Task 5. ✓
- Default off / byte-identical when off → asserted in Task 5 Step 1 ("default is off"). ✓
- Object streams imply xref streams (one boolean) → Task 1 helper sets both. ✓
- Incremental paths untouched → only `create_document_json` / `manipulate_pages_json` changed; the eight incremental entry fns are never edited. ✓
- Docs + extend 1.10.0 CHANGELOG + **no version bump** → Task 6 (and Global Constraints). ✓
- lopdf pinned, `SaveOptions` via crate root → Global Constraints + Task 1. ✓

**Type consistency:** trailing `object_streams: bool` (Rust) / `objectStreams?: boolean` (TS) added consistently across the two full-doc entry fns, their wasm exports, `RawBindings`, `makeBindings`, `CoreWasm`, `SaveOptions`, `ManipulateOptions`, and the four assembly methods. `serialize_document(&mut doc, compress, object_streams)` used identically at both full-doc save sites. `ManipulateOptions` defined once in `document.ts`, exported from `exports-common.ts`, imported by both entry barrels.

**Out of scope (unchanged from spec):** linearization; object streams on incremental/loaded-document saves (structurally impossible); changing the default; PDF/A conformance modes.
