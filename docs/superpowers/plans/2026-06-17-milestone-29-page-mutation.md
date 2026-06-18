# Milestone M29: Page Mutation (rotate / resize) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rotate and resize existing pages — `page.setRotation(degrees)`, `page.setSize(width, height)`, `page.setMediaBox(x0,y0,x1,y1)` — on both loaded and created PDFs.

**Architecture:** Two new ops, `setRotation` and `setMediaBox`, ride the existing draw-op queue. On LOADED PDFs they are handled by `apply_draw_ops_json` (draw.rs), which already clones the target page into the incremental document and mutates its dict — the ops just set `/Rotate` / `/MediaBox` on that cloned page dict. On CREATED PDFs they are handled by `create_document_json` (create.rs), which applies them to the freshly built page dict. `page.setSize(w,h)` is TS sugar for `setMediaBox(0,0,w,h)`.

**Tech Stack:** Rust 2024, `lopdf` 0.41, `serde`; TypeScript ESM; Bun + cargo test.

## Global Constraints

- Op-queue architecture: mutation ops are queued on the same `DrawQueue` as draw ops and applied at `save()`.
- Loaded PDFs: incremental update (the page is cloned into the new document, as `apply_draw_ops` already does); original bytes preserved as prefix.
- `/Rotate` MUST be a multiple of 90; normalize any input to `[0,360)` via `((deg % 360) + 360) % 360` and reject non-multiples-of-90.
- `/MediaBox` = `[x0,y0,x1,y1]`, all finite, `x1 > x0`, `y1 > y0`.
- A page that has ONLY mutation ops (no text/image/shape) must NOT get an empty content stream appended — apply the dict mutation without adding draw content.
- Both engines (draw.rs loaded, create.rs created) handle the two ops. Standard draw/create paths unchanged.
- Validate ALL ops before mutating, in both engines' existing validation passes.
- Every task ends green: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`, `bun test`, `bun run typecheck`. No root Cargo.toml. Rebuild wasm before bun tests after Rust changes. `pkg-web/` gitignored. Tests in `tests/`.
- Branch `m29-page-mutation`; do NOT implement on master.

## File Structure

- Modify: `crates/core/src/draw.rs` — add `SetRotation`/`SetMediaBox` to `DrawOp`; validate; apply to cloned page dict; guard empty content stream.
- Modify: `crates/core/src/create.rs` — add `SetRotation`/`SetMediaBox` to `CreateOp`; validate; apply to built page dict.
- Modify: `src/generate/draw-queue.ts` — `SetRotationOp`/`SetMediaBoxOp` types + push methods; include in both payloads.
- Modify: `src/generate/page.ts` — `setRotation`/`setSize`/`setMediaBox` on `PdfPage` (+ validation).
- Tests: `crates/core/src/draw.rs` + `create.rs` (`#[cfg(test)]`), `tests/page-mutation.test.ts`.

## Interfaces (cross-task contract)

- Wire ops (both engines): `{"op":"setRotation","page":i,"degrees":90}` and `{"op":"setMediaBox","page":i,"box":[x0,y0,x1,y1]}`.
- `DrawOp`/`CreateOp` gain `SetRotation { page: usize, degrees: i64 }` and `SetMediaBox { page: usize, box: [f32;4] }` (use `#[serde(rename="box")]` on a field named `media_box` since `box` is a Rust keyword).
- TS: `page.setRotation(degrees: number): void`, `page.setSize(width: number, height: number): void`, `page.setMediaBox(x0: number, y0: number, x1: number, y1: number): void`. DrawQueue: `pushSetRotation(page, degrees)`, `pushSetMediaBox(page, box)`.

---

### Task 1: Loaded-PDF page mutation (draw.rs)

**Files:** `crates/core/src/draw.rs`.

- [ ] **Step 1: Write failing tests**

```rust
// in draw.rs tests module (uses existing FICHA fixture + ops() helper signature)
#[test]
fn set_rotation_persists() {
    let out = apply_draw_ops_json(FICHA, r#"[{"op":"setRotation","page":0,"degrees":90}]"#, &[], &[], "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    let rot = doc.get_dictionary(pid).unwrap().get(b"Rotate").unwrap().as_i64().unwrap();
    assert_eq!(rot, 90);
}

#[test]
fn set_rotation_normalizes_negative() {
    let out = apply_draw_ops_json(FICHA, r#"[{"op":"setRotation","page":0,"degrees":-90}]"#, &[], &[], "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    assert_eq!(doc.get_dictionary(pid).unwrap().get(b"Rotate").unwrap().as_i64().unwrap(), 270);
}

#[test]
fn set_rotation_rejects_non_multiple_of_90() {
    let r = apply_draw_ops_json(FICHA, r#"[{"op":"setRotation","page":0,"degrees":45}]"#, &[], &[], "[]");
    assert!(r.unwrap_err().contains("90"));
}

#[test]
fn set_media_box_changes_dimensions() {
    let out = apply_draw_ops_json(FICHA, r#"[{"op":"setMediaBox","page":0,"box":[0,0,200,300]}]"#, &[], &[], "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    let mb = doc.get_dictionary(pid).unwrap().get(b"MediaBox").unwrap().as_array().unwrap();
    assert!((mb[2].as_float().unwrap() - 200.0).abs() < 0.5);
    assert!((mb[3].as_float().unwrap() - 300.0).abs() < 0.5);
}

#[test]
fn set_media_box_rejects_inverted() {
    let r = apply_draw_ops_json(FICHA, r#"[{"op":"setMediaBox","page":0,"box":[100,0,50,300]}]"#, &[], &[], "[]");
    assert!(r.is_err());
}

#[test]
fn rotation_only_page_has_no_empty_draw_stream_corruption() {
    // a page with only a mutation op must still reload cleanly
    let out = apply_draw_ops_json(FICHA, r#"[{"op":"setRotation","page":0,"degrees":180}]"#, &[], &[], "[]").unwrap();
    assert_eq!(&out[..FICHA.len()], FICHA); // incremental
    assert!(Document::load_mem(&out).is_ok());
}
```

