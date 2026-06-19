# Milestone M34: Text Rotation + Opacity & Outlines/Bookmarks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Two features. (A) Rotated and/or translucent text via `page.drawText(text, { ..., rotate, opacity })`. (B) Document outline (bookmarks) via `doc.setOutline([{title, page, children?}])`. Both on loaded and created PDFs. Final milestone → v0.12.0.

**Architecture:**
- **Text rotate/opacity:** the `text` op gains optional `rotate` (degrees) and `opacity`. The emit helpers (`emit_text_block`, `emit_text_block_cid`) wrap the block in `q … Q` when rotate or opacity is set: apply an ExtGState (`/gs`) for opacity, and a rotation matrix `cm` (cos/sin about the anchor point, with `Td 0 0`) for rotation; otherwise the existing `Td x y` path is unchanged.
- **Outlines:** a recursive outline tree is built into the catalog `/Outlines`. For created docs a `{"op":"outline", items:[...]}` create-op carries the tree; for loaded docs a new `set_outline(data, json)` WASM entrypoint does an incremental update. Each item → `/Title /Parent /Dest [pageRef /XYZ null null null] /First /Last /Next /Prev /Count`.

**Tech Stack:** Rust 2024, lopdf 0.41; TS ESM; Bun + cargo.

## Global Constraints

- Text rotate: degrees may be any finite value (free rotation, NOT restricted to multiples of 90 — unlike page rotation). opacity 0..1. Rotation is about the text anchor (x,y).
- Existing non-rotated/opaque text output unchanged (no q/Q wrap when neither rotate nor opacity set).
- Outlines: page index validated against output page count; `/Dest` = `[pageRef /XYZ null null null]`. Item `/Count` = number of descendant items (positive = open). Nested children supported.
- Loaded outline = incremental update; created outline via create-op.
- Both engines for text; outline on both (create-op + set_outline WASM).
- Validate before mutation. Every task green: cargo + bun + typecheck. No root Cargo.toml. Rebuild wasm before bun (set_outline is a NEW export). pkg-web gitignored. Tests in `tests/`. Branch `m34-text-rotation-outlines`; not on master.

## File Structure

- Modify: `crates/core/src/draw.rs` — `Text` op gains `rotate`/`opacity`; `emit_text_block`/`emit_text_block_cid` gain `rotate`/`gs_key`; apply in `apply_draw_ops_json`.
- Modify: `crates/core/src/create.rs` — `Text` op gains `rotate`/`opacity`; same emit; `CreateOp::Outline`.
- Create: `crates/core/src/outline.rs` — `build_outline(doc_add, items, page_ref_of) -> ObjectId` (the /Outlines tree builder), `set_outline_json(data, json) -> Vec<u8>` (loaded incremental).
- Modify: `crates/core/src/lib.rs` — `mod outline;`, `set_outline` WASM export, fuzz_api.
- Modify: `src/generate/draw-queue.ts` — Text op `rotate`/`opacity`; outline op for create; `src/generate/page.ts` — drawText opts; `src/core/document.ts` — `setOutline` + save wiring + CoreWasm.setOutline; `src/core/wasm.ts`/`wasm-browser.ts` — setOutline wrapper.
- Tests: draw.rs/create.rs/outline.rs `#[cfg(test)]`, `tests/text-rotation.test.ts`, `tests/outline.test.ts`.

## Interfaces (cross-task contract)

- Text op gains `#[serde(default)] rotate: Option<f32>, #[serde(default)] opacity: Option<f32>` (both engines).
- `emit_text_block(out, font_key, x, y, size, color, text, line_height, rotate: Option<f32>, gs_key: Option<&str>)` and `emit_text_block_cid(..., rotate, gs_key)`.
- Outline wire: items = `[{"title":"...","page":i,"children":[...]?}]`. `set_outline(data, json) -> Vec<u8>`; create-op `{"op":"outline","items":[...]}`.
- `build_outline(doc: &mut Document, items: &[OutlineItem], page_ref: impl Fn(usize)->Option<ObjectId>) -> Result<ObjectId, String>` returns the /Outlines dict id.
- TS: `page.drawText(text, {..., rotate?: number, opacity?: number})`; `doc.setOutline(items: {title: string; page: number; children?: ...[]}[]): void`. `CoreWasm.setOutline(data, json)`.

