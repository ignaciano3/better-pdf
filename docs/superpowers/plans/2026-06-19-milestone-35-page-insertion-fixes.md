# Milestone M35: Page Insertion + Metadata UTF-16BE + Palette PNG Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Three things. (A) Insert/append/remove/move pages on existing (loaded) PDFs, incrementally (preserving forms). (B) Fix non-ASCII document metadata via UTF-16BE. (C) Support palette (indexed) PNG embedding (incl. tRNS transparency). → v0.13.0.

**Architecture:**
- **Page insertion:** a new `insert_pages(data, ops_json)` WASM fn does incremental page-tree surgery — clone the `/Pages` node, edit `/Kids`, fix `/Count`; a blank page = new page object with `/MediaBox` + empty `/Contents` + `/Parent` + empty `/Resources`. TS exposes `doc.addPage` (loaded = append), `insertPage`, `removePage`, `movePage`. At save the structure step runs BEFORE `applyDrawOps`, so an appended page exists in the bytes and can be drawn on by index in the same save.
- **Metadata UTF-16BE:** `build_info_dict` writes text strings as UTF-16BE with a `FE FF` BOM (so non-Latin titles/authors survive); `read_metadata` detects the BOM and decodes UTF-16BE, else falls back to the current Latin-1/PDFDocEncoding `from_utf8_lossy`.
- **Palette PNG:** extend the PNG decoder to handle color type 3 (indexed): read `PLTE` (RGB palette) + optional `tRNS` (per-index alpha); expand index → RGB pixels, and when `tRNS` present produce an alpha plane → reuse the M30 `/SMask` path.

**Tech Stack:** Rust 2024, lopdf 0.41; TS ESM; Bun + cargo.

## Global Constraints

- Page insertion = incremental update (loaded docs); original bytes preserved as prefix; forms/links/etc. survive (page-tree edit only, no rebuild).
- **Drawable-append contract:** `doc.addPage()` on a loaded doc appends a blank page whose index = effective end; you CAN draw on it in the same `save()`. `insertPage`/`removePage`/`movePage` are queued structure ops applied at save; the live `getPage`/`getPageCount` reflect appends accurately, and insert/remove/move are reflected after save+reload (document this — do not pretend the live getter re-mirrors mid-document reorders).
- Save order (load mode): fill → flatten → **insert_pages (structure)** → applyDrawOps → setMetadata → setOutline.
- Metadata: write UTF-16BE+BOM for ALL text values (simplest + always-correct), OR only when non-ASCII (smaller for ASCII). Pick UTF-16BE-when-non-ASCII to keep ASCII output compact; read must handle both. Round-trip non-ASCII is the gate.
- Palette PNG keeps the existing "8-bit, non-interlaced" constraints; only adds color type 3. 16-bit/interlaced still unsupported.
- Every task green: cargo + bun + typecheck. No root Cargo.toml. Rebuild wasm before bun (insert_pages is a NEW export). pkg-web gitignored. Tests in `tests/`. Branch `m35-page-insertion-fixes`; not on master.

## File Structure

- Create: `crates/core/src/pagetree.rs` — `insert_pages_json(data, ops_json) -> Vec<u8>` (incremental page-tree ops).
- Modify: `crates/core/src/lib.rs` — `mod pagetree;`, `insert_pages` export, fuzz_api.
- Modify: `crates/core/src/metadata.rs` — UTF-16BE write + BOM-aware read.
- Modify: `crates/core/src/appearance.rs` — palette PNG in `png_image` (color type 3 + PLTE + tRNS → RGB + alpha).
- Modify (TS): `src/core/document.ts` (loaded addPage/insertPage/removePage/movePage + structure queue + save wiring + CoreWasm.insertPages), `src/core/wasm.ts`/`wasm-browser.ts` (insertPages wrapper).
- Tests: pagetree.rs/metadata.rs/appearance.rs `#[cfg(test)]`, `tests/page-insertion.test.ts`, `tests/metadata-unicode.test.ts`, `tests/png-palette.test.ts`.

## Interfaces (cross-task contract)

