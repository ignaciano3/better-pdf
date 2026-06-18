# Milestone M26: Document Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read and write the document Info dictionary (Title, Author, Subject, Keywords, Creator, Producer, CreationDate, ModificationDate) on both loaded and created PDFs.

**Architecture:** A new `metadata` module in the Rust core reads/writes the Info dict. For LOADED PDFs a new `set_metadata` WASM entrypoint does an incremental update (mirrors `apply_draw_ops`). For CREATED PDFs metadata rides in `ops_json` as a new `CreateOp::Metadata` variant (avoids adding yet another positional arg to the already-5-arg `create_document`). The TS layer holds a metadata object set via `doc.setTitle(...)` etc.; on `save()` it calls `set_metadata` (load mode) or emits the metadata create-op (create mode).

**Tech Stack:** Rust 2024, `lopdf` 0.41, `serde`/`serde_json`, `wasm-bindgen`; TypeScript ESM; Bun + cargo test.

## Global Constraints

- Op-queue architecture locked — WASM stateless; metadata serialized on `save()`.
- Draw/metadata features work on BOTH loaded and created PDFs.
- Loaded-PDF writes use incremental update (`IncrementalDocument`), like `draw.rs`/`fill.rs` — original bytes preserved as prefix.
- Absent metadata keys are PRESERVED on write (clone existing Info, set only provided keys). Explicit removal is out of scope for this milestone (only set/overwrite).
- No new positional arg on `create_document` — create-mode metadata travels in `ops_json`.
- PDF date strings use the PDF date syntax `D:YYYYMMDDHHmmSS` with a trailing `Z` or UTC offset; the TS layer formats `Date` → this string.
- Every task ends green: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml` and `bun test` and `bun run typecheck`. No root Cargo.toml — always `--manifest-path crates/core/Cargo.toml`. Rebuild wasm (`bun run build:wasm`) after Rust signature/export changes, before bun tests.
- `pkg-web/` is gitignored — never `git add` it. Test dir is `tests/` (not `test/`).
- Commit on a feature branch `m26-metadata`; do NOT implement directly on master.

## File Structure

- Create: `crates/core/src/metadata.rs` — `Metadata` struct, `read_metadata_json`, `set_metadata_json`, `build_info_dict` (shared with create.rs).
- Modify: `crates/core/src/lib.rs` — `mod metadata;`, `read_metadata`/`set_metadata` wasm exports, fuzz_api re-export.
- Modify: `crates/core/src/create.rs` — `CreateOp::Metadata` variant; attach Info dict to trailer when present.
- Create: `src/generate/metadata.ts` — `DocumentMetadata` type + `toPdfDate(Date)`.
- Modify: `src/core/document.ts` — metadata field + setters + `getMetadata()` + save-path wiring; `CoreWasm` gains `setMetadata`/`readMetadata`.
- Modify: `src/core/wasm.ts`, `src/core/wasm-browser.ts` — `setMetadata`/`readMetadata` wrappers.
- Modify: `src/generate/draw-queue.ts` — carry a metadata create-op into `toCreatePayload`.
- Tests: `crates/core/src/metadata.rs` (`#[cfg(test)]`), `crates/core/src/create.rs` tests, `tests/metadata.test.ts`.

## Interfaces (cross-task contract)

- Rust: `pub fn read_metadata_json(data: &[u8]) -> Result<String, String>` (JSON object, keys present only when set). `pub fn set_metadata_json(data: &[u8], meta_json: &str) -> Result<Vec<u8>, String>` (incremental). `pub(crate) fn build_info_dict(meta: &Metadata) -> lopdf::Dictionary`. `Metadata` is a serde struct with camelCase optional fields: `title, author, subject, keywords, creator, producer, creationDate, modDate`.
- WASM: `read_metadata(data) -> String`, `set_metadata(data, meta_json) -> Vec<u8>`.
- Wire (create-op): `{"op":"metadata","title":"…","author":"…",…}` — same field names as `Metadata`.
- TS: `doc.setTitle(s)`, `setAuthor(s)`, `setSubject(s)`, `setKeywords(string[])`, `setProducer(s)`, `setCreator(s)`, `setCreationDate(Date)`, `setModificationDate(Date)`, `getMetadata(): Promise<DocumentMetadata>`. `CoreWasm.setMetadata(data, metaJson)`, `CoreWasm.readMetadata(data)`.