---

### Task 1: Text rotation + opacity (both engines)

**Files:** `crates/core/src/draw.rs`, `crates/core/src/create.rs`.

- [ ] **Step 1: Failing tests**

```rust
// draw.rs
#[test]
fn rotated_text_emits_matrix() {
    let out = apply_draw_ops_json(FICHA,
        r#"[{"op":"text","page":0,"x":100,"y":100,"size":12,"font":"Helvetica","color":[0,0,0],"text":"hi","rotate":90}]"#,
        &[], &[], "[]").unwrap();
    let s = last_draw_stream_content(&out);
    assert!(s.contains(" cm"), "rotation must emit a cm matrix: {s}");
    assert!(s.contains("q") && s.contains("Q"), "rotated text wrapped in q/Q: {s}");
    assert!(s.contains("0 0 Td"), "rotated text uses Td 0 0 (cm positions): {s}");
}
#[test]
fn translucent_text_registers_extgstate() {
    let out = apply_draw_ops_json(FICHA,
        r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"wm","opacity":0.3}]"#,
        &[], &[], "[]").unwrap();
    let s = last_draw_stream_content(&out);
    assert!(s.contains("/BPG"), "opacity text references an ExtGState: {s}");
}
#[test]
fn plain_text_unchanged_no_wrap() {
    let out = apply_draw_ops_json(FICHA,
        r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#,
        &[], &[], "[]").unwrap();
    let s = last_draw_stream_content(&out);
    assert!(s.contains("50 700 Td"), "plain text keeps x y Td: {s}");
}
```

- [ ] **Step 2: Run — FAIL.**

- [ ] **Step 3: Implement**

- Add `#[serde(default)] rotate: Option<f32>` and `#[serde(default)] opacity: Option<f32>` to `DrawOp::Text` and `CreateOp::Text`.
- Change `emit_text_block` and `emit_text_block_cid` to take `rotate: Option<f32>, gs_key: Option<&str>`. New emit logic:
  ```
  let wrap = rotate.is_some() || gs_key.is_some();
  if wrap { out "q\n" }
  if let Some(k) = gs_key { out "/{k} gs\n" }
  out "BT\n"
  out "/{font_key} {size} Tf\n"
  out "{r} {g} {b} rg\n"
  out "{leading} TL\n"
  if let Some(deg) = rotate {
      let t = deg.to_radians(); let (s_, c_) = (t.sin(), t.cos());
      out "{cos} {sin} {-sin} {cos} {x} {y} cm\n"   // place + rotate
      out "0 0 Td\n"
  } else {
      out "{x} {y} Td\n"
  }
  ... Tj / T* lines ...
  out "ET\n"
  if wrap { out "Q\n" }
  ```
  NOTE: the `cm` must come AFTER `BT`? No — `cm` is a general graphics-state operator and is allowed inside a text object in PDF, but cleanest is to emit `cm` BEFORE `BT` (graphics state set first), then `BT … 0 0 Td`. Put the rotation `cm` right after the optional `gs` and BEFORE `BT`. Adjust the order: `q` → `gs` → `cm` (if rotate) → `BT` → `Tf/rg/TL` → `Td (0 0 if rotate else x y)` → text → `ET` → `Q`. Use `fmt_num`.
- Update ALL call sites of `emit_text_block`/`emit_text_block_cid` in draw.rs and create.rs to pass the new args (existing non-rotate calls pass `None, None`).
- In both engines' Text handling: if `opacity` is Some, register an ExtGState (`BPG{gs_counter}`, reuse the shape pattern) and pass its key; pass `rotate` through.
- Validation: `opacity` (if Some) in 0..1; `rotate` (if Some) finite.

- [ ] **Step 4: Run — PASS, full suite.** (existing text tests must pass — they pass `None,None` and keep `x y Td`.)

- [ ] **Step 5: Commit**

