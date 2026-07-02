# PDF Stream Compression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deflate-compress the content/appearance/font streams `better-pdf` emits, cutting output size, controllable via `doc.save({ compress })` (default `true`).

**Architecture:** Add one internal Rust helper, `compress_generated_streams(&mut Document)`, that walks a freshly-built `Document` and FlateDecode-compresses every stream that both permits compression and has no existing `/Filter`. Call it at each production serialization site — before `save_to` for full-document paths and before `IncrementalDocument::create_from` for incremental paths — gated by a `compress: bool` threaded from the wasm boundary. TypeScript `save()` gains an optional `{ compress }` option (default `true`) plumbed through `applyAll` / `createDocument` / the chained fallback. Images (already `FlateDecode`) are skipped by the per-stream guard, so there is no double-compression.

**Tech Stack:** Rust (`lopdf 0.41`, `flate2`), compiled to WebAssembly via `wasm-bindgen`; TypeScript API layer; `cargo test` (Rust unit) + `bun test` (TS integration).

## Global Constraints

- Public API is frozen as of 1.0.0; follows SemVer. This feature is **additive only** — an optional options object on `save()`. No existing signature may change meaning. Ship as a **minor** version bump.
- Default behavior is `compress: true`. Output bytes will change vs. pre-feature; that is expected and is **not** an API break (byte output is not part of the SemVer contract).
- `lopdf` is pinned at `0.41` with `default-features = false` (native) and `features = ["wasm_js"]` (wasm32). Do not change the dependency.
- Compression must be **lossless and structural only**: deflate of stream bodies. No image resampling, no lossy re-encoding, no object-stream/xref-stream rewriting in this plan.
- Never recompress or mutate objects that already carry a `/Filter` (fonts’ FontFile, image XObjects). The per-stream guard enforces this.
- Incremental (loaded-document) paths are append-only: only compress the **newly built** `Document`, never the original bytes.

---

## File Structure

**New files:**
- `crates/core/src/compress.rs` — the `compress_generated_streams` helper + its Rust unit tests. Single responsibility: given a `&mut Document`, compress eligible generated streams.
- `tests/compression.test.ts` — TS integration tests: round-trip fidelity and size reduction across create, draw, and fill flows, plus the `compress: false` opt-out.

**Modified files (Rust — wire the helper + thread the flag):**
- `crates/core/src/lib.rs` — declare `mod compress;`; add `compress: bool` params to the affected `#[wasm_bindgen]` exports.
- `crates/core/src/create.rs:742` — full-document path; compress before `save_to`.
- `crates/core/src/apply.rs:104` — incremental fast path; compress the new `Document` before `create_from`.
- `crates/core/src/draw.rs:988`, `crates/core/src/fill.rs:70`, `crates/core/src/flatten.rs:34`, `crates/core/src/inject.rs:106`, `crates/core/src/outline.rs:141`, `crates/core/src/metadata.rs:123`, `crates/core/src/pagetree.rs:181` — incremental paths; compress before `create_from`.
- `crates/core/src/pageops.rs:373` — full-document merge path; compress before `save_to`.

**Modified files (TS — surface the option):**
- `src/core/wasm-bindings.ts` / `src/core/wasm-browser.ts` — pass the `compress` arg through to the raw wasm exports.
- `src/core/wasm.ts` — update the binding type signatures.
- `src/core/document.ts:197` — `save(options?: SaveOptions)`; thread `compress` into `applyAll`, `buildCreatedBytes`, and `saveChained`.
- `src/index.ts` / `src/index.browser.ts` — export the `SaveOptions` type.

**Docs:**
- `README.md` — add a compression bullet + `save({ compress })` note.
- `CHANGELOG.md` — new minor-version entry.

---

## Design Notes (read before Task 1)

`lopdf` mechanics, verified in `lopdf-0.41.0/src`:

- `Document::compress()` (`processor.rs:22`) iterates all objects; for each `Object::Stream` where `stream.allows_compression == true`, calls `stream.compress()`.
- `Stream::compress()` (`object.rs:777`): **no-op if `/Filter` already set**; else zlib-encodes, and **keeps the result only if it actually shrinks** (`compressed.len() + 19 < original.len()`), setting `/Filter = FlateDecode`. Idempotent and self-guarding.
- `Stream::with_compression(bool)` (`object.rs:738`) sets the `allows_compression` flag; default on `Stream::new` is `true`.
- `save_to` does **not** call `compress()` — nothing compresses today unless we call it.

Our helper is a thin, intention-revealing wrapper over `Document::compress()` so all call sites read identically and future policy (e.g. skipping specific stream subtypes) has one home. Current `.with_compression(false)` calls in `create.rs`, `appearance.rs`, `flatten.rs` set `allows_compression = false`, which would make our helper skip those exact content/appearance streams — so those calls must be dropped (Task 2) for the feature to reach page content.

---

### Task 1: The `compress_generated_streams` helper

**Files:**
- Create: `crates/core/src/compress.rs`
- Modify: `crates/core/src/lib.rs` (add `mod compress;` near the other `mod` declarations, e.g. after `mod apply;`)
- Test: `crates/core/src/compress.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: `lopdf::Document`.
- Produces: `pub fn compress_generated_streams(doc: &mut lopdf::Document)` — compresses every eligible stream in-place. Eligible = `allows_compression == true` AND no existing `/Filter`. Idempotent.

- [ ] **Step 1: Write the failing test**

In `crates/core/src/compress.rs`:

```rust
use lopdf::{Document, Object, Stream, dictionary};

/// Compress every generated stream in `doc` that permits compression and is
/// not already filtered. Delegates to lopdf's per-stream guard, so streams
/// with an existing `/Filter` (fonts, images) and streams that would not
/// shrink are left untouched. Idempotent.
pub fn compress_generated_streams(doc: &mut Document) {
    doc.compress();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highly_compressible() -> Vec<u8> {
        vec![b'A'; 4096]
    }

    #[test]
    fn compresses_unfiltered_stream() {
        let mut doc = Document::with_version("1.7");
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            highly_compressible(),
        )));
        compress_generated_streams(&mut doc);
        let stream = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("expected stream"),
        };
        assert_eq!(
            stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"FlateDecode"
        );
        assert!(stream.content.len() < 4096);
    }

    #[test]
    fn skips_already_filtered_stream() {
        let mut doc = Document::with_version("1.7");
        let mut dict = dictionary! {};
        dict.set("Filter", "FlateDecode");
        let original = vec![1u8, 2, 3, 4];
        let id = doc.add_object(Object::Stream(Stream::new(dict, original.clone())));
        compress_generated_streams(&mut doc);
        let stream = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("expected stream"),
        };
        assert_eq!(stream.content, original, "filtered stream must be untouched");
    }

    #[test]
    fn skips_stream_with_compression_disabled() {
        let mut doc = Document::with_version("1.7");
        let id = doc.add_object(Object::Stream(
            Stream::new(dictionary! {}, highly_compressible()).with_compression(false),
        ));
        compress_generated_streams(&mut doc);
        let stream = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("expected stream"),
        };
        assert!(stream.dict.get(b"Filter").is_err(), "must stay uncompressed");
    }

    #[test]
    fn idempotent() {
        let mut doc = Document::with_version("1.7");
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            highly_compressible(),
        )));
        compress_generated_streams(&mut doc);
        let after_first = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s.content.clone(),
            _ => panic!(),
        };
        compress_generated_streams(&mut doc);
        let after_second = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s.content.clone(),
            _ => panic!(),
        };
        assert_eq!(after_first, after_second);
    }
}
```

- [ ] **Step 2: Wire the module**

In `crates/core/src/lib.rs`, add alongside the existing `mod` declarations:

```rust
mod compress;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd crates/core && cargo test compress::tests`
Expected: 4 passed. (The helper body is written in Step 1; these are characterization tests confirming lopdf semantics through our wrapper.)

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/compress.rs crates/core/src/lib.rs
git commit -m "feat(core): add compress_generated_streams helper"
```

