# Milestone M31: Embed PDF Pages as XObject (drawPage) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Draw a page from an existing PDF onto another page as a Form XObject — for watermarks, letterhead overlays, N-up, stamping. `doc.embedPdfPage(src, pageIndex)` → handle; `page.drawPage(handle, {x,y,width,height})`.

**Architecture:** A source PDF page is converted into a Form XObject: its content stream(s) concatenated as the XObject content, its resolved Resources subtree deep-copied into the target document via a recursive id-remapping importer (`import_object_tree`), `/BBox` from the MediaBox, `/Matrix` translating the MediaBox origin to 0,0. The Form XObject is then drawn with `q cm /key Do Q` (cm scales BBox→placement) — the same mechanism as images. Source PDFs travel from TS to WASM on the EXISTING image blob channel (concatenated bytes + offset/length), so no new WASM args. A new `page` draw op (in both draw.rs and create.rs) carries the source offset/length + source page index + placement.

**Tech Stack:** Rust 2024, lopdf 0.41 (`renumber`-free targeted copy via recursive importer; `get_pages`, `get_page_contents`, `get_page_resources`, `get_object`, `add_object`, `dereference`), flate2; TS ESM; Bun + cargo.

## Global Constraints

- Deep-copy ONLY the page's reachable subtree (content + Resources graph), NOT the whole source document. Use a recursive importer with a visited-map (handles shared refs + breaks cycles).
- The importer must work into both `inc.new_document` (loaded target, incremental) and a plain `doc` (created target) — take a `&mut Document` (both are `Document`). New object ids come from that target doc's `add_object`/`new_object_id`, which allocate safely above existing ids (incremental new_document is initialized above the prev doc's max).
- Form XObject: `/Type /XObject /Subtype /Form /FormType 1 /BBox [0 0 W H] /Matrix [1 0 0 1 -x0 -y0] /Resources <copied> ` + concatenated decompressed content (or keep compressed with proper Filter — simplest: decompress all content streams and store the Form content uncompressed or re-flate it).
- Placement cm = `[width/W 0 0 height/H x y]` where W=x1-x0, H=y1-y0 from the source MediaBox. Reuse/extend the image emit (`emit_image_op` emits `w 0 0 h x y cm /key Do Q`; for a Form with BBox [0 0 W H] the scale must be width/W not width — so compute the cm explicitly, do NOT pass raw width/height to emit_image_op).
- Source page index out of range / unparseable source → error before mutation.
- Reuse the image blob channel: the `page` op carries `imageOffset`/`imageLength` (into the images blob) + `srcPage` + x/y/width/height. No new WASM signature.
- Every task green: cargo + bun + typecheck. No root Cargo.toml. Rebuild wasm before bun. pkg-web gitignored. Tests in `tests/`. Branch `m31-embed-pdf-pages`; not on master.

## File Structure

- Create: `crates/core/src/embed.rs` — `import_object_tree`, `embed_page_as_xobject`.
- Modify: `crates/core/src/lib.rs` — `mod embed;` (+ fuzz_api if useful).
- Modify: `crates/core/src/draw.rs` — `DrawOp::Page` variant; validate; build via embed + emit Do.
- Modify: `crates/core/src/create.rs` — `CreateOp::Page` variant; same.
- Modify: `src/generate/image.ts` or new `src/generate/embedded-page.ts` — `EmbeddedPdfPage` handle; `src/core/document.ts` `embedPdfPage`; `src/generate/page.ts` `drawPage`; `src/generate/draw-queue.ts` `PageOp` + push (rides image blob).
- Tests: `crates/core/src/embed.rs` + draw/create `#[cfg(test)]`, `tests/embed-pdf.test.ts`. Fixtures: existing PDFs under `tests/fixtures/`.

## Interfaces (cross-task contract)