---

### Task 1: Rust metadata module — read + incremental write + WASM exports

**Files:** Create `crates/core/src/metadata.rs`; modify `crates/core/src/lib.rs`.

**Interfaces produced:** `read_metadata_json`, `set_metadata_json`, `build_info_dict`, `Metadata` (per contract above); wasm `read_metadata`/`set_metadata`.

- [ ] **Step 1: Write failing test — read returns a JSON object**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn read_metadata_returns_json_object() {
        let json = read_metadata_json(FICHA).unwrap();
        assert!(json.starts_with('{') && json.ends_with('}'));
    }

    #[test]
    fn set_then_read_round_trips() {
        let out = set_metadata_json(FICHA, r#"{"title":"Quarterly Report","author":"ACME"}"#).unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental: original preserved
        let json = read_metadata_json(&out).unwrap();
        assert!(json.contains("Quarterly Report"), "json was {json}");
        assert!(json.contains("ACME"), "json was {json}");
    }
}
```

- [ ] **Step 2: Run — expect FAIL (undefined fns)**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml metadata::tests`
Expected: FAIL (unresolved `read_metadata_json`/`set_metadata_json`).

- [ ] **Step 3: Implement the module**

Implement `Metadata` (serde, camelCase, all `Option<String>`, `#[serde(skip_serializing_if = "Option::is_none")]`), `read_metadata_json` (read trailer `/Info` ref → dict → each key via a `get_str` helper, serialize), `build_info_dict(&Metadata) -> Dictionary` (set each provided key as `Object::string_literal`), and `set_metadata_json` (load doc, clone existing Info dict so unspecified keys survive, overlay provided keys via `build_info_dict` semantics, `IncrementalDocument::create_from`, add Info object, set `trailer.set("Info", ref)`, `save_to`). Keys: Title/Author/Subject/Keywords/Creator/Producer/CreationDate/ModDate.

> VERIFY, do not assume: confirm `IncrementalDocument` emits the new trailer `/Info` on `save_to` so `read_metadata_json` finds it after round-trip. The `set_then_read_round_trips` test is the gate. If the incremental trailer is not written, set Info on `inc.new_document.trailer` AND ensure the prev-doc Root/trailer chain still resolves — adjust until the test passes. Look at how `draw.rs` uses `IncrementalDocument` for the pattern.

- [ ] **Step 4: Run — expect PASS**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml metadata::tests`
Expected: PASS (both).

- [ ] **Step 5: Add WASM exports + module registration in lib.rs**

```rust
mod metadata;

/// Read the document Info dictionary as a JSON object.
#[wasm_bindgen]
pub fn read_metadata(data: &[u8]) -> Result<String, JsError> {
    metadata::read_metadata_json(data).map_err(|e| JsError::new(&e))
}

/// Set Info-dictionary metadata; returns new PDF bytes (incremental update).
#[wasm_bindgen]
pub fn set_metadata(data: &[u8], meta_json: &str) -> Result<Vec<u8>, JsError> {
    metadata::set_metadata_json(data, meta_json).map_err(|e| JsError::new(&e))
}
```
Add `pub use crate::metadata::{read_metadata_json, set_metadata_json};` to the `fuzz_api` module.

- [ ] **Step 6: Full crate suite + commit**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: all pass, pristine.
```bash
git checkout -b m26-metadata
git add crates/core/src/metadata.rs crates/core/src/lib.rs
git commit -m "feat(metadata): read + incrementally write document Info dictionary

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Create-mode metadata via CreateOp::Metadata

**Files:** `crates/core/src/create.rs`.