- `pub fn insert_pages_json(data: &[u8], ops_json: &str) -> Result<Vec<u8>, String>`. Wire ops: `{"op":"appendBlank","width":w,"height":h}`, `{"op":"insertBlank","index":i,"width":w,"height":h}`, `{"op":"removePage","index":i}`, `{"op":"movePage","from":a,"to":b}`. Applied in array order.
- WASM: `insert_pages(data, ops_json) -> Vec<u8>`.
- TS: `doc.addPage(size?)` (works on loaded now), `doc.insertPage(index, size?)`, `doc.removePage(index)`, `doc.movePage(from, to)`. `CoreWasm.insertPages(data, opsJson)`.

---

### Task 1: Rust — incremental page-tree ops (pagetree.rs)

**Files:** Create `crates/core/src/pagetree.rs`; modify `crates/core/src/lib.rs`.

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const FICHA: &[u8] = include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
    fn count(b: &[u8]) -> usize { Document::load_mem(b).unwrap().get_pages().len() }

    #[test]
    fn append_blank_adds_a_page() {
        let n = count(FICHA);
        let out = insert_pages_json(FICHA, r#"[{"op":"appendBlank","width":595,"height":842}]"#).unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental
        assert_eq!(count(&out), n + 1);
        // the new last page has the requested MediaBox
        let doc = Document::load_mem(&out).unwrap();
        let last = doc.get_pages().into_values().last().copied().unwrap();
        let mb = doc.get_dictionary(last).unwrap().get(b"MediaBox").unwrap().as_array().unwrap();
        assert!((mb[2].as_float().unwrap() - 595.0).abs() < 0.5);
    }
    #[test]
    fn insert_blank_at_zero_is_first() {
        let n = count(FICHA);
        let out = insert_pages_json(FICHA, r#"[{"op":"insertBlank","index":0,"width":100,"height":100}]"#).unwrap();
        assert_eq!(count(&out), n + 1);
        let doc = Document::load_mem(&out).unwrap();
        let first = doc.get_pages().into_values().next().copied().unwrap();
        let mb = doc.get_dictionary(first).unwrap().get(b"MediaBox").unwrap().as_array().unwrap();
        assert!((mb[2].as_float().unwrap() - 100.0).abs() < 0.5, "inserted page should be first");
    }
    #[test]
    fn remove_page_drops_one() {
        let n = count(FICHA);
        if n >= 1 {
            let out = insert_pages_json(FICHA, r#"[{"op":"removePage","index":0}]"#).unwrap();
            assert_eq!(count(&out), n - 1);
        }
    }
    #[test]
    fn move_page_reorders() {
        let n = count(FICHA);
        if n >= 2 {
            let out = insert_pages_json(FICHA, r#"[{"op":"movePage","from":0,"to":1}]"#).unwrap();
            assert_eq!(count(&out), n);
        }
    }
    #[test]
    fn errors_on_out_of_range_index() {
        assert!(insert_pages_json(FICHA, r#"[{"op":"removePage","index":9999}]"#).is_err());
    }
}
```

- [ ] **Step 2: Run — FAIL.**

- [ ] **Step 3: Implement**

Use `IncrementalDocument` (like draw.rs/metadata.rs). Approach:
- Load doc; get the ordered page ObjectIds (`get_pages().into_values()`), and find the `/Pages` tree root id (from catalog `/Pages`). NOTE: the page tree may be nested (intermediate `/Pages` nodes). To keep this robust and simple, FLATTEN: build a single `/Pages` root whose `/Kids` is the ordered list of leaf page ids, set each leaf page's `/Parent` to that root, set `/Count`. This sidesteps nested-tree splicing. (The leaf page objects are reused as-is — their content/resources/annots are untouched, so forms/links survive.)
- Maintain an ordered `Vec<PageEntry>` where a PageEntry is either an existing leaf ObjectId or a new blank page id. Apply ops in order:
  - appendBlank: push a new blank page.
  - insertBlank{index}: insert at index (index in `0..=len`; out of range → Err).
  - removePage{index}: remove (index `< len`; else Err).
  - movePage{from,to}: remove at `from`, insert at `to` (both in range).
- A blank page object: `dictionary!{ Type Page, Parent <root>, MediaBox [0 0 w h], Resources <<>>, Contents <empty stream id> }`. Add an empty content stream object.
- Build the new flattened `/Pages` root in `inc.new_document` (reuse the existing catalog's Pages id if you can mutate it, OR create a new Pages node and point the catalog `/Pages` at it). Simplest incremental: clone the existing `/Pages` root id into new_document, set its `/Kids` = new ordered refs, `/Count` = len; set each entry page's `/Parent` to the root id (clone each touched page? only NEW blank pages need /Parent set to the root; existing pages already have a /Parent — but if you flattened under a possibly-different root id you must update their /Parent. To avoid touching every existing page, REUSE the existing top `/Pages` root id as the flattened root: clone it, set Kids/Count; existing leaf pages already point /Parent at it IF the tree was already flat (1-level). For nested trees, also clone+repoint affected pages.)

> VERIFY / SIMPLIFY: Most PDFs (incl. the FICHA fixture) have a single-level page tree (catalog → /Pages → /Kids = [leaf pages]). For v1, handle the single-level case robustly: reuse the existing `/Pages` root id, set its `/Kids` to the new ordered list + `/Count`, set each NEW blank page's `/Parent` to that root id, and for existing pages whose `/Parent` already is that root, no change. If the source has a NESTED page tree, either (a) also re-parent the moved existing leaves to the root, or (b) detect nesting and return an error "nested page trees not supported" for v1. Pick (a) if cheap, else (b) with the error documented. The tests use FICHA (flat) — make them pass; document whichever nested-tree behavior you ship.

- lib.rs: `mod pagetree;` + `#[wasm_bindgen] pub fn insert_pages(data, ops_json) -> Result<Vec<u8>, JsError>`; fuzz_api.

- [ ] **Step 4: Run — PASS, full suite.**

- [ ] **Step 5: Commit**

```bash
git checkout -b m35-page-insertion-fixes
git add crates/core/src/pagetree.rs crates/core/src/lib.rs
git commit -m "feat(pages): incremental append/insert/remove/move blank pages on loaded PDFs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: TS — loaded addPage/insertPage/removePage/movePage

**Files:** `src/core/document.ts`, `src/core/wasm.ts`, `src/core/wasm-browser.ts`.

- [ ] **Step 1: Rebuild wasm** (insert_pages new export).

- [ ] **Step 2: Failing test** (`tests/page-insertion.test.ts`)

```ts
import { expect, test } from "bun:test";
import { PdfDocument, PageSizes, rgb } from "../src/index.js";
import { readFileSync } from "node:fs";
const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("append a page to a loaded doc and draw on it", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  const n = doc.getPageCount();
  const page = doc.addPage(PageSizes.A4);            // works on loaded now
  page.drawText("Appended", { x: 50, y: 700, size: 24, color: rgb(0,0,0) });
  const out = await doc.save();
  const re = await PdfDocument.load(out);
  expect(re.getPageCount()).toBe(n + 1);
});
test("insertPage / removePage change count", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  const n = doc.getPageCount();
  doc.insertPage(0, PageSizes.A4);
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(n + 1);

  const doc2 = await PdfDocument.load(readFileSync(FIXTURE));
  doc2.removePage(0);
  const out2 = await doc2.save();
  expect((await PdfDocument.load(out2)).getPageCount()).toBe(n - 1);
});
```

- [ ] **Step 3: Implement**

- `CoreWasm.insertPages(data, opsJson)`; wrappers in wasm.ts + wasm-browser.ts (browser ensureInitialized).
- Document holds `private structureOps: PageStructureOp[] = []` and the appended PdfPage handles.
- `addPage(size)`: REMOVE the `mode !== "create"` throw. Create mode: existing behavior (pushAddPage). Load mode: push `{op:"appendBlank", width, height}` to `structureOps`; effective new index = `getPageCount()` (current loaded count + prior appends/inserts net); create a `PdfPage(index, w, h, 0, drawQueue)`, track it, return it (drawable).
- `insertPage(index, size)` (load only): push `{op:"insertBlank", index, width, height}`. `removePage(index)` (load only): push `{op:"removePage", index}`. `movePage(from, to)` (load only): push `{op:"movePage", from, to}`. (These return void; per the contract, live getPage may not re-mirror mid-doc reorders — that's fine for v1.)
- `getPageCount()` (load): `loadPages().length + netDelta(structureOps)` where append/insertBlank = +1, removePage = −1, movePage = 0.
- `save()` (load branch): after flatten, BEFORE applyDrawOps: `if (this.structureOps.length) bytes = this.wasm.insertPages(bytes, JSON.stringify(this.structureOps));`. Then the existing applyDrawOps (draws on appended pages by index now resolve). Keep metadata/outline after.
- Export a `PageStructureOp` type only if needed internally (not necessarily public).

- [ ] **Step 4: Run focused + full + typecheck + cargo. Green.**

- [ ] **Step 5: Commit** (`feat(pages): addPage/insertPage/removePage/movePage on loaded PDFs (TS)`)

---

### Task 3: Metadata UTF-16BE (non-ASCII)

**Files:** `crates/core/src/metadata.rs`.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn non_ascii_metadata_round_trips() {
    let out = set_metadata_json(FICHA, r#"{"title":"日本語のタイトル","author":"Renée"}"#).unwrap();
    let json = read_metadata_json(&out).unwrap();
    assert!(json.contains("日本語のタイトル"), "json: {json}");
    assert!(json.contains("Renée"), "json: {json}");
}
#[test]
fn ascii_metadata_still_round_trips() {
    let out = set_metadata_json(FICHA, r#"{"title":"Plain ASCII"}"#).unwrap();
    assert!(read_metadata_json(&out).unwrap().contains("Plain ASCII"));
}
```

- [ ] **Step 2: Run — FAIL (non-ASCII currently mangled).**

- [ ] **Step 3: Implement**

- In `build_info_dict`: for each value, if it `is_ascii()` keep `Object::string_literal(bytes)` (compact); else encode UTF-16BE with a leading `FE FF` BOM: `let mut b = vec![0xFE, 0xFF]; for u in v.encode_utf16() { b.extend_from_slice(&u.to_be_bytes()); }` then `Object::String(b, StringFormat::Hexadecimal)` (or Literal — pick what lopdf round-trips; hex avoids escaping issues with the BOM/null bytes — prefer `StringFormat::Hexadecimal`). Confirm the lopdf `Object::String(Vec<u8>, StringFormat)` variant + `StringFormat` enum names.
- In `get_str`: if the bytes start with `FE FF`, decode as UTF-16BE (`char::decode_utf16` over `u16::from_be_bytes` pairs after the BOM); else current `from_utf8_lossy`. Return the decoded String.

> VERIFY: lopdf `StringFormat` variants (`Literal`/`Hexadecimal`) and that `Object::String(bytes, StringFormat::Hexadecimal)` serializes as `<FEFF....>`. The round-trip test is the gate.

- [ ] **Step 4: Run — PASS, full suite.**

- [ ] **Step 5: Commit** (`fix(metadata): encode non-ASCII Info strings as UTF-16BE`)

---

### Task 4: Palette (indexed) PNG support

**Files:** `crates/core/src/appearance.rs`.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn decodes_palette_png() {
    // a minimal color-type-3 (indexed) PNG with a PLTE chunk
    let img = signature_image(tiny_palette_png()).unwrap();
    match img {
        SignatureImage::Raw { info, data, .. } => {
            assert_eq!(info.color_space, "DeviceRGB");
            assert_eq!(data.len(), (info.width * info.height) as usize * 3); // expanded to RGB
        }
        _ => panic!("expected Raw"),
    }
}
```
(Add a `tiny_palette_png()` helper: a real 1×1 (or 2×1) color-type-3 PNG with IHDR(bit_depth 8, color type 3), a PLTE chunk (≥1 RGB entry), IDAT (zlib of filtered index rows), IEND. If hand-constructing a valid one is error-prone, generate it once and paste the bytes. A tRNS variant for the alpha test is a stretch — at minimum cover PLTE→RGB.)

- [ ] **Step 2: Run — FAIL (color type 3 currently rejected: "unsupported PNG color type").**

- [ ] **Step 3: Implement**

- In `png_image`: accept color type 3. Read the `PLTE` chunk (RGB triples, indexed by palette index) and optional `tRNS` chunk (1 alpha byte per palette index; indices beyond tRNS length are opaque/255). For bit depths 1/2/4/8 indexed — at minimum support 8-bit indices (keep the existing 8-bit constraint; if <8-bit indexed is easy, add it, else document 8-bit-indexed only). For each pixel: look up `palette[index]` → RGB (3 bytes into `out`); if tRNS present, push `tRNS[index]` (or 255) into the alpha plane. So color type 3 → `out` = RGB (DeviceRGB), `alpha = Some(..)` only when tRNS present.
- This reuses the M30 alpha/SMask machinery (build_image_xobjects already handles `Raw{alpha:Some}`), so transparent palette PNGs get an `/SMask` for free.
- Update the color-type match + the `unfilter`/row handling (indexed rows are 1 byte/pixel pre-expansion; expand AFTER unfiltering). Keep palette/index bounds checks (index >= palette len → error or clamp; prefer error "PNG palette index out of range").

> Note: if extending the hand-rolled decoder for indexed is too fiddly, the user approved adding deps — you MAY pull in the `png` crate and route PNG decoding through it (returning RGB + optional alpha), but that's a larger refactor; prefer the targeted color-type-3 extension. Document the decision.

- [ ] **Step 4: Run — PASS, full suite.**

- [ ] **Step 5: Commit** (`feat(images): support palette (indexed) PNG with optional tRNS transparency`)

---

### Task 5: Docs + version 0.13.0

**Files:** generating.md, limitations.md, from-pdf-lib.md, SKILL.md, README.md, CHANGELOG.md, package.json, Cargo.toml.

- [ ] **Step 1: Docs**
  - generating.md: "Adding & removing pages" section (`addPage` on loaded, `insertPage`/`removePage`/`movePage`; note appended pages are drawable in the same save; insert/remove/move reflected after reload).
  - limitations.md: page insertion/removal now SUPPORTED; non-ASCII metadata now SUPPORTED (UTF-16BE); palette/indexed PNG now SUPPORTED (note: interlaced/16-bit PNG still unsupported; nested page trees [if you shipped error path] noted).
  - from-pdf-lib.md: parity for insertPage/removePage; note metadata Unicode + palette PNG.
  - SKILL.md + README.md: add the page methods + note the two fixes.
- [ ] **Step 2: Version** 0.13.0 (package.json + Cargo.toml). CHANGELOG 0.13.0: "Page insertion: add/insert/remove/move pages on existing PDFs (incremental, forms preserved). Fixes: non-ASCII metadata (UTF-16BE), palette/indexed PNG embedding (with tRNS transparency). Also `git add` this plan doc."
- [ ] **Step 3: TypeDoc regen if clean.** `git add` the M35 plan doc.
- [ ] **Step 4: Final verify (cargo + bun + typecheck) + commit** (`docs: document page insertion + metadata/PNG fixes; release 0.13.0`).

---

## Self-Review

**Spec coverage:** page-tree ops Rust (T1) + TS (T2); metadata UTF-16BE (T3); palette PNG (T4); docs/version (T5).

**Risk callouts:** (1) page-tree flattening / single-vs-nested tree — handle flat robustly, document nested behavior; (2) save() order: insert_pages BEFORE applyDrawOps so appended pages are drawable; (3) getPageCount net-delta on loaded; live getPage append-accurate only (documented); (4) lopdf `StringFormat::Hexadecimal` + `Object::String` for UTF-16BE; (5) palette index bounds + tRNS optional → reuse M30 SMask.

**Type consistency:** insert_pages ops `appendBlank/insertBlank/removePage/movePage` identical Rust↔TS. `insert_pages(data, ops_json)` consistent across pagetree.rs/lib.rs/CoreWasm/wrappers. UTF-16BE BOM `FE FF` written + detected symmetrically. Palette → `Raw{alpha}` reuses build_image_xobjects.