- `pub fn import_object_tree(dst: &mut Document, src: &Document, src_id: ObjectId, map: &mut HashMap<ObjectId, ObjectId>) -> Result<ObjectId, String>` — recursively copies `src_id` and everything it references into `dst`, returns the new id; idempotent via `map`.
- `pub fn embed_page_as_xobject(dst: &mut Document, src_bytes: &[u8], src_page_index: usize) -> Result<(ObjectId, f32, f32), String>` — returns (form_xobject_id, bbox_width, bbox_height). Builds the Form XObject in `dst`.
- Wire op (both engines): `{"op":"page","page":i,"x":..,"y":..,"width":..,"height":..,"imageOffset":o,"imageLength":l,"srcPage":k}`.
- TS: `doc.embedPdfPage(src: Uint8Array, pageIndex: number): Promise<EmbeddedPdfPage>` (handle holds src bytes, srcPage, intrinsic width/height); `page.drawPage(embedded, {x,y,width?,height?})`. DrawQueue `pushPage(...)` rides the image blob.

---

### Task 1: Rust core — import_object_tree + embed_page_as_xobject

**Files:** Create `crates/core/src/embed.rs`; `mod embed;` in lib.rs.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Document, Object};
    const SRC: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn embeds_page_as_form_xobject() {
        let mut dst = Document::with_version("1.7");
        let (xid, w, h) = embed_page_as_xobject(&mut dst, SRC, 0).unwrap();
        assert!(w > 0.0 && h > 0.0);
        let xobj = dst.get_object(xid).unwrap().as_stream().unwrap();
        assert_eq!(xobj.dict.get(b"Subtype").unwrap().as_name().unwrap(), b"Form");
        assert!(xobj.dict.has(b"BBox"));
        assert!(xobj.dict.has(b"Resources"), "form must carry copied resources");
        // content present
        let content = xobj.decompressed_content().unwrap_or_else(|_| xobj.content.clone());
        assert!(!content.is_empty(), "form content must be non-empty");
    }

    #[test]
    fn embed_rejects_page_out_of_range() {
        let mut dst = Document::with_version("1.7");
        assert!(embed_page_as_xobject(&mut dst, SRC, 9999).is_err());
    }

    #[test]
    fn import_object_tree_dedupes_shared_refs() {
        // importing the same id twice via the map returns the same new id
        let src = Document::load_mem(SRC).unwrap();
        let mut dst = Document::with_version("1.7");
        let mut map = std::collections::HashMap::new();
        let (_, page_id) = src.get_pages().into_iter().next().unwrap();
        let a = import_object_tree(&mut dst, &src, page_id, &mut map).unwrap();
        let b = import_object_tree(&mut dst, &src, page_id, &mut map).unwrap();
        assert_eq!(a, b, "second import of same id must return cached new id");
    }
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml embed::tests`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
//! Embed a page from another PDF as a Form XObject (deep-copies the page's
//! content + resource subtree into the target document).
use std::collections::HashMap;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};

/// Recursively copy `src_id` and everything it references from `src` into `dst`,
/// remapping object ids. Idempotent via `map`. Breaks cycles by inserting the
/// new id into `map` BEFORE recursing into the object's children.
pub fn import_object_tree(
    dst: &mut Document,
    src: &Document,
    src_id: ObjectId,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Result<ObjectId, String> {
    if let Some(&new_id) = map.get(&src_id) {
        return Ok(new_id);
    }
    let new_id = dst.new_object_id();
    map.insert(src_id, new_id);
    let obj = src.get_object(src_id).map_err(|e| e.to_string())?.clone();
    let rewritten = import_object(dst, src, obj, map)?;
    dst.objects.insert(new_id, rewritten);
    Ok(new_id)
}

/// Deep-copy a single Object value, importing any nested references.
fn import_object(
    dst: &mut Document,
    src: &Document,
    obj: Object,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Result<Object, String> {
    Ok(match obj {
        Object::Reference(id) => Object::Reference(import_object_tree(dst, src, id, map)?),
        Object::Array(a) => Object::Array(
            a.into_iter().map(|o| import_object(dst, src, o, map)).collect::<Result<_,_>>()?,
        ),
        Object::Dictionary(d) => Object::Dictionary(import_dict(dst, src, d, map)?),
        Object::Stream(mut s) => {
            s.dict = import_dict(dst, src, s.dict, map)?;
            Object::Stream(s)
        }
        other => other,
    })
}

fn import_dict(
    dst: &mut Document,
    src: &Document,
    d: Dictionary,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Result<Dictionary, String> {
    let mut out = Dictionary::new();
    for (k, v) in d.into_iter() {
        out.set(k, import_object(dst, src, v, map)?);
    }
    Ok(out)
}

pub fn embed_page_as_xobject(
    dst: &mut Document,
    src_bytes: &[u8],
    src_page_index: usize,
) -> Result<(ObjectId, f32, f32), String> {
    let src = Document::load_mem(src_bytes).map_err(|e| e.to_string())?;
    let page_ids: Vec<ObjectId> = src.get_pages().into_values().collect();
    let page_id = *page_ids.get(src_page_index)
        .ok_or_else(|| format!("source page {src_page_index} out of range"))?;

    // MediaBox (resolve inherited by walking /Parent if absent)
    let media = resolve_media_box(&src, page_id)?;
    let (x0, y0, x1, y1) = (media[0], media[1], media[2], media[3]);
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0.0 || h <= 0.0 { return Err("source page has invalid MediaBox".to_string()); }

    // Concatenate decompressed content streams.
    let mut content: Vec<u8> = Vec::new();
    for cid in src.get_page_contents(page_id) {
        if let Ok(stream) = src.get_object(cid).and_then(|o| o.as_stream()) {
            let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
            content.extend_from_slice(&bytes);
            content.push(b'\n');
        }
    }

    // Import the page's resolved Resources subtree.
    let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
    let resources_obj: Object = {
        // get_page_resources returns (Option<&Dictionary> inline, Vec<ObjectId> referenced dicts)
        // Simplest robust path: find the page's /Resources value (ref or inline), import it.
        let page_dict = src.get_dictionary(page_id).map_err(|e| e.to_string())?;
        match page_dict.get(b"Resources") {
            Ok(Object::Reference(rid)) => Object::Reference(import_object_tree(dst, &src, *rid, &mut map)?),
            Ok(Object::Dictionary(d)) => Object::Dictionary(import_dict(dst, &src, d.clone(), &mut map)?),
            _ => {
                // inherited resources: walk parents
                match inherited_resources(&src, page_id) {
                    Some(Object::Reference(rid)) => Object::Reference(import_object_tree(dst, &src, rid, &mut map)?),
                    Some(Object::Dictionary(d)) => Object::Dictionary(import_dict(dst, &src, d, &mut map)?),
                    _ => Object::Dictionary(Dictionary::new()),
                }
            }
        }
    };

    let form = dictionary! {
        "Type" => Object::Name(b"XObject".to_vec()),
        "Subtype" => Object::Name(b"Form".to_vec()),
        "FormType" => Object::Integer(1),
        "BBox" => Object::Array(vec![
            Object::Real(0.0), Object::Real(0.0), Object::Real(w), Object::Real(h),
        ]),
        "Matrix" => Object::Array(vec![
            Object::Real(1.0), Object::Real(0.0), Object::Real(0.0),
            Object::Real(1.0), Object::Real(-x0), Object::Real(-y0),
        ]),
        "Resources" => resources_obj,
    };
    let mut stream = lopdf::Stream::new(form, content);
    // compress
    stream.compress().ok();
    let xid = dst.add_object(Object::Stream(stream));
    Ok((xid, w, h))
}

fn resolve_media_box(src: &Document, page_id: ObjectId) -> Result<[f32;4], String> {
    let mut cur = Some(page_id);
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1; if guard > 64 { break; }
        let d = src.get_dictionary(id).map_err(|e| e.to_string())?;
        if let Ok(mb) = d.get(b"MediaBox") {
            let arr = src.dereference(mb).map_err(|e| e.to_string())?.1.as_array().map_err(|e| e.to_string())?;
            if arr.len() == 4 {
                let f = |o: &Object| src.dereference(o).ok().and_then(|(_,v)| v.as_float().ok()).unwrap_or(0.0);
                return Ok([f(&arr[0]), f(&arr[1]), f(&arr[2]), f(&arr[3])]);
            }
        }
        cur = d.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Err("source page has no MediaBox".to_string())
}

fn inherited_resources(src: &Document, page_id: ObjectId) -> Option<Object> {
    let mut cur = src.get_dictionary(page_id).ok()?.get(b"Parent").and_then(Object::as_reference).ok();
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1; if guard > 64 { break; }
        let d = src.get_dictionary(id).ok()?;
        if let Ok(r) = d.get(b"Resources") { return Some(r.clone()); }
        cur = d.get(b"Parent").and_then(Object::as_reference).ok();
    }
    None
}
```
> VERIFY against lopdf 0.41: `Stream::compress` / `decompressed_content` method names; `as_stream`/`as_array`/`as_float`/`as_reference` signatures; `dst.objects.insert` is valid (objects is pub BTreeMap). `new_object_id` increments max_id. Adjust to compile. The 3 tests are the gate.