**Interfaces produced:** new `CreateOp::Metadata` variant; when present, the created doc's trailer references an Info dict built via `crate::metadata::build_info_dict`.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn created_doc_has_metadata() {
    let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"metadata","title":"Generated","author":"better-pdf"}]"#;
    let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
    let json = crate::metadata::read_metadata_json(&out).unwrap();
    assert!(json.contains("Generated"), "json was {json}");
    assert!(json.contains("better-pdf"), "json was {json}");
}
```

- [ ] **Step 2: Run — expect FAIL (unknown op `metadata`)**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml create::tests::created_doc_has_metadata`
Expected: FAIL (serde rejects unknown variant, or no Info written).

- [ ] **Step 3: Implement**

Add to the `CreateOp` enum a `Metadata` variant deserializing the same fields as `crate::metadata::Metadata` (reuse the struct: e.g. `Metadata(crate::metadata::Metadata)` with `#[serde(rename="metadata")]`, or inline fields — pick what fits the existing `#[serde(tag="op")]` enum). In `create_document_json`, after building the catalog, if a metadata op was present build the Info dict via `crate::metadata::build_info_dict` and `doc.trailer.set("Info", Object::Reference(info_id))`. Make `crate::metadata::Metadata` and `build_info_dict` visible (`pub(crate)`). At most one metadata op; if multiple, last wins (document this).

- [ ] **Step 4: Run — expect PASS, then full suite**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/create.rs crates/core/src/metadata.rs
git commit -m "feat(metadata): set Info dictionary on created documents via metadata op

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: TypeScript API — setters, getMetadata, save wiring, wasm wrappers

**Files:** `src/generate/metadata.ts` (new), `src/core/document.ts`, `src/core/wasm.ts`, `src/core/wasm-browser.ts`, `src/generate/draw-queue.ts`.

**Interfaces produced:** the `doc.set*`/`getMetadata` API + `CoreWasm.setMetadata`/`readMetadata`.

- [ ] **Step 1: Rebuild wasm so the new exports exist**

Run: `. ~/.cargo/env && bun run build:wasm`
Expected: `pkg-web` exports `read_metadata`/`set_metadata`. (Do not commit pkg-web.)

- [ ] **Step 2: Write failing TS test**

```ts
// tests/metadata.test.ts
import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { readFileSync } from "node:fs";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("set + get metadata round-trips on a loaded PDF", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  doc.setTitle("Quarterly Report");
  doc.setAuthor("ACME");
  doc.setKeywords(["invoice", "2026"]);
  const bytes = await doc.save();
  const reopened = await PdfDocument.load(bytes);
  const meta = await reopened.getMetadata();
  expect(meta.title).toBe("Quarterly Report");
  expect(meta.author).toBe("ACME");
  expect(meta.keywords).toContain("invoice");
});

test("metadata on a created document", async () => {
  const doc = await PdfDocument.create();
  doc.setTitle("Generated");
  doc.addPage();
  const bytes = await doc.save();
  const meta = await (await PdfDocument.load(bytes)).getMetadata();
  expect(meta.title).toBe("Generated");
});
```

- [ ] **Step 3: Run — expect FAIL (`setTitle` undefined)**

Run: `bun test tests/metadata.test.ts`
Expected: FAIL.

- [ ] **Step 4: Implement**