---

### Task 2: Enable compression on generated content/appearance streams

Drop the `.with_compression(false)` calls so page-content and appearance streams become eligible for the Task 1 helper. These were vestigial (nothing called `compress()`), so removing them changes nothing until the helper is wired in Task 3 — but do it as its own reviewable step.

**Files:**
- Modify: `crates/core/src/create.rs:407`
- Modify: `crates/core/src/appearance.rs:441`, `:583`, `:968`
- Modify: `crates/core/src/flatten.rs:194`
- Test: existing suites must stay green.

**Interfaces:**
- Consumes: nothing new.
- Produces: content/appearance streams now default to `allows_compression == true`.

- [ ] **Step 1: Verify current call sites**

Run: `cd crates/core && grep -rn "with_compression(false)" src`
Expected: exactly the 5 lines listed above.

- [ ] **Step 2: Remove each `.with_compression(false)`**

For each site, drop the trailing `.with_compression(false)`. Example — `create.rs:407`:

```rust
// before
Stream::new(dict, content).with_compression(false)
// after
Stream::new(dict, content)
```

Apply the identical edit at `appearance.rs:441`, `appearance.rs:583`, `appearance.rs:968`, and `flatten.rs:194`. Do **not** touch `appearance.rs:652` (image XObject: it sets an explicit `FlateDecode` filter and uses `with_compression(false)` deliberately — the per-stream guard already skips filtered streams, but leaving the flag off is correct and avoids re-scanning it).

- [ ] **Step 3: Confirm no stray disables remain (except the intentional image one)**

Run: `cd crates/core && grep -rn "with_compression(false)" src`
Expected: only `appearance.rs:652` (the image XObject).

- [ ] **Step 4: Run the full Rust suite — nothing compresses yet, so all snapshots hold**

Run: `cd crates/core && cargo test`
Expected: all pass (no call site invokes `compress()` yet).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/create.rs crates/core/src/appearance.rs crates/core/src/flatten.rs
git commit -m "refactor(core): allow compression on generated content/appearance streams"
```

---

### Task 3: Compress in the create (full-document) path, gated by a flag

**Files:**
- Modify: `crates/core/src/create.rs` — `create_document_json` (signature at `:545`, `save_to` at `:742`)
- Modify: `crates/core/src/lib.rs` — the `#[wasm_bindgen] create_document` export (near `:87`)
- Test: `crates/core/src/create.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `compress_generated_streams` (Task 1).
- Produces:
  - `pub fn create_document_json(..., compress: bool) -> Result<Vec<u8>, String>` — the new trailing param.
  - wasm export `create_document(..., compress: bool)` — new trailing param.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `crates/core/src/create.rs`. Use a minimal single-text-page op JSON already used by neighboring tests (copy the JSON shape from `builder_writes_default_values_for_all_field_types` or the simplest existing create test; the assertion is size + filter, not exact content):

```rust
#[test]
fn create_document_compresses_content_when_enabled() {
    // Reuse the smallest valid ops JSON from an existing create test.
    let ops_json = SIMPLE_TEXT_PAGE_OPS_JSON; // hoist an existing fixture to a const
    let empty = Vec::new();
    let compressed =
        create_document_json(ops_json, &empty, &empty, "[]", "[]", true).unwrap();
    let raw =
        create_document_json(ops_json, &empty, &empty, "[]", "[]", false).unwrap();
    assert!(
        compressed.len() < raw.len(),
        "compressed {} should be smaller than raw {}",
        compressed.len(),
        raw.len()
    );
    // FlateDecode must appear in the compressed output's object stream headers.
    assert!(
        compressed.windows(11).any(|w| w == b"FlateDecode"),
        "expected a FlateDecode filter in compressed output"
    );
}
```

If no shared constant exists, define `const SIMPLE_TEXT_PAGE_OPS_JSON: &str = r#"[{ ...one create-page-with-text op... }]"#;` at the top of the test module by copying a known-good JSON already exercised in this file. (Match the exact `CreateOp` JSON shape — check `create.rs` op deserialization structs — so it parses.)

