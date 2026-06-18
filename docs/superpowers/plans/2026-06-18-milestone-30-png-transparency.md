# Milestone M30: PNG Transparency (alpha → SMask) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Preserve PNG alpha as a PDF soft mask (`/SMask`) instead of discarding it, so transparent logos/watermarks composite correctly. Applies to both loaded and created PDFs.

**Architecture:** The hand-rolled PNG decoder (`crates/core/src/appearance.rs::png_image`) currently strips the alpha channel for color types 4 (gray+alpha) and 6 (RGBA). Extend it to also collect the alpha bytes into a separate plane. A new `build_image_xobjects(image, add)` helper builds the main color image XObject and, when an alpha plane is present, a DeviceGray `/SMask` image XObject, links them, and returns the main image's ObjectId. The two image-op call sites (draw.rs loaded, create.rs created) switch to this helper. JPEG and alpha-less PNG behavior is unchanged.

**Tech Stack:** Rust 2024, lopdf 0.41, flate2; TS ESM; Bun + cargo. (No new crate — extends the existing tested decoder; palette/interlace/16-bit PNGs remain unsupported as before.)

## Global Constraints

- Alpha preserved only for 8-bit, non-interlaced PNG color types 4 and 6 (the existing decoder's supported set). Palette (tRNS), interlaced, and 16-bit remain unsupported — document, don't crash.
- `/SMask` image is DeviceGray, same width/height, 8 BitsPerComponent, FlateDecode.
- Alpha-less images (JPEG, PNG types 0/2) produce NO SMask — output byte-identical to before.
- The new builder must work for both `inc.new_document.add_object` (draw.rs) and `doc.add_object` (create.rs) via a closure.
- Every task ends green: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`, `bun test`, `bun run typecheck`. No root Cargo.toml. Rebuild wasm before bun tests. pkg-web gitignored. Tests in `tests/`.
- Branch `m30-png-transparency`; not on master.

## File Structure

- Modify: `crates/core/src/appearance.rs` — `png_image` extracts alpha; `SignatureImage::Raw` carries `alpha: Option<Vec<u8>>`; new `build_image_xobjects`.
- Modify: `crates/core/src/draw.rs` — image op uses `build_image_xobjects`.
- Modify: `crates/core/src/create.rs` — image op uses `build_image_xobjects`.
- Tests: appearance.rs/draw.rs/create.rs `#[cfg(test)]`, `tests/png-transparency.test.ts`.

## Interfaces (cross-task contract)

- `SignatureImage::Raw { data: Vec<u8>, info: ImageInfo, alpha: Option<Vec<u8>> }` — `alpha` = one byte per pixel (width*height) when the source had an alpha channel.
- `pub fn build_image_xobjects(image: SignatureImage, add: &mut dyn FnMut(lopdf::Object) -> lopdf::ObjectId) -> lopdf::ObjectId` — adds the SMask (if any) and the main image; sets `/SMask` on the main dict; returns the main image ObjectId.
- Existing `build_signature_image_xobject(image) -> Stream` retained for the no-alpha single-stream path / existing tests (it ignores alpha).

---

### Task 1: Alpha extraction + SMask builder (appearance.rs)

**Files:** `crates/core/src/appearance.rs`.

- [ ] **Step 1: Write failing tests**

```rust
// in appearance.rs tests (reuse the existing tiny_rgba_png() helper)
#[test]
fn rgba_png_extracts_alpha_plane() {
    let img = signature_image(tiny_rgba_png()).unwrap();
    match img {
        SignatureImage::Raw { ref info, ref alpha, ref data } => {
            assert_eq!(info.color_space, "DeviceRGB");
            let px = (info.width * info.height) as usize;
            assert_eq!(data.len(), px * 3, "color data must be RGB (alpha stripped)");
            let a = alpha.as_ref().expect("RGBA png must yield an alpha plane");
            assert_eq!(a.len(), px, "alpha plane must be one byte per pixel");
        }
        _ => panic!("expected Raw"),
    }
}

#[test]
fn opaque_png_has_no_alpha() {
    // a color-type-2 (RGB, no alpha) PNG yields alpha == None
    let img = signature_image(tiny_rgb_png()).unwrap();
    if let SignatureImage::Raw { alpha, .. } = img {
        assert!(alpha.is_none(), "RGB png must not produce an alpha plane");
    } else { panic!("expected Raw"); }
}

#[test]
fn build_image_xobjects_sets_smask_for_alpha() {
    use lopdf::{Document, Object};
    let mut doc = Document::with_version("1.7");
    let img = signature_image(tiny_rgba_png()).unwrap();
    let mut add = |o: Object| doc.add_object(o);
    let main_id = build_image_xobjects(img, &mut add);
    let main = doc.get_object(main_id).unwrap().as_stream().unwrap();
    let smask_ref = main.dict.get(b"SMask").expect("main image must have /SMask").as_reference().unwrap();
    let smask = doc.get_object(smask_ref).unwrap().as_stream().unwrap();
    assert_eq!(smask.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), b"DeviceGray");
    assert_eq!(smask.dict.get(b"Subtype").unwrap().as_name().unwrap(), b"Image");
}

#[test]
fn build_image_xobjects_no_smask_for_opaque() {
    use lopdf::{Document, Object};
    let mut doc = Document::with_version("1.7");
    let img = signature_image(tiny_rgb_png()).unwrap();
    let mut add = |o: Object| doc.add_object(o);
    let main_id = build_image_xobjects(img, &mut add);
    let main = doc.get_object(main_id).unwrap().as_stream().unwrap();
    assert!(main.dict.get(b"SMask").is_err(), "opaque image must not have /SMask");
}
```
> If `tiny_rgb_png()` (a color-type-2 PNG) doesn't already exist in the test module, add a minimal one (mirror `tiny_rgba_png()` but IHDR color type 2 and 3-byte pixels). A 1×1 RGB PNG is fine.

- [ ] **Step 2: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml appearance::tests::rgba_png_extracts_alpha`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Change `SignatureImage::Raw` to `{ data, info, alpha: Option<Vec<u8>> }`. Update the `signature_image` PNG path and all constructors/matches (jpeg path is a different variant; `info()` unaffected).
- In `png_image`: for color types 4 and 6, while emitting the color components into `out`, ALSO push the alpha byte (the last src component of each pixel) into a separate `alpha: Vec<u8>`. For types 0/2, `alpha = None`. The color `out` keeps stripping alpha as today (out_components excludes alpha). Set `alpha = Some(..)` only for types 4/6. (Look at `push_png_output_row` — extend it or collect alpha in the row loop alongside it.)
- Add `build_image_xobjects(image, add)`:
  - JPEG → `add(Object::Stream(build_jpeg_image_xobject(...)))`, return id (no SMask).
  - Raw with `alpha=None` → `add(Object::Stream(build_raw_image_xobject(data, &info)))`, return id.
  - Raw with `alpha=Some(a)` → build the main RGB/Gray stream dict (as `build_raw_image_xobject` does) but BEFORE adding it: build a DeviceGray SMask stream (width=info.width, height=info.height, 8 bpc, FlateDecode, data = flate(a)), `let smask_id = add(smask_stream);` then set `main_dict.set("SMask", Object::Reference(smask_id))`, `let main_id = add(main_stream); return main_id`. Factor a small helper to build a raw-gray stream to avoid duplicating the flate+dict code, or reuse `build_raw_image_xobject` with a DeviceGray ImageInfo for the SMask and then you only need to inject /SMask into the main — but `build_raw_image_xobject` returns a Stream; you can mutate `stream.dict.set("SMask", ...)` before adding. Keep it DRY.

- [ ] **Step 4: Run — expect PASS, full suite**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: green, pristine. (Existing image tests must still pass — `build_signature_image_xobject` retained.)

- [ ] **Step 5: Commit**

```bash
git checkout -b m30-png-transparency
git add crates/core/src/appearance.rs
git commit -m "feat(images): extract PNG alpha and build /SMask soft-mask XObject

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Wire SMask into the image ops (draw.rs + create.rs)

**Files:** `crates/core/src/draw.rs`, `crates/core/src/create.rs`.

- [ ] **Step 1: Write failing tests**

```rust
// draw.rs tests — alpha image on a loaded page registers an image with /SMask
#[test]
fn loaded_image_with_alpha_has_smask() {
    let png = /* an RGBA PNG bytes; reuse the appearance tiny_rgba_png via a local copy or include a fixture */ rgba_png_bytes();
    let len = png.len();
    let json = format!(r#"[{{"op":"image","page":0,"x":10,"y":10,"width":20,"height":20,"imageOffset":0,"imageLength":{len}}}]"#);
    let out = apply_draw_ops_json(FICHA, &json, png, &[], "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    // find a BPI* image XObject in any page Resources and assert it has /SMask
    // (resolve Resources/XObject like the existing draws_image_on_page test)
    assert!(has_image_with_smask(&doc), "embedded alpha image should have /SMask");
}
```
(Mirror the create.rs `creates_doc_with_image` test for a created-page version asserting `/SMask`.)

- [ ] **Step 2: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml draw::tests::loaded_image_with_alpha_has_smask`
Expected: FAIL (current path strips alpha → no SMask).

- [ ] **Step 3: Implement**

- In draw.rs image-op handling: replace `let stream = build_signature_image_xobject(img); let xid = inc.new_document.add_object(Object::Stream(stream));` with `let xid = build_image_xobjects(img, &mut |o| inc.new_document.add_object(o));`. (Adjust to the exact local names; `img` already comes from `signature_image(bytes)`.)
- In create.rs image-op handling: same, with `doc.add_object`.
- Both: the returned `xid` is registered in the page's XObject resources under the `BPI{n}` key exactly as before. The SMask is referenced only from the main image dict, not registered in page resources (correct — SMask is an indirect object referenced by the image, not a named resource).
- Import `build_image_xobjects` where needed (both files already import from `crate::appearance`).

- [ ] **Step 4: Run — expect PASS, full suite**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
Expected: green, pristine. Existing image tests pass (opaque images still embed, just without SMask).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/draw.rs crates/core/src/create.rs
git commit -m "feat(images): embed PNG alpha as /SMask on loaded and created pages

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: TS test + docs + version 0.8.0

**Files:** `tests/png-transparency.test.ts`, `docs/site/src/content/docs/guides/generating.md`, `docs/site/src/content/docs/reference/limitations.md`, `skills/better-pdf/SKILL.md`, `README.md`, `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`.

- [ ] **Step 1: Rebuild wasm**

Run: `. ~/.cargo/env && bun run build:wasm` (no new exports; ensures fresh).

- [ ] **Step 2: TS regression test**

Add `tests/png-transparency.test.ts`: embed an RGBA PNG (add a tiny RGBA PNG fixture under `tests/fixtures/` or inline a Uint8Array of one), `page.drawImage` on a created page, `save()`, reload, and assert the document is valid + an image XObject with `/SMask` exists. If asserting `/SMask` from TS is awkward (no low-level access), at minimum assert the embed+save+reload round-trips without error and the output is larger than the same image embedded opaque — OR keep the structural assertion in the Rust tests (Task 2) and make the TS test a smoke test (embed transparent PNG → valid PDF). Prefer a real check if feasible.

- [ ] **Step 3: Docs + version**

- `generating.md`: note PNG transparency is preserved automatically (no API change — `embedPng` + `drawImage` just work; alpha becomes a soft mask).
- `limitations.md`: PNG alpha/transparency now SUPPORTED (was "alpha flattened to RGB"); keep the caveat that palette (indexed)/interlaced/16-bit PNGs are still unsupported.
- `SKILL.md` + `README.md`: mention transparent PNG support.
- `CHANGELOG.md` `0.8.0`: "PNG transparency: alpha channel preserved as a soft mask (/SMask) for RGBA and gray+alpha PNGs on embedded images."
- Bump `package.json` + `crates/core/Cargo.toml` to `0.8.0`.

- [ ] **Step 4: TypeDoc regen if it builds**; add api-reference if clean else note.

- [ ] **Step 5: Final verification + commit**

Run: `. ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml && bun test && bun run typecheck`
Expected: green.
```bash
git add docs/ skills/ README.md CHANGELOG.md package.json crates/core/Cargo.toml tests/png-transparency.test.ts
git commit -m "docs(images): document PNG transparency; release 0.8.0

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:** alpha extraction (T1), SMask builder (T1), both engines wired (T2), TS test + docs + version (T3). Opaque images unchanged (no SMask). Palette/interlace/16-bit still unsupported (documented).

**Placeholder scan:** Test fixtures (tiny_rgb_png, rgba_png_bytes) may need adding — instructed explicitly, not placeholders.

**Type consistency:** `SignatureImage::Raw` gains `alpha: Option<Vec<u8>>` — every match/constructor updated. `build_image_xobjects(image, add: &mut dyn FnMut(Object)->ObjectId) -> ObjectId` used identically in draw.rs and create.rs. SMask dict: DeviceGray / Image / 8bpc / FlateDecode / Width / Height matching the main image.