- `src/generate/metadata.ts`: export `DocumentMetadata` interface (`title?`, `author?`, `subject?`, `keywords?: string`, `creator?`, `producer?`, `creationDate?`, `modDate?`) and `toPdfDate(d: Date): string` producing `D:YYYYMMDDHHmmSS` + `Z` (use UTC getters). Keywords stored as a single comma-joined string on the wire (PDF /Keywords is one string); `getMetadata` may split back to an array OR return the raw string — keep the wire as a string and expose `keywords` to users as `string[]` in setters (join with `, `) and split on read. Keep it simple and tested.
- `document.ts`: add `private metadata: Record<string, string> = {}` + `private metadataDirty = false`. Each setter writes into it (`setKeywords` joins with `, `; `setCreationDate`/`setModificationDate` use `toPdfDate`) and sets the flag. `getMetadata()` (async): for create mode, return the locally-set values; for load mode, parse `this.wasm.readMetadata(this.bytes)` merged with any locally-set overrides (locally-set wins). In `save()`: create mode → push a metadata op into the draw queue when dirty (Step 5); load mode → after the existing draw branch, `if (this.metadataDirty) bytes = this.wasm.setMetadata(bytes, JSON.stringify(this.metadata));`. Add `setMetadata`/`readMetadata` to the `CoreWasm` interface.
- `wasm.ts` + `wasm-browser.ts`: add `setMetadata(data, metaJson)` and `readMetadata(data)` wrappers (browser version calls `ensureInitialized()` first), importing `set_metadata`/`read_metadata`.

- [ ] **Step 5: Wire create-mode metadata op in draw-queue.ts**

Add a `pushMetadata(meta: Record<string,string>)` (store one metadata object) and include it as `{op:"metadata", ...meta}` at the FRONT of the create ops in `toCreatePayload()` (so it's present alongside `addPage`). `document.save()` create-branch calls `drawQueue.pushMetadata(this.metadata)` when `metadataDirty` before building the payload.

- [ ] **Step 6: Run — expect PASS, then full suite + typecheck**

Run: `bun test tests/metadata.test.ts && bun test && bun run typecheck && . ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: all green, tsc clean.

- [ ] **Step 7: Commit**

```bash
git add src/ tests/metadata.test.ts
git commit -m "feat(metadata): doc.setTitle/getMetadata TS API for loaded and created PDFs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Docs, skill, version 0.5.0

**Files:** `docs/site/src/content/docs/guides/generating.md` (or a metadata guide), `docs/site/src/content/docs/reference/limitations.md`, `docs/site/src/content/docs/migrating/from-pdf-lib.md`, `skills/better-pdf/SKILL.md`, `README.md`, `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`.

- [ ] **Step 1: Docs** — add a "Document metadata" section (setters + `getMetadata`, loaded + created). Update `limitations.md` (metadata no longer a gap). Update `from-pdf-lib.md` (parity with pdf-lib's `setTitle`/etc.). Update `SKILL.md` (LLM-facing) and `README.md` feature list with a short example.

- [ ] **Step 2: Version** — bump `package.json` and `crates/core/Cargo.toml` to `0.5.0`. Add a `CHANGELOG.md` `0.5.0` entry: "Document metadata: read/write Info dictionary (Title/Author/Subject/Keywords/Creator/Producer/dates) on loaded and created PDFs."

- [ ] **Step 3: Regenerate TypeDoc if it builds** — `bun run build:wasm && bun run docs`; if clean, `git add docs/site/src/content/docs/api-reference/`; else note and rely on hand-written guide.

- [ ] **Step 4: Final verification + commit**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml && bun test && bun run typecheck`
Expected: all green.
```bash
git add docs/ skills/ README.md CHANGELOG.md package.json crates/core/Cargo.toml
git commit -m "docs(metadata): document metadata API; release 0.5.0

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:** read (T1), incremental write on loaded PDFs (T1), create-mode write (T2), TS API + both save paths + wasm wrappers (T3), docs/version (T4). Covers Title/Author/Subject/Keywords/Creator/Producer/CreationDate/ModDate read+write on loaded and created PDFs with key preservation.

**Placeholder scan:** One explicit verification point (IncrementalDocument trailer `/Info` emission in T1) with a gating test and a fallback — not a placeholder.

**Type consistency:** `Metadata` (Rust) ↔ `DocumentMetadata` (TS) ↔ create-op fields all use the same camelCase keys (title/author/subject/keywords/creator/producer/creationDate/modDate). `build_info_dict` reused by T2. `setMetadata`/`readMetadata` signatures consistent across CoreWasm/wasm.ts/wasm-browser.ts. Keywords: array in the TS setter, comma-joined string on the wire and in PDF /Keywords.