- [ ] **Step 2: Run to verify it fails**

Run: `cd crates/core && cargo test create_document_compresses_content_when_enabled`
Expected: FAIL — `create_document_json` does not yet take a `compress` argument (compile error).

- [ ] **Step 3: Thread the flag through `create_document_json`**

In `crates/core/src/create.rs`, change the signature (`:545`) to add a trailing `compress: bool`, and insert the compression call immediately before serialization (`:742`):

```rust
pub fn create_document_json(
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    fields_json: &str,
    compress: bool,
) -> Result<Vec<u8>, String> {
    // ... existing body unchanged, up to just before save_to ...

    if compress {
        crate::compress::compress_generated_streams(&mut doc);
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```

- [ ] **Step 4: Thread the flag through the wasm export**

In `crates/core/src/lib.rs`, update the `create_document` `#[wasm_bindgen]` function to accept `compress: bool` and forward it to `create_document_json`:

```rust
#[wasm_bindgen]
pub fn create_document(
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    fields_json: &str,
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    create::create_document_json(ops_json, images, fonts, fonts_json, fields_json, compress)
        .map_err(|e| JsError::new(&e))
}
```

(Preserve the actual existing return/error types and param names — only add the trailing `compress` param and pass it on.)

- [ ] **Step 5: Run to verify it passes**

Run: `cd crates/core && cargo test create_document_compresses_content_when_enabled`
Expected: PASS.

- [ ] **Step 6: Run the full Rust suite**

Run: `cd crates/core && cargo test`
Expected: all pass. (Other create tests call `create_document_json` — update those call sites to pass `false` so their content-inspecting assertions keep seeing plaintext.)

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/create.rs crates/core/src/lib.rs
git commit -m "feat(core): compress streams in create path behind compress flag"
```

---

### Task 4: Compress in the incremental paths, gated by the flag

All incremental emit sites share the shape: build a new `Document`, then `IncrementalDocument::create_from(orig_bytes, doc)`, then `inc.save_to(...)`. Compress the **new** `Document` right before `create_from`. This task covers the fast path (`apply.rs`) and the per-op paths.

**Files:**
- Modify: `crates/core/src/apply.rs` — `apply_all_json` (`:53`, `create_from` at `:84`)
- Modify: `crates/core/src/draw.rs:988`, `crates/core/src/fill.rs:70`, `crates/core/src/flatten.rs:34`, `crates/core/src/inject.rs:106`, `crates/core/src/outline.rs:141`, `crates/core/src/metadata.rs:123`, `crates/core/src/pagetree.rs:181`
- Modify: `crates/core/src/lib.rs` — the corresponding `#[wasm_bindgen]` exports (`apply_all`, `apply_draw_ops`, `fill_fields`, `flatten_fields`, `inject_fields`, `set_outline`, `set_metadata`, `insert_pages`)
- Test: `crates/core/src/apply.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `compress_generated_streams` (Task 1).
- Produces: each listed Rust entry function and its wasm export gains a trailing `compress: bool`. The new `Document` is compressed before `create_from` when `compress == true`.

- [ ] **Step 1: Write the failing test (fast path)**

Add to `#[cfg(test)] mod tests` in `crates/core/src/apply.rs`. Reuse the fixture the existing `apply_all_composes_draw_metadata_outline_in_one_pass` test uses (load bytes + a draw plan JSON):

