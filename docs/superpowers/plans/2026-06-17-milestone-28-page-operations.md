# Milestone M28: Page Operations (merge / extract / reorder / remove / split) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Assemble a new PDF from an ordered selection of pages drawn from one or more source PDFs. This single primitive covers merge, page extraction/copy, reordering, removal, and splitting.

**Architecture:** All operations reduce to ONE primitive — `manipulate_pages(docs_blob, docs_json, plan_json)` — which takes N source PDFs (concatenated blob + offset/length table) and an ordered plan of `{doc, page}` selections, and returns a new PDF containing exactly those pages in that order. Implementation uses lopdf's `renumber_objects_with(starting_id)` to shift each source doc's object ids into a disjoint range, bulk-moves all objects into one merged `Document`, resolves inherited page attributes (MediaBox/Resources/Rotate/CropBox) onto each kept page so it stands alone, builds a fresh Pages tree + Catalog over the selected pages, prunes orphans, and does a full save. (This replaces the roadmap's hand-rolled `import_object_tree` — lopdf's renumber does the id-remapping for a whole doc at once, which is simpler and lower-risk.) The TS layer provides `merge`, `assemble`, `copyPages`, `splitPages` as thin wrappers that build the plan.

**Tech Stack:** Rust 2024, `lopdf` 0.41 (`Document.objects`/`max_id` public fields; `renumber_objects_with`, `get_pages`, `get_dictionary(_mut)`, `dereference`, `prune_objects`, `new_object_id`, `add_object`, `save_to`), `serde`; TypeScript ESM; Bun + cargo test.

## Global Constraints

- Op-queue architecture untouched — this is a stateless transform (`manipulate_pages`), not a queued op; the TS conveniences call it directly and return bytes.
- Output is a freshly built document (full save, not incremental) — the inputs are not mutated.
- Each kept page must be SELF-CONTAINED: resolve inherited MediaBox/CropBox/Resources/Rotate from the source page tree onto the page dict before reparenting (only set a key the page lacks).
- Validate ALL input before building: empty plan → error; `doc` index out of range → error; `page` index out of range → error; out-of-range blob offsets → error (use `checked_add`).
- A source page selected more than once must produce DISTINCT page objects (shallow-clone the page dict for duplicates; shared Contents/Resources stream refs are fine) so a page can appear twice (N-up / repeat) without two `/Parent`s on one object.
- AcroForm interactivity is NOT reconstructed on assembled pages — page `/Annots` (and their appearance streams) are carried so widgets/links still RENDER, but no new `/AcroForm` is built, so form fields are non-interactive after assembly. Document this. Do not corrupt the output.
- No new positional churn elsewhere; `manipulate_pages` is a new standalone WASM export.
- Every task ends green: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`, `bun test`, `bun run typecheck`. No root Cargo.toml. Rebuild wasm (`bun run build:wasm`) before bun tests after Rust export changes. `pkg-web/` gitignored — never commit. Tests in `tests/`.
- Branch `m28-page-operations`; do NOT implement on master.

## File Structure

- Create: `crates/core/src/pageops.rs` — `manipulate_pages_json` + helpers (`resolve_inherited`, assemble).
- Modify: `crates/core/src/lib.rs` — `mod pageops;`, `manipulate_pages` wasm export, fuzz_api re-export.
- Modify: `src/core/document.ts` — `CoreWasm.manipulatePages`; static `assemble`/`merge`; instance `copyPages`/`splitPages` (load mode).
- Modify: `src/core/wasm.ts`, `src/core/wasm-browser.ts` — `manipulatePages` wrapper.
- Tests: `crates/core/src/pageops.rs` (`#[cfg(test)]`), `tests/page-operations.test.ts`. Fixtures: existing `tests/fixtures/**` PDFs (use two distinct multi-page fixtures; the `Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf` is one — find a second multi-page fixture under `tests/fixtures/`).

## Interfaces (cross-task contract)