```bash
git checkout -b m34-text-rotation-outlines
git add crates/core/src/draw.rs crates/core/src/create.rs
git commit -m "feat(text): rotation + opacity on drawText (loaded and created)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Outlines / bookmarks (Rust)

**Files:** Create `crates/core/src/outline.rs`; modify `crates/core/src/lib.rs`, `crates/core/src/create.rs`.

- [ ] **Step 1: Failing tests**

```rust
// outline.rs
#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const FICHA: &[u8] = include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
    #[test]
    fn sets_outline_with_dest() {
        let out = set_outline_json(FICHA, r#"[{"title":"Intro","page":0},{"title":"End","page":0}]"#).unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental
        let doc = Document::load_mem(&out).unwrap();
        let cat = doc.catalog().unwrap();
        let outlines_ref = cat.get(b"Outlines").unwrap().as_reference().unwrap();
        let outlines = doc.get_object(outlines_ref).unwrap().as_dict().unwrap();
        assert!(outlines.has(b"First") && outlines.has(b"Last"));
        let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
        assert!(count >= 2);
    }
    #[test]
    fn nested_outline_links_parent() {
        let out = set_outline_json(FICHA, r#"[{"title":"Ch1","page":0,"children":[{"title":"1.1","page":0}]}]"#).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        // first top item has a First child whose Parent points back
        let cat = doc.catalog().unwrap();
        let outlines = doc.get_object(cat.get(b"Outlines").unwrap().as_reference().unwrap()).unwrap().as_dict().unwrap();
        let first = doc.get_object(outlines.get(b"First").unwrap().as_reference().unwrap()).unwrap().as_dict().unwrap();
        assert!(first.has(b"First"), "nested item must have a child");
    }
    #[test]
    fn outline_rejects_bad_page() {
        assert!(set_outline_json(FICHA, r#"[{"title":"x","page":9999}]"#).is_err());
    }
}
```

- [ ] **Step 2: Run — FAIL.**

- [ ] **Step 3: Implement**

- `#[derive(Deserialize)] pub struct OutlineItem { pub title: String, pub page: usize, #[serde(default)] pub children: Vec<OutlineItem> }`.
- `pub fn build_outline(doc: &mut Document, items: &[OutlineItem], page_ref: &dyn Fn(usize) -> Option<ObjectId>) -> Result<ObjectId, String>`:
  - Reserve the /Outlines root id (`doc.new_object_id()`).
  - Recursive helper builds a list of sibling item ids under a given parent id: for each item reserve an id; recurse children with the item id as parent; set `/Title` (string_literal), `/Parent` (Reference), `/Dest [page_ref(page) /XYZ Null Null Null]` (error if page_ref returns None), and if children: `/First`, `/Last`, `/Count` (negative or positive count of descendants — use positive = open; e.g. count of immediate+nested visible). Link siblings via `/Next`/`/Prev`.
  - Set root `/Type /Outlines`, `/First`, `/Last`, `/Count` (total top-level... use count of all descendants, positive).
  - Return root id.
- `pub fn set_outline_json(data: &[u8], json: &str) -> Result<Vec<u8>, String>`: parse items; load doc; validate every page index `< doc.get_pages().len()`; `IncrementalDocument::create_from`; `page_ref` resolves index → the prev doc's sorted page ObjectId; `let root = build_outline(&mut inc.new_document, &items, &page_ref)?`; clone the catalog into new_document and set `/Outlines` → root (catalog mutation on incremental — mirror how metadata sets the trailer Info, but here it's the catalog /Root dict: get the Root ref, clone it into new_document, set Outlines). Save.
- `lib.rs`: `mod outline;` + `#[wasm_bindgen] pub fn set_outline(data, json) -> Result<Vec<u8>, JsError>`; fuzz_api.
- create.rs: add `CreateOp::Outline { items: Vec<crate::outline::OutlineItem> }`; in `create_document_json`, after building pages + catalog, if an outline op is present, `build_outline(&mut doc, &items, &|i| page_ids.get(i).copied())` and set `catalog /Outlines` → root (validate indices first; add a no-op arm in the content match).

> VERIFY: lopdf catalog access for incremental (`doc.catalog()` is read-only; to mutate on incremental, get the Root reference from trailer/prev, clone the catalog dict into new_document via opt_clone_object_to_new_document, set Outlines). The set_outline tests are the gate.

- [ ] **Step 4: Run — PASS, full suite.**

- [ ] **Step 5: Commit** (`feat(outline): document outline/bookmarks on loaded and created PDFs`)

---

### Task 3: TypeScript — drawText rotate/opacity + doc.setOutline

**Files:** `src/generate/draw-queue.ts`, `src/generate/page.ts`, `src/core/document.ts`, `src/core/wasm.ts`, `src/core/wasm-browser.ts`.

- [ ] **Step 1: Rebuild wasm** (set_outline is a new export).

- [ ] **Step 2: Failing tests** (`tests/text-rotation.test.ts`, `tests/outline.test.ts`)

```ts
// text-rotation: rotate + opacity round-trip on created + loaded; validation (opacity out of range throws)
// outline: doc.setOutline([{title:"A",page:0},{title:"B",page:0,children:[{title:"B.1",page:0}]}]) on created (>=1 page) + loaded → save → reload valid; (structural assertion lives in Rust tests)
```

- [ ] **Step 3: Implement**

- `draw-queue.ts`: extend `TextOp` with `rotate?: number; opacity?: number`; `pushText` forwards them when present. Add an outline create-op channel: `setOutline(items)` stores the items; `toCreatePayload` prepends `{op:"outline", items}` when set (like metadata). Also expose the items so document.save() create-branch can include them. (Mirror the metadata op handling from M26.)
- `page.ts`: `drawText` opts gain `rotate?: number` and `opacity?: number`; validate opacity 0..1 (RangeError) + rotate finite; forward to pushText.
- `document.ts`: `setOutline(items: OutlineItem[]): void` — store on the doc; create mode → push the outline create-op via the draw queue; load mode → set a flag and in `save()` (load branch) call `bytes = this.wasm.setOutline(bytes, JSON.stringify(items))` (after draw/metadata). Add `setOutline(data, json)` to `CoreWasm`. Define + export an `OutlineItem` type (`{title:string; page:number; children?:OutlineItem[]}`) from `src/index.ts`.
- `wasm.ts` + `wasm-browser.ts`: `setOutline(data, json)` wrapper (browser ensureInitialized first).

- [ ] **Step 4: Run focused + full + typecheck + cargo. Green.**

- [ ] **Step 5: Commit** (`feat(text,outline): drawText rotate/opacity + doc.setOutline TS API`)

---

### Task 4: Docs + version 0.12.0

- [ ] **Step 1: Docs** — `generating.md`: "Rotated & translucent text" + "Outlines / bookmarks" sections with examples. `limitations.md`: text rotation/opacity + outlines now SUPPORTED. `from-pdf-lib.md`: parity (rotate/opacity drawText options; setOutline vs pdf-lib's manual outline). `SKILL.md` + `README.md`.
- [ ] **Step 2: Version** 0.12.0 (package.json + Cargo.toml). `CHANGELOG.md` `0.12.0`: "Text rotation & opacity (`drawText({rotate, opacity})`) and document outlines/bookmarks (`doc.setOutline()`), on loaded and created PDFs."
- [ ] **Step 3: TypeDoc regen if clean.** Also `git add` this milestone's plan doc.
- [ ] **Step 4: Final verify (cargo + bun + typecheck) + commit** (`docs(text,outline): document rotation/opacity + outlines; release 0.12.0`).

---

## Self-Review

**Spec coverage:** text rotate+opacity both engines + emit helpers (T1), outlines loaded (set_outline WASM) + created (create-op) + tree builder (T2), TS drawText opts + setOutline (T3), docs/version (T4).

**Risk callouts:** (1) emit_text_block/cid signature change → update ALL call sites (both engines, standard + embedded fonts); (2) cm order (q → gs → cm → BT → Td 0 0); (3) incremental catalog mutation for set_outline (clone Root catalog dict into new_document, set /Outlines); (4) outline /Next/Prev/First/Last/Parent/Count linkage; (5) page_ref resolution (created page_ids vs loaded prev sorted pages).

**Type consistency:** `OutlineItem` {title,page,children} identical Rust↔TS. Text op `rotate`/`opacity` identical across DrawOp/CreateOp/TS. `emit_text_block(...rotate,gs_key)` + `emit_text_block_cid(...rotate,gs_key)` signatures used by both engines. `set_outline(data,json)` consistent across outline.rs/lib.rs/CoreWasm/wasm wrappers.