```rust
#[test]
fn apply_all_compresses_drawn_content_when_enabled() {
    let base = FICHA; // existing loaded-PDF fixture used elsewhere in this module
    let plan = DRAW_TEXT_PLAN_JSON; // reuse the draw plan from the composition test
    let empty = Vec::new();
    let compressed = apply_all_json(base, plan, &empty, &empty, &empty, true).unwrap();
    let raw = apply_all_json(base, plan, &empty, &empty, &empty, false).unwrap();
    assert!(
        compressed.len() < raw.len(),
        "compressed {} should be smaller than raw {}",
        compressed.len(),
        raw.len()
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd crates/core && cargo test apply_all_compresses_drawn_content_when_enabled`
Expected: FAIL — `apply_all_json` does not yet take `compress` (compile error).

- [ ] **Step 3: Thread the flag through `apply_all_json`**

In `crates/core/src/apply.rs`, add trailing `compress: bool` to `apply_all_json` and compress the new `Document` before `create_from` (`:84`):

```rust
pub fn apply_all_json(
    data: &[u8],
    plan_json: &str,
    fill_images: &[u8],
    draw_images: &[u8],
    fonts: &[u8],
    compress: bool,
) -> Result<Vec<u8>, String> {
    // ... build `doc` (the new Document) exactly as today ...

    if compress {
        crate::compress::compress_generated_streams(&mut doc);
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```

Verify the local variable holding the new `Document` is named correctly (read the function body first — it may not be `doc`) and that it is still owned/mutable at that point.

- [ ] **Step 4: Apply the same pattern to the per-op paths**

In each of `draw.rs:988`, `fill.rs:70`, `flatten.rs:34`, `inject.rs:106`, `outline.rs:141`, `metadata.rs:123`, `pagetree.rs:181`: add a trailing `compress: bool` to the public entry function, and insert immediately before its `IncrementalDocument::create_from(...)`:

```rust
if compress {
    crate::compress::compress_generated_streams(&mut doc); // use the real local name
}
```

For each file, read the function to confirm the new-document variable name before editing.

- [ ] **Step 5: Thread the flag through every affected wasm export**

In `crates/core/src/lib.rs`, add a trailing `compress: bool` to each of `apply_all`, `apply_draw_ops`, `fill_fields`, `flatten_fields`, `inject_fields`, `set_outline`, `set_metadata`, `insert_pages`, forwarding it to the corresponding entry function. Example:

```rust
#[wasm_bindgen]
pub fn apply_all(
    data: &[u8],
    plan_json: &str,
    fill_images: &[u8],
    draw_images: &[u8],
    fonts: &[u8],
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    apply::apply_all_json(data, plan_json, fill_images, draw_images, fonts, compress)
        .map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 6: Update `pageops.rs` (full-document merge)**

`pageops.rs:373` serializes a full `merged` `Document` via `save_to`. Add `compress: bool` to its entry function (`manipulate_pages_json` or equivalent — read the file) and, before `merged.save_to`, insert:

```rust
if compress {
    crate::compress::compress_generated_streams(&mut merged);
}
```

Thread `compress` through the `manipulate_pages` wasm export in `lib.rs`.

- [ ] **Step 7: Fix all internal callers**

Run: `cd crates/core && cargo build`
Expected: compile errors listing every remaining call to the changed functions. Update each **non-test** internal caller to pass the real flag; update each **test** caller to pass `false` (so content-inspecting assertions keep seeing plaintext), except tests that specifically assert compression.

- [ ] **Step 8: Run the failing test, then the full suite**

Run: `cd crates/core && cargo test apply_all_compresses_drawn_content_when_enabled`
Expected: PASS.
Run: `cd crates/core && cargo test`
Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add crates/core/src
git commit -m "feat(core): compress streams in incremental + merge paths behind compress flag"
```

---

### Task 5: Rebuild the wasm artifact and thread `compress` through the TS bindings