- [ ] **Step 2: Run — expect FAIL (unknown variant)**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml draw::tests::set_rotation_persists`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Add to `DrawOp`:
  ```rust
  SetRotation { page: usize, degrees: i64 },
  SetMediaBox {
      page: usize,
      #[serde(rename = "box")]
      media_box: [f32; 4],
  },
  ```
- In the validation pass: both need `page < page_count`. `SetRotation`: `degrees.rem_euclid(90) != 0` → `Err("rotation degrees must be a multiple of 90")`. `SetMediaBox`: all four finite, `media_box[2] > media_box[0]`, `media_box[3] > media_box[1]`, else `Err("invalid media box")`.
- In the per-page grouping: include these ops (they have a `page`). In the per-page processing, AFTER the page is cloned (`opt_clone_object_to_new_document(page_id)` already runs), apply mutations to the page dict via `dict_mut(&mut inc, page_id)?`:
  - `SetRotation`: normalized = `((degrees % 360) + 360) % 360`; `set("Rotate", Object::Integer(normalized))`.
  - `SetMediaBox`: `set("MediaBox", Object::Array(box.map(Object::Real)))`.
- **Empty-content guard:** only append the draw content stream / wrap with q…Q when `stream_content` is non-empty. A page touched solely by mutation ops produces empty `stream_content` → skip the Contents-array rewrite entirely (still clone + mutate the dict). Verify the existing q/Q wrapping and Contents-array logic is bypassed in that case.

> VERIFY: the page is currently cloned unconditionally when it appears in `page_ops`. Confirm mutation-only pages still trigger the clone (they must, to mutate the dict) but NOT the empty content append. Adjust the grouping/append so a mutation-only page clones + mutates without adding an empty draw stream. The `rotation_only_page...` test is the gate.

- [ ] **Step 4: Run — expect PASS, then full crate suite**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: green, pristine.

- [ ] **Step 5: Commit**

```bash
git checkout -b m29-page-mutation
git add crates/core/src/draw.rs
git commit -m "feat(pages): setRotation/setMediaBox on loaded PDFs (incremental)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Created-PDF page mutation (create.rs)

**Files:** `crates/core/src/create.rs`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn created_page_rotation_applied() {
    let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"setRotation","page":0,"degrees":90}]"#;
    let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    assert_eq!(doc.get_dictionary(pid).unwrap().get(b"Rotate").unwrap().as_i64().unwrap(), 90);
}

#[test]
fn created_page_media_box_override() {
    let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"setMediaBox","page":0,"box":[0,0,200,300]}]"#;
    let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    let mb = doc.get_dictionary(pid).unwrap().get(b"MediaBox").unwrap().as_array().unwrap();
    assert!((mb[2].as_float().unwrap() - 200.0).abs() < 0.5);
}

#[test]
fn created_page_rotation_rejects_non_multiple() {
    let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"setRotation","page":0,"degrees":33}]"#;
    assert!(create_document_json(ops, &[], &[], "[]", "[]").is_err());
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml create::tests::created_page_rotation_applied`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Add the same `SetRotation`/`SetMediaBox` variants to `CreateOp` (mirror Task 1's serde shape).
- Validation pass: `page < pages.len()`; same degree/box checks as Task 1.
- These ops are NOT page content — add a no-op arm in the per-page drawing match (don't draw them). After building each `page_dict` for `page_index` (where `/MediaBox` is set from the AddPage width/height), apply any mutation ops targeting that index: set `/Rotate` (normalized) and override `/MediaBox`. Apply BEFORE the page object is added, or mutate the dict before `doc.add_object(Object::Dictionary(page_dict))`.

- [ ] **Step 4: Run — expect PASS, then full suite**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/create.rs
git commit -m "feat(pages): setRotation/setMediaBox on created PDFs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: TypeScript API — PdfPage.setRotation/setSize/setMediaBox

**Files:** `src/generate/draw-queue.ts`, `src/generate/page.ts`.

- [ ] **Step 1: Rebuild wasm (no new exports, but ensure fresh)**

Run: `. ~/.cargo/env && bun run build:wasm`
Expected: ok. (No signature change — apply_draw_ops/create_document already take the same args; the new ops ride in opsJson.)

- [ ] **Step 2: Write failing TS tests**

```ts
// tests/page-mutation.test.ts
import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { readFileSync } from "node:fs";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("rotate a loaded page persists", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  doc.getPage(0).setRotation(90);
  const out = await doc.save();
  const reopened = await PdfDocument.load(out);
  expect(reopened.getPage(0).rotation).toBe(90);
});