- [ ] **Step 4: Run — expect PASS, full suite**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: green, pristine.

- [ ] **Step 5: Commit**

```bash
git checkout -b m31-embed-pdf-pages
git add crates/core/src/embed.rs crates/core/src/lib.rs
git commit -m "feat(embed): import a source PDF page as a Form XObject

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Wire the `page` draw op into draw.rs + create.rs

**Files:** `crates/core/src/draw.rs`, `crates/core/src/create.rs`.

- [ ] **Step 1: Write failing tests**

draw.rs (loaded) + create.rs (created): embed page 0 of a fixture onto the target; assert the target page's XObject resources contain a Form XObject and the draw stream references `/BPp0 Do`.

```rust
// draw.rs test sketch
#[test]
fn draws_embedded_pdf_page() {
    let src = FICHA; // reuse as the source PDF
    let len = src.len();
    let json = format!(r#"[{{"op":"page","page":0,"x":0,"y":0,"width":300,"height":400,"imageOffset":0,"imageLength":{len},"srcPage":0}}]"#);
    let out = apply_draw_ops_json(FICHA, &json, src, &[], "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    // assert some XObject in page 0 resources is /Subtype /Form, and draw stream has "/BPp0 Do"
    assert!(has_form_xobject_and_do(&out));
}
```
(Write the helper to walk page0 Resources/XObject for a Form subtype + grep the last draw stream for `/BPp` and ` Do`.)

- [ ] **Step 2: Run — expect FAIL**

- [ ] **Step 3: Implement**

- Add `DrawOp::Page { page, x, y, width, height, image_offset (rename imageOffset), image_length (rename imageLength), src_page (rename srcPage) }` and the analogous `CreateOp::Page`.
- Validation: `page < page_count`; `image_offset + image_length <= images.len()` (checked_add); width/height finite & > 0.
- In the per-page processing: for a Page op, slice the source bytes from the images blob, call `crate::embed::embed_page_as_xobject(<target_doc>, src_bytes, src_page)` → `(xid, bw, bh)`. Register under a `BPp{n}` key (distinct from BPI/BPF/BPE/BPG). Emit the draw block: `q\n {width/bw} 0 0 {height/bh} {x} {y} cm\n /BPp{n} Do\n Q\n` (compute the scale from the returned bbox dims; use `fmt_num`). Do NOT reuse emit_image_op directly (it would scale by width not width/bw).
- draw.rs target = `inc.new_document`; create.rs target = `doc`. Same `embed_page_as_xobject(&mut <doc>, ...)`.
- Register the Form XObject in the page's XObject resources exactly like images (same register_xobject path / xobject_res).

- [ ] **Step 4: Run — expect PASS, full suite**

- [ ] **Step 5: Commit** (`feat(embed): drawPage op embeds a source page onto loaded and created pages`)

---

### Task 3: TypeScript API — embedPdfPage + drawPage

**Files:** `src/generate/embedded-page.ts` (new), `src/core/document.ts`, `src/generate/page.ts`, `src/generate/draw-queue.ts`.

- [ ] **Step 1: Rebuild wasm.**
- [ ] **Step 2: Failing test** (`tests/embed-pdf.test.ts`): `const src = readFileSync(FIXTURE); const doc = await PdfDocument.create(); const embedded = await doc.embedPdfPage(src, 0); const page = doc.addPage(); page.drawPage(embedded, {x:0,y:0,width:300,height:400}); const out = await doc.save(); const re = await PdfDocument.load(out); expect(re.getPageCount()).toBe(1);` Plus a loaded-target variant.
- [ ] **Step 3: Implement**
  - `EmbeddedPdfPage` handle: holds `bytes: Uint8Array`, `srcPage: number`, and intrinsic `width`/`height` (read via `wasm.readPages(src)[pageIndex]`).
  - `doc.embedPdfPage(src, pageIndex)`: parse `readPages(src)` for the page's width/height; return the handle. (No wasm embed call yet — the embed happens at save via the op.)
  - `page.drawPage(embedded, {x, y, width?, height?})`: default width/height to the embedded intrinsic size; push a `PageOp` onto the draw queue with the source bytes riding the image blob (mirror `pushImage`: store the bytes as an image-blob entry and emit `imageOffset/imageLength`, plus `srcPage`).
  - `draw-queue.ts`: add `PageOp` type + `pushPage(page, bytes, {x,y,width,height,srcPage})` that registers the bytes in the image blob (reuse the existing image-entry mechanism so the offset/length are computed in `buildDrawOps`) and emits `{op:"page", page, x, y, width, height, imageOffset, imageLength, srcPage}`. Look at how `pushImage` stores an `ImageEntry` and how `buildDrawOps` assigns offsets — extend that so a page-entry also contributes bytes to the same blob and gets `imageOffset`/`imageLength`.
- [ ] **Step 4: Run focused + full + typecheck + cargo. Green.**
- [ ] **Step 5: Commit** (`feat(embed): embedPdfPage + page.drawPage TS API`)

---

### Task 4: Docs + version 0.9.0

**Files:** generating.md, limitations.md, from-pdf-lib.md, SKILL.md, README.md, CHANGELOG.md, package.json, Cargo.toml.

- [ ] **Step 1: Docs** — "Embed pages from other PDFs" section (`embedPdfPage` + `drawPage`; watermark/stamp/N-up examples). limitations.md: embedding source pages now SUPPORTED; note interactive form fields on embedded pages are flattened to static appearance (Form XObject content only). from-pdf-lib.md: parity with pdf-lib `embedPdf`/`drawPage`. SKILL.md + README.md.
- [ ] **Step 2: Version** 0.9.0 (package.json + Cargo.toml). CHANGELOG 0.9.0: "Embed PDF pages: `doc.embedPdfPage()` + `page.drawPage()` stamp a page from another PDF as a Form XObject (loaded + created)."
- [ ] **Step 3: TypeDoc regen if clean.**
- [ ] **Step 4: Final verify (cargo + bun + typecheck) + commit** (`docs(embed): document PDF page embedding; release 0.9.0`).

---

## Self-Review

**Spec coverage:** importer + Form-XObject build (T1), both engines + page op (T2), TS embedPdfPage/drawPage (T3), docs/version (T4).

**Risk callouts:** (1) `import_object_tree` must insert the new id into `map` BEFORE recursing (cycle safety) — done in the draft. (2) Form cm uses width/bbox_w, NOT raw width — do not reuse emit_image_op. (3) lopdf API names (`Stream::compress`, `decompressed_content`, `dereference`) must be verified at compile. (4) Inherited MediaBox/Resources resolved by walking /Parent. (5) If a source page's content/resources copy proves too fragile on real fixtures, this is the milestone to skip+document per the run policy.

**Type consistency:** `import_object_tree(dst,src,src_id,map)` and `embed_page_as_xobject(dst,src_bytes,src_page_index)->(ObjectId,f32,f32)` used by both engines. Page op fields `imageOffset/imageLength/srcPage` identical across DrawOp/CreateOp/TS. Resource key `BPp{n}` distinct from BPI/BPF/BPE/BPG.