**Files:**
- Build: run the project's wasm build script (see below)
- Modify: `src/core/wasm-bindings.ts` (native binding, `:62`–`:84`), `src/core/wasm-browser.ts` (mirror)
- Modify: `src/core/wasm.ts` (binding interface types)
- Test: none new here (covered by Task 6); this task ends when TS type-checks.

**Interfaces:**
- Consumes: the rebuilt wasm exports, each now taking a trailing `compress: boolean`.
- Produces: TS binding wrappers accept and forward `compress`:
  - `createDocument(opsJson, images, fonts, fontsJson, fieldsJson, compress)`
  - `applyAll(data, planJson, fillImages, drawImages, fonts, compress)`
  - `applyDrawOps(data, opsJson, images, fonts, fontsJson, compress)`
  - `fillFields(data, opsJson, images, compress)`
  - `flattenFields(data, namesJson, compress)`
  - `injectFields(data, fieldsJson, fonts, fontsJson, compress)`
  - `setOutline(data, json, compress)`
  - `setMetadata(data, metaJson, compress)`
  - `insertPages(data, opsJson, compress)`
  - `manipulatePages(docsBlob, docsJson, planJson, compress)`

- [ ] **Step 1: Find and run the wasm build**

Run: `cat package.json | grep -A2 -iE '"build|wasm|wasm-pack"'`
Then run the wasm build script it reveals (e.g. `bun run build:wasm` or the `wasm-pack`/`scripts/` invocation). Confirm the generated `.d.ts` (under `pkg-web/` or `crates/*/pkg`) now shows the `compress` params.

Expected: regenerated bindings include `compress: boolean` on the listed exports.

- [ ] **Step 2: Update the binding wrappers**

In `src/core/wasm-bindings.ts`, add `compress` to each wrapper and forward it. Example edits:

```ts
createDocument: (opsJson, images, fonts, fontsJson, fieldsJson, compress) =>
  (guard(), raw.create_document(opsJson, images, fonts, fontsJson, fieldsJson, compress)),
applyAll: (data, planJson, fillImages, drawImages, fonts, compress) =>
  (guard(), raw.apply_all(data, planJson, fillImages, drawImages, fonts, compress)),
fillFields: (data, opsJson, images, compress) =>
  (guard(), raw.fill_fields(data, opsJson, images, compress)),
flattenFields: (data, namesJson, compress) =>
  (guard(), raw.flatten_fields(data, namesJson, compress)),
injectFields: (data, fieldsJson, fonts = EMPTY, fontsJson = "[]", compress = true) =>
  (guard(), raw.inject_fields(data, fieldsJson, fonts, fontsJson, compress)),
setOutline: (data, json, compress) => (guard(), raw.set_outline(data, json, compress)),
setMetadata: (data, metaJson, compress) => (guard(), raw.set_metadata(data, metaJson, compress)),
insertPages: (data, opsJson, compress) => (guard(), raw.insert_pages(data, opsJson, compress)),
manipulatePages: (docsBlob, docsJson, planJson, compress) =>
  (guard(), raw.manipulate_pages(docsBlob, docsJson, planJson, compress)),
applyDrawOps: (data, opsJson, images, fonts, fontsJson, compress) =>
  (guard(), raw.apply_draw_ops(data, opsJson, images, fonts, fontsJson, compress)),
```

Mirror every change in `src/core/wasm-browser.ts`.

- [ ] **Step 3: Update the binding interface types**

In `src/core/wasm.ts`, add `compress: boolean` to the corresponding method signatures on the bindings interface so callers type-check.

- [ ] **Step 4: Type-check**

Run: `bunx tsc --noEmit -p tsconfig.json`
Expected: errors only at `document.ts` call sites (fixed in Task 6) — no errors inside the binding files.

- [ ] **Step 5: Commit**

```bash
git add pkg-web crates/core/pkg src/core/wasm-bindings.ts src/core/wasm-browser.ts src/core/wasm.ts
git commit -m "build(wasm): thread compress flag through wasm bindings"
```

---

### Task 6: Public `save({ compress })` option + integration tests