test("resize a created page", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.setSize(200, 300);
  const out = await doc.save();
  const reopened = await PdfDocument.load(out);
  const p = reopened.getPage(0);
  expect(Math.round(p.width)).toBe(200);
  expect(Math.round(p.height)).toBe(300);
});

test("setRotation rejects non-multiple of 90", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  expect(() => doc.getPage(0).setRotation(45)).toThrow();
});
```
> Confirm `PdfPage.rotation` is exposed (it is constructed from `read_pages`). If `reopened.getPage(0).rotation` reflects the new value after reload, the read path already covers it.

- [ ] **Step 3: Run — expect FAIL (`setRotation` undefined)**

Run: `bun test tests/page-mutation.test.ts`
Expected: FAIL.

- [ ] **Step 4: Implement**

- `draw-queue.ts`: add `SetRotationOp = {op:"setRotation"; page:number; degrees:number}` and `SetMediaBoxOp = {op:"setMediaBox"; page:number; box:[number,number,number,number]}` to the op union; `pushSetRotation(page, degrees)` and `pushSetMediaBox(page, box)`; include them in the drawOps array so BOTH `toDrawPayload` and `toCreatePayload` serialize them (they already flow through `buildDrawOps`/the ops list — mirror how Line/Rectangle ops are pushed via `pushLine` etc.).
- `page.ts`: add to `PdfPage`:
  - `setRotation(degrees: number)`: validate `Number.isFinite(degrees) && degrees % 90 === 0` (throw `RangeError` otherwise); `this.drawQueue.pushSetRotation(this.index, degrees)`.
  - `setMediaBox(x0,y0,x1,y1)`: validate finite + `x1>x0 && y1>y0` (throw `RangeError`); `pushSetMediaBox(this.index, [x0,y0,x1,y1])`.
  - `setSize(width, height)`: validate `>0`; delegate to `setMediaBox(0,0,width,height)`.
  - These work in both load and create mode (the op targets `this.index`; create mode applies it in create.rs).

- [ ] **Step 5: Run — expect PASS, then full verification**

Run: `bun test tests/page-mutation.test.ts && bun test && bun run typecheck && . ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: all green, tsc clean.

- [ ] **Step 6: Commit**

```bash
git add src/ tests/page-mutation.test.ts
git commit -m "feat(pages): PdfPage.setRotation/setSize/setMediaBox TS API

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Docs, skill, version 0.7.0

**Files:** `docs/site/src/content/docs/guides/generating.md`, `docs/site/src/content/docs/reference/limitations.md`, `docs/site/src/content/docs/migrating/from-pdf-lib.md`, `skills/better-pdf/SKILL.md`, `README.md`, `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`.

- [ ] **Step 1: Docs** — add a "Rotate & resize pages" subsection (`page.setRotation`, `page.setSize`, `page.setMediaBox`; loaded + created). Update `limitations.md`: in-place page rotation/resize now SUPPORTED (remove from the M28 "not yet available" note; keep blank-page-insert as still unavailable). Update `from-pdf-lib.md` (parity with pdf-lib's `page.setRotation`/`setSize`/`setMediaBox`). Update `SKILL.md` + `README.md`.

- [ ] **Step 2: Version** — bump `package.json` + `crates/core/Cargo.toml` to `0.7.0`. `CHANGELOG.md` `0.7.0`: "Page rotate/resize: `page.setRotation()`, `page.setSize()`, `page.setMediaBox()` on loaded and created PDFs."

- [ ] **Step 3: Regen TypeDoc if it builds** — `bun run build:wasm && bun run docs`; add regenerated api-reference if clean, else note.

- [ ] **Step 4: Final verification + commit**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml && bun test && bun run typecheck`
Expected: green.
```bash
git add docs/ skills/ README.md CHANGELOG.md package.json crates/core/Cargo.toml
git commit -m "docs(pages): document page rotate/resize; release 0.7.0

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:** loaded rotate/resize (T1), created rotate/resize (T2), TS PdfPage API incl. setSize sugar + validation (T3), docs/version (T4). Rotation normalized + multiple-of-90 enforced; MediaBox validated; mutation-only pages don't corrupt.

**Placeholder scan:** One verify block (T1 empty-content guard for mutation-only pages) with a gating test — not a placeholder.

**Type consistency:** ops `setRotation {page,degrees}` / `setMediaBox {page,box}` identical across DrawOp (draw.rs), CreateOp (create.rs), and the TS op types. Rust field `media_box` with `#[serde(rename="box")]`. `setSize(w,h)` → `setMediaBox(0,0,w,h)` in TS only. No WASM signature changes (ops ride existing opsJson).