- Rust: `pub fn manipulate_pages_json(docs_blob: &[u8], docs_json: &str, plan_json: &str) -> Result<Vec<u8>, String>`.
- WASM: `manipulate_pages(docs_blob: &[u8], docs_json: &str, plan_json: &str) -> Vec<u8>`.
- Wire: `docs_json` = `[{"offset":0,"length":N}, ...]` (indexes `docs_blob`). `plan_json` = `[{"doc":0,"page":2},{"doc":1,"page":0}, ...]` (output page order = array order; `page` is 0-based within that source doc's page order).
- TS: `PdfDocument.assemble(docs: Uint8Array[], selections: {docIndex: number, pageIndex: number}[]): Promise<Uint8Array>`; `PdfDocument.merge(docs: Uint8Array[]): Promise<Uint8Array>`; `doc.copyPages(indices: number[]): Promise<Uint8Array>` (load mode); `doc.splitPages(): Promise<Uint8Array[]>` (load mode). `CoreWasm.manipulatePages(docsBlob, docsJson, planJson)`.

---

### Task 1: Rust core — `manipulate_pages_json` + WASM export

**Files:** Create `crates/core/src/pageops.rs`; modify `crates/core/src/lib.rs`.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn page_count(bytes: &[u8]) -> usize {
        Document::load_mem(bytes).unwrap().get_pages().len()
    }

    // Concatenate sources, build the docs_json table.
    fn pack(docs: &[&[u8]]) -> (Vec<u8>, String) {
        let mut blob = Vec::new();
        let mut table = String::from("[");
        for (i, d) in docs.iter().enumerate() {
            if i > 0 { table.push(','); }
            table.push_str(&format!(r#"{{"offset":{},"length":{}}}"#, blob.len(), d.len()));
            blob.extend_from_slice(d);
        }
        table.push(']');
        (blob, table)
    }

    #[test]
    fn merge_two_copies_doubles_page_count() {
        let n = page_count(FICHA);
        let (blob, docs) = pack(&[FICHA, FICHA]);
        // plan = all pages of doc 0 then all pages of doc 1
        let mut plan = String::from("[");
        for d in 0..2 { for p in 0..n {
            if !(d==0 && p==0) { plan.push(','); }
            plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
        }}
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan).unwrap();
        assert_eq!(page_count(&out), 2 * n);
    }

    #[test]
    fn extract_single_page() {
        let (blob, docs) = pack(&[FICHA]);
        let out = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":0}]"#).unwrap();
        assert_eq!(page_count(&out), 1);
        // MediaBox present on the extracted page (inherited attrs resolved)
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert!(doc.get_dictionary(pid).unwrap().has(b"MediaBox"), "extracted page must carry MediaBox");
    }

    #[test]
    fn reorder_preserves_count() {
        let n = page_count(FICHA);
        if n >= 2 {
            let (blob, docs) = pack(&[FICHA]);
            let out = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":1},{"doc":0,"page":0}]"#).unwrap();
            assert_eq!(page_count(&out), 2);
        }
    }

    #[test]
    fn errors_on_empty_plan() {
        let (blob, docs) = pack(&[FICHA]);
        assert!(manipulate_pages_json(&blob, &docs, "[]").is_err());
    }

    #[test]
    fn errors_on_page_out_of_range() {
        let (blob, docs) = pack(&[FICHA]);
        let r = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":9999}]"#);
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_doc_out_of_range() {
        let (blob, docs) = pack(&[FICHA]);
        let r = manipulate_pages_json(&blob, &docs, r#"[{"doc":5,"page":0}]"#);
        assert!(r.unwrap_err().contains("doc"));
    }

    #[test]
    fn duplicate_page_selection_produces_two_distinct_pages() {
        let (blob, docs) = pack(&[FICHA]);
        let out = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":0},{"doc":0,"page":0}]"#).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ids: Vec<_> = doc.get_pages().into_values().collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "duplicate selection must yield distinct page objects");
    }
}
```

- [ ] **Step 2: Run — expect FAIL (undefined)**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops::tests`
Expected: FAIL.

- [ ] **Step 3: Implement `pageops.rs`**

Implement using this structure (fill in details, keep it correct against lopdf 0.41):

```rust
//! Assemble a new PDF from an ordered selection of pages across source PDFs.
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use serde::Deserialize;

#[derive(Deserialize)]
struct DocDesc { offset: usize, length: usize }
#[derive(Deserialize)]
struct Sel { doc: usize, page: usize }

const INHERITABLE: &[&[u8]] = &[b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// Walk the page's /Parent chain; for each inheritable key the page lacks,
/// return the nearest ancestor's value (resolved one level via dereference).
fn resolve_inherited(doc: &Document, page_id: ObjectId) -> Vec<(Vec<u8>, Object)> {
    let mut found: Vec<(Vec<u8>, Object)> = Vec::new();
    let mut current = Some(page_id);
    let mut guard = 0;
    while let Some(id) = current {
        guard += 1;
        if guard > 64 { break; } // cycle guard
        let dict = match doc.get_dictionary(id) { Ok(d) => d, Err(_) => break };
        for &key in INHERITABLE {
            if found.iter().any(|(k, _)| k == key) { continue; }
            if let Ok(v) = dict.get(key) {
                // resolve a reference one level so the value is self-contained-ish
                let resolved = match v {
                    Object::Reference(r) => doc.get_object(*r).cloned().unwrap_or_else(|_| v.clone()),
                    other => other.clone(),
                };
                found.push((key.to_vec(), resolved));
            }
        }
        current = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    found
}

pub fn manipulate_pages_json(docs_blob: &[u8], docs_json: &str, plan_json: &str) -> Result<Vec<u8>, String> {
    let descs: Vec<DocDesc> = serde_json::from_str(docs_json).map_err(|e| format!("invalid docs: {e}"))?;
    let plan: Vec<Sel> = serde_json::from_str(plan_json).map_err(|e| format!("invalid plan: {e}"))?;
    if plan.is_empty() { return Err("no pages selected".to_string()); }

    let mut merged = Document::with_version("1.7");
    let mut next: u32 = 1;
    let mut per_doc_pages: Vec<Vec<ObjectId>> = Vec::new();

    for d in &descs {
        let end = d.offset.checked_add(d.length).ok_or("doc range out of bounds")?;
        if end > docs_blob.len() { return Err("doc range out of bounds".to_string()); }
        let mut doc = Document::load_mem(&docs_blob[d.offset..end]).map_err(|e| e.to_string())?;

        // Resolve inherited attrs onto each page BEFORE renumber/move.
        let pre_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        for &pid in &pre_ids {
            let inh = resolve_inherited(&doc, pid);
            if let Ok(pd) = doc.get_dictionary_mut(pid) {
                for (k, v) in inh { if !pd.has(&k) { pd.set(k, v); } }
            }
        }

        doc.renumber_objects_with(next);
        next = doc.max_id + 1;
        let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        per_doc_pages.push(page_ids);
        merged.objects.extend(std::mem::take(&mut doc.objects));
        merged.max_id = merged.max_id.max(doc.max_id);
    }
    merged.max_id = next.saturating_sub(1).max(merged.max_id);

    // Resolve the plan to concrete page ObjectIds, cloning on duplicate.
    let pages_id = merged.new_object_id();
    let mut kids: Vec<Object> = Vec::with_capacity(plan.len());
    let mut used: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    for s in &plan {
        let pages = per_doc_pages.get(s.doc).ok_or_else(|| format!("doc index {} out of range", s.doc))?;
        let src_pid = *pages.get(s.page).ok_or_else(|| format!("page index {} out of range", s.page))?;
        let pid = if used.contains(&src_pid) {
            // shallow-clone the page dict so a page can appear more than once
            let cloned = merged.get_dictionary(src_pid).map_err(|e| e.to_string())?.clone();
            merged.add_object(Object::Dictionary(cloned))
        } else {
            used.insert(src_pid);
            src_pid
        };
        if let Ok(pd) = merged.get_dictionary_mut(pid) {
            pd.set("Parent", Object::Reference(pages_id));
        }
        kids.push(Object::Reference(pid));
    }

    let count = kids.len() as i64;
    merged.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(count),
    }));
    let catalog_id = merged.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    merged.trailer.set("Root", Object::Reference(catalog_id));
    merged.prune_objects();

    let mut out = Vec::new();
    merged.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```
> VERIFY against lopdf 0.41 as you compile: `new_object_id` relies on `max_id` (set `merged.max_id` correctly before calling it — note `new_object_id` is called BEFORE the final max_id line above; reorder so `merged.max_id` is set from the loop's `next` BEFORE `let pages_id = merged.new_object_id();`). Confirm `prune_objects` keeps everything reachable from `trailer.Root` and drops the old per-source catalogs/pages-trees/unselected pages. Confirm `renumber_objects_with` + `objects.extend` gives disjoint ids (it must — each doc renumbered starting at `next`). If `save_to` emits an invalid doc, check that `Count`/`Kids`/`Parent` are consistent and that no selected page still points `/Parent` at a pruned old tree. The tests are the gate.

- [ ] **Step 4: Run — expect PASS**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops::tests`
Expected: PASS (all). Fix ordering/borrow issues until green.

- [ ] **Step 5: WASM export + lib.rs wiring**

```rust
mod pageops;

/// Assemble a new PDF from an ordered page selection across source PDFs.
#[wasm_bindgen]
pub fn manipulate_pages(docs_blob: &[u8], docs_json: &str, plan_json: &str) -> Result<Vec<u8>, JsError> {
    pageops::manipulate_pages_json(docs_blob, docs_json, plan_json).map_err(|e| JsError::new(&e))
}
```
Add `pub use crate::pageops::manipulate_pages_json;` to `fuzz_api`.

- [ ] **Step 6: Full suite + commit**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: green, pristine.
```bash
git checkout -b m28-page-operations
git add crates/core/src/pageops.rs crates/core/src/lib.rs
git commit -m "feat(pages): assemble PDFs from ordered page selections (merge/extract/reorder)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: TypeScript API — assemble / merge / copyPages / splitPages

**Files:** `src/core/wasm.ts`, `src/core/wasm-browser.ts`, `src/core/document.ts`.

- [ ] **Step 1: Rebuild wasm**

Run: `. ~/.cargo/env && bun run build:wasm`
Expected: `pkg-web` exports `manipulate_pages`. (Do not commit pkg-web.)

- [ ] **Step 2: Write failing TS test**

```ts
// tests/page-operations.test.ts
import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { readFileSync } from "node:fs";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("merge doubles the page count", async () => {
  const bytes = readFileSync(FIXTURE);
  const n = (await PdfDocument.load(bytes)).getPageCount();
  const merged = await PdfDocument.merge([bytes, bytes]);
  const out = await PdfDocument.load(merged);
  expect(out.getPageCount()).toBe(2 * n);
});

test("copyPages extracts the first page", async () => {
  const bytes = readFileSync(FIXTURE);
  const doc = await PdfDocument.load(bytes);
  const onePage = await doc.copyPages([0]);
  expect((await PdfDocument.load(onePage)).getPageCount()).toBe(1);
});

test("splitPages yields one PDF per page", async () => {
  const bytes = readFileSync(FIXTURE);
  const doc = await PdfDocument.load(bytes);
  const n = doc.getPageCount();
  const parts = await doc.splitPages();
  expect(parts.length).toBe(n);
  expect((await PdfDocument.load(parts[0]!)).getPageCount()).toBe(1);
});
```

- [ ] **Step 3: Run — expect FAIL**

Run: `bun test tests/page-operations.test.ts`
Expected: FAIL (`merge` undefined).

- [ ] **Step 4: Implement**

- `wasm.ts` + `wasm-browser.ts`: import `manipulate_pages`; add `manipulatePages(docsBlob: Uint8Array, docsJson: string, planJson: string): Uint8Array` (browser calls `ensureInitialized()` first).
- `document.ts`: add `manipulatePages` to `CoreWasm`. Add a private static helper that concatenates `docs: Uint8Array[]` into a blob + `docs_json` table, calls `wasm.manipulatePages`, returns bytes. Then:
  - `static async assemble(docs: Uint8Array[], selections: {docIndex: number, pageIndex: number}[]): Promise<Uint8Array>` — build `plan_json` from selections (`{doc: s.docIndex, page: s.pageIndex}`), call the helper. Validate `docs` non-empty and selections non-empty (throw a `PdfError` otherwise) — or let the core error surface via `toPdfError`.
  - `static async merge(docs: Uint8Array[]): Promise<Uint8Array>` — for each doc, read its page count (`JSON.parse(wasm.readPages(d)).length`), build selections for all pages of all docs in order, delegate to `assemble`.
  - `async copyPages(indices: number[]): Promise<Uint8Array>` — LOAD mode only (throw `PdfError` in create mode, mirroring `getForm`); selections = `indices.map(i => ({docIndex:0, pageIndex:i}))` against `[this.bytes]`.
  - `async splitPages(): Promise<Uint8Array[]>` — LOAD mode only; for each page index `0..getPageCount()`, call the single-doc assemble with `[{docIndex:0,pageIndex:i}]`; return the array.
  - Use the static wasm singleton the file already references (same one `PdfDocument.load` uses).
- Use `toPdfError` to wrap core errors consistently.

- [ ] **Step 5: Run — expect PASS, then full verification**

Run: `bun test tests/page-operations.test.ts && bun test && bun run typecheck && . ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: all green, tsc clean.

- [ ] **Step 6: Commit**

```bash
git add src/ tests/page-operations.test.ts
git commit -m "feat(pages): merge/assemble/copyPages/splitPages TS API

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Docs, skill, version 0.6.0

**Files:** `docs/site/src/content/docs/guides/generating.md` (or a new "Pages" guide), `docs/site/src/content/docs/reference/limitations.md`, `docs/site/src/content/docs/migrating/from-pdf-lib.md`, `skills/better-pdf/SKILL.md`, `README.md`, `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`.

- [ ] **Step 1: Docs** — add a "Pages: merge, extract, split" section with runnable examples (`PdfDocument.merge`, `doc.copyPages`, `doc.splitPages`, `PdfDocument.assemble`). Update `limitations.md`: page merge/copy/split now SUPPORTED; note the carried-but-non-interactive AcroForm caveat (form fields on assembled pages render but are not interactive) and that blank-page insertion / in-place page mutation (rotate/resize) are not yet available (rotate/resize is M29). Update `from-pdf-lib.md` (parity with `copyPages`/`PDFDocument.create`+`addPage` merge pattern). Update `SKILL.md` + `README.md` feature list with a short example.

- [ ] **Step 2: Version** — bump `package.json` and `crates/core/Cargo.toml` to `0.6.0`. Add `CHANGELOG.md` `0.6.0` entry: "Page operations: merge multiple PDFs, extract/copy/reorder pages, and split — `PdfDocument.merge`/`assemble`, `doc.copyPages`/`splitPages`."

- [ ] **Step 3: Regenerate TypeDoc if it builds** — `bun run build:wasm && bun run docs`; if clean, `git add docs/site/src/content/docs/api-reference/`; else note + rely on hand-written guide.

- [ ] **Step 4: Final verification + commit**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml && bun test && bun run typecheck`
Expected: green.
```bash
git add docs/ skills/ README.md CHANGELOG.md package.json crates/core/Cargo.toml
git commit -m "docs(pages): document page operations; release 0.6.0

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:** merge (T1+T2), extract/copy (T1+T2 copyPages), reorder (T1 plan order), remove (T2 — caller omits indices), split (T2 splitPages), out-of-range/empty validation (T1), duplicate-page distinctness (T1), docs/version (T3). Insert-blank-page and in-place rotate/resize are explicitly out of scope (rotate/resize is M29; blank-page deferred) — noted in limitations.

**Placeholder scan:** One explicit verification block in T1 (max_id ordering before `new_object_id`, prune correctness, disjoint renumber) with the test suite as the gate — not a placeholder.

**Type consistency:** Wire keys `offset`/`length` (DocDesc) and `doc`/`page` (Sel) used consistently across Rust serde, the test `pack` helper, and the TS plan builder. `manipulate_pages(docs_blob, docs_json, plan_json)` signature identical across pageops.rs, lib.rs, CoreWasm, wasm.ts, wasm-browser.ts. TS `assemble` selections use `{docIndex, pageIndex}` and are translated to `{doc, page}` for the wire.

**Risk callouts:** (1) `merged.max_id` must be set from the loop's `next` BEFORE `new_object_id()` is called for `pages_id` — the draft code computes it after; reorder during implementation (the duplicate-page test will catch a bad id). (2) `prune_objects` must not drop selected pages — they're reachable via the new Pages tree; old trees become orphans and are correctly dropped. (3) inherited-attribute resolution must run before renumber while the source tree is intact.