**Files:**
- Modify: `src/core/document.ts` — `save()` (`:197`), `buildCreatedBytes`, `saveChained` (`:260`), `injectPendingFields` (`:693`)
- Modify: `src/index.ts`, `src/index.browser.ts` — export `SaveOptions`
- Test: `tests/compression.test.ts` (new)

**Interfaces:**
- Consumes: the Task 5 bindings.
- Produces:
  - `export interface SaveOptions { compress?: boolean }`
  - `save(options?: SaveOptions): Promise<Uint8Array>` — `compress` defaults to `true`.

- [ ] **Step 1: Write the failing integration test**

Create `tests/compression.test.ts`. Match the existing test harness (import style, wasm init, fixture loading) used by a neighboring test in `tests/`:

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument } from "../src/index";

describe("stream compression", () => {
  test("create: compressed output is smaller and still valid", async () => {
    async function build(compress: boolean) {
      const doc = await PdfDocument.create();
      const page = doc.addPage();
      // repetitive text compresses well
      for (let i = 0; i < 200; i++) {
        page.drawText("The quick brown fox jumps over the lazy dog.", {
          x: 50,
          y: 700 - (i % 40) * 15,
          size: 10,
        });
      }
      return doc.save({ compress });
    }
    const compressed = await build(true);
    const raw = await build(false);
    expect(compressed.length).toBeLessThan(raw.length);
    // Both remain valid PDFs.
    expect(new TextDecoder().decode(compressed.slice(0, 5))).toBe("%PDF-");
    expect(new TextDecoder().decode(raw.slice(0, 5))).toBe("%PDF-");
  });

  test("default is compressed", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage();
    for (let i = 0; i < 200; i++) {
      page.drawText("compress me compress me compress me", { x: 40, y: 60 + i, size: 8 });
    }
    const dflt = await doc.save();

    const doc2 = await PdfDocument.create();
    const page2 = doc2.addPage();
    for (let i = 0; i < 200; i++) {
      page2.drawText("compress me compress me compress me", { x: 40, y: 60 + i, size: 8 });
    }
    const raw = await doc2.save({ compress: false });
    expect(dflt.length).toBeLessThan(raw.length);
  });

  test("round-trip: compressed load-path draw is reloadable", async () => {
    const base = await (await PdfDocument.create()).save();
    const doc = await PdfDocument.load(base);
    doc.getPage(0).drawText("stamped", { x: 50, y: 50, size: 12 });
    const out = await doc.save({ compress: true });
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });
});
```

Adjust API calls (`addPage`/`getPage`/`getPageCount`/`drawText` option names) to the real signatures — read an existing draw test in `tests/` and mirror it exactly.

- [ ] **Step 2: Run to verify it fails**

Run: `bun test tests/compression.test.ts`
Expected: FAIL — `save` does not accept an options argument (type error or ignored option → size assertion fails).

- [ ] **Step 3: Add the `SaveOptions` type and thread it through `save()`**

In `src/core/document.ts`:

```ts
export interface SaveOptions {
  /** Deflate-compress generated streams. Defaults to true. */
  compress?: boolean;
}
```

Update `save` and the private helpers to accept and forward `compress` (default `true`):

```ts
async save(options: SaveOptions = {}): Promise<Uint8Array> {
  const compress = options.compress ?? true;

  if (this.mode === "create" && !this.sealed) {
    try {
      return this.buildCreatedBytes(compress);
    } catch (e) {
      throw toPdfError(e);
    }
  }

  this.injectPendingFields(compress); // load-mode: bake any pending builder fields
  const form = this.form;

  if (this.structureOps.length > 0) {
    return this.saveChained(form, compress);
  }

  // ... build `plan` exactly as today ...

  return callBytes(() =>
    this.wasm.applyAll(this.bytes, JSON.stringify(plan), fillImages, drawImages, fonts, compress),
  );
}
```

Update the signatures/bodies of `buildCreatedBytes(compress: boolean)` (forward to `this.wasm.createDocument(..., compress)`), `saveChained(form, compress: boolean)` (forward `compress` into each `fillFields` / `flattenFields` / `insertPages` / `manipulatePages` / `setMetadata` / `setOutline` / `applyDrawOps` call it makes), and `injectPendingFields(compress: boolean)` (forward into `this.wasm.injectFields(..., compress)`).

Note: `injectPendingFields` runs before compression matters for the field bake; pass `compress` through so the injected new streams are compressed too. Confirm the internal callers of `injectPendingFields` (e.g. `document.ts:654`) pass a value — default them to `true`.

- [ ] **Step 4: Export the type**

In `src/index.ts` and `src/index.browser.ts`, re-export:

```ts
export type { SaveOptions } from "./core/document";
```

- [ ] **Step 5: Run the compression test**

Run: `bun test tests/compression.test.ts`
Expected: PASS.

- [ ] **Step 6: Type-check and run the whole suite**

Run: `bunx tsc --noEmit -p tsconfig.json && bun test`
Expected: no type errors; all tests pass. Fix any existing test that asserted on raw output size/content by having it pass `{ compress: false }`.

- [ ] **Step 7: Commit**

```bash
git add src/core/document.ts src/index.ts src/index.browser.ts tests/compression.test.ts
git commit -m "feat: add save({ compress }) option, default on"
```

---

### Task 7: Documentation

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: nothing.
- Produces: user-facing docs for `save({ compress })`.

- [ ] **Step 1: Add a Features bullet in `README.md`**

Under the Features list, add:

```markdown
- Deflate-compress generated content, appearance, and font streams on save — on by default, opt out with `doc.save({ compress: false })`. Streams already compressed (images, embedded fonts) are left untouched.
```

- [ ] **Step 2: Add a usage note in `README.md`**

Near the existing `doc.save()` examples, add:

````markdown
### Compression

`save()` deflates generated streams by default, producing smaller PDFs:

```ts
const small = await doc.save();                    // compressed (default)
const plain = await doc.save({ compress: false }); // uncompressed, e.g. for debugging
```
````

- [ ] **Step 3: Add a `CHANGELOG.md` entry**

Add a new minor-version section (bump the current minor) documenting: stream compression on save, default `true`, new `SaveOptions.compress`, additive/backward-compatible.

- [ ] **Step 4: Bump the version**

Update `version` in `package.json` and `crates/core/Cargo.toml` to the new minor version (match the CHANGELOG heading).

- [ ] **Step 5: Commit**

```bash
git add README.md CHANGELOG.md package.json crates/core/Cargo.toml
git commit -m "docs: document save({ compress }) and release notes"
```

---

## Self-Review

**Spec coverage:**
- Helper compressing eligible generated streams → Task 1. ✓
- Content/appearance streams made eligible → Task 2. ✓
- Create path wired + gated → Task 3. ✓
- All incremental paths + merge wired + gated → Task 4. ✓
- wasm rebuild + binding thread-through → Task 5. ✓
- Public `save({ compress })` default true + type export + integration tests → Task 6. ✓
- Docs + version bump → Task 7. ✓
- Images/fonts not double-compressed → guaranteed by `Stream::compress()`'s `/Filter` guard (Design Notes) + leaving `appearance.rs:652` untouched (Task 2 Step 2). ✓
- Append-only integrity → only the new `Document` is compressed before `create_from` (Task 4). ✓

**Type consistency:** trailing `compress: bool` (Rust) / `compress: boolean` (TS) added consistently across Rust entry fns, wasm exports, TS bindings, and `document.ts`; `compress_generated_streams` name used identically at every call site; `SaveOptions.compress` optional with `?? true` default at the single `save()` entry.

**Out of scope (explicitly not in this plan):** image downsampling/lossy re-encoding, object streams (`use_object_streams`), xref streams, linearization. These are separate future plans.
