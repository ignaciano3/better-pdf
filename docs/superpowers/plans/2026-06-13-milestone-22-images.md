# Milestone 22 — Images Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Embed JPEG/PNG images and draw them on pages of both loaded and created documents: `doc.embedJpg(bytes)` / `doc.embedPng(bytes)` → `PdfImage`, then `page.drawImage(image, { x, y, width, height })`.

**Architecture:** Reuse the existing signature-image pipeline — `appearance::signature_image(bytes) -> SignatureImage` (decodes JPEG + supported PNG) and `appearance::build_signature_image_xobject(img) -> Stream`. Add an `image` op to the draw/create op enums and an `images` blob parameter to `apply_draw_ops`/`create_document` (the same offset/length blob scheme `fill_fields` already uses). A new `image_info` WASM export returns intrinsic pixel dimensions so the TS `embed*` methods can populate `PdfImage`. The TS `DrawQueue` carries image bytes and builds the combined blob + offsets at `save()`.

**Tech Stack:** Rust (lopdf 0.41), wasm-bindgen, TypeScript ESM, bun test.

**Spec:** `docs/superpowers/specs/2026-06-12-pdf-generation-design.md` (M22 row + `InvalidImageError`).

**Environment:** `source "$HOME/.cargo/env"` before cargo/wasm-pack; `bun run build:wasm` after Rust changes. Baselines after M21 merge: cargo 58 pass, bun 67 pass / 4 skip / 0 fail, typecheck clean.

**Reuse references:**
- `crates/core/src/appearance.rs:238` `signature_image(&[u8]) -> Result<SignatureImage,String>`; `.info()` → `ImageInfo { width, height, color_space }`.
- `crates/core/src/appearance.rs:339` `build_signature_image_xobject(SignatureImage) -> Stream`.
- `crates/core/src/fill.rs:107-112` blob slice-by-offset/length pattern.
- `crates/core/src/draw.rs` — `emit_text_block`, `register_font`/`set_font`/`dict_mut`, q/Q wrap, page grouping.
- `src/forms/fields.ts` `FillQueue.toPayload()` — how the TS side concatenates image blobs and computes offsets.

---

### Task 1: Rust — image op in draw.rs (apply_draw_ops)

**Files:** Modify `crates/core/src/draw.rs`, `crates/core/src/lib.rs`

The `apply_draw_ops` export gains an `images: &[u8]` blob param. The `DrawOp` enum gains an `Image` variant. Per-page emission dispatches text vs image; image XObjects register under `/Resources/XObject`.

- [ ] **Step 1: extend the op enum.** Add to `DrawOp`:

```rust
    Image {
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(rename = "imageOffset")]
        image_offset: usize,
        #[serde(rename = "imageLength")]
        image_length: usize,
    },
```

- [ ] **Step 2: add shared helpers in draw.rs:**

```rust
/// Append a `q … cm /key Do Q` image-draw block. `(x, y)` is the lower-left of
/// the placed image; `width`/`height` are the drawn size in points.
pub(crate) fn emit_image_op(out: &mut Vec<u8>, xobj_key: &str, x: f32, y: f32, width: f32, height: f32) {
    out.extend_from_slice(b"q\n");
    out.extend_from_slice(
        format!(
            "{} 0 0 {} {} {} cm\n",
            fmt_num(width), fmt_num(height), fmt_num(x), fmt_num(y)
        ).as_bytes(),
    );
    out.extend_from_slice(format!("/{xobj_key} Do\n").as_bytes());
    out.extend_from_slice(b"Q\n");
}

/// Register `key -> xobject_id` under the page's /Resources/XObject.
pub(crate) fn register_xobject(
    inc: &mut IncrementalDocument,
    page_id: ObjectId,
    key: &str,
    xobject_id: ObjectId,
) -> Result<(), String> {
    let res_ref = match dict_mut(inc, page_id)?.get(b"Resources") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };
    match res_ref {
        Some(id) => {
            inc.opt_clone_object_to_new_document(id).map_err(|e| e.to_string())?;
            set_xobject(dict_mut(inc, id)?, key, xobject_id);
        }
        None => {
            let page = dict_mut(inc, page_id)?;
            if !page.has(b"Resources") {
                page.set("Resources", Object::Dictionary(Dictionary::new()));
            }
            let res = page.get_mut(b"Resources").and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
            set_xobject(res, key, xobject_id);
        }
    }
    Ok(())
}

fn set_xobject(res: &mut Dictionary, key: &str, xobject_id: ObjectId) {
    if !res.has(b"XObject") {
        res.set("XObject", Object::Dictionary(Dictionary::new()));
    }
    if let Ok(xo) = res.get_mut(b"XObject").and_then(Object::as_dict_mut) {
        xo.set(key.as_bytes().to_vec(), Object::Reference(xobject_id));
    }
}
```

- [ ] **Step 3: thread `images` through `apply_draw_ops_json`.** Change signature to `pub fn apply_draw_ops_json(data: &[u8], ops_json: &str, images: &[u8]) -> Result<Vec<u8>, String>`. In validation, for `Image` ops validate the page index AND that `image_offset + image_length <= images.len()` (err `"image range out of bounds"`) and that the bytes decode: `appearance::signature_image(slice)` — propagate its error (so unsupported images error before mutation).

- [ ] **Step 4: per-page emission dispatch.** Where the per-page stream is built, iterate that page's ops in original order. For `Text` → `emit_text_block` (unchanged). For `Image`:
  - slice `let bytes = &images[image_offset..image_offset+image_length];`
  - `let img = appearance::signature_image(bytes)?;`
  - build the XObject: `let stream = appearance::build_signature_image_xobject(img); let xid = inc.new_document.add_object(Object::Stream(stream));`
  - assign a unique key per image op: `let key = format!("BPI{img_counter}");` (increment a per-call counter so each image gets a distinct resource name across all pages)
  - `emit_image_op(&mut stream_content, &key, x, y, width, height);`
  - record `(key, xid)` to register on this page after the stream is built.
  Keep the q/Q wrap of original content and the font registration intact. After building the page stream, register fonts (as now) AND each `(key, xid)` via `register_xobject`.

- [ ] **Step 5: update lib.rs export + fuzz_api.** Change the wasm-bindgen `apply_draw_ops` to `pub fn apply_draw_ops(data: &[u8], ops_json: &str, images: &[u8])` forwarding `images`. Update the `fuzz_api` re-export signature. Update the existing `draw_ops` fuzz target to pass an empty `&[]` for images (or arbitrary). Existing text-only fuzz still valid.

- [ ] **Step 6: tests.** Add to draw.rs tests (need a tiny valid image — reuse the test image helper the appearance.rs tests use; grep appearance.rs tests for `tiny_rgba_png` or a JPEG fixture and call the same helper, or inline a minimal PNG). Tests:
  - `draws_image_on_page`: ops with one image op + a real PNG blob → output reloads; touched page `/Resources/XObject/BPI0` exists and is an Image XObject; page content contains `/BPI0 Do`.
  - `image_out_of_bounds_errors`: image_offset/length beyond blob → Err "out of bounds".
  - `invalid_image_bytes_error`: blob is `b"not an image"` → Err.
  - Update the existing text tests to pass `&[]` as the new images arg (they currently call `apply_draw_ops_json(FICHA, json)` — add the third arg).

- [ ] **Step 7:** `cargo test` — all pass (58 prior, adjusted call sites, +3 new). Iterate.
- [ ] **Step 8: commit** `feat(core): draw images on existing pages`

---

### Task 2: Rust — image op in create.rs + image_info export

**Files:** Modify `crates/core/src/create.rs`, `crates/core/src/lib.rs`

- [ ] **Step 1: extend `CreateOp`** with an `Image` variant matching DrawOp's (same fields; `imageOffset`/`imageLength` renames). Change `create_document_json` signature to `(ops_json: &str, images: &[u8])`.

- [ ] **Step 2: validation** — same as Task 1 for image ops (page range, blob bounds, decode check up front).

- [ ] **Step 3: per-page build** — when iterating a page's ops, for `Image` ops: slice blob, `signature_image`, `build_signature_image_xobject`, `doc.add_object`, assign `/BPI{counter}`, `emit_image_op` into content, and add the key→ref into that page's XObject resource dict (extend the resources dict built for fonts: add an `XObject` sub-dict alongside `Font`). Text ops unchanged.

- [ ] **Step 4: lib.rs** — change `create_document` wasm export to `(ops_json: &str, images: &[u8])`; update fuzz_api re-export and the `create_document` fuzz target (pass `&[]`).

- [ ] **Step 5: image_info export.** Add to `lib.rs`:

```rust
/// Return JSON `{"width":W,"height":H}` (intrinsic pixels) for a JPEG/PNG, or error.
#[wasm_bindgen]
pub fn image_info(data: &[u8]) -> Result<String, JsError> {
    appearance::signature_image(data)
        .map(|img| {
            let i = img.info();
            format!("{{\"width\":{},\"height\":{}}}", i.width, i.height)
        })
        .map_err(|e| JsError::new(&e))
}
```

(`appearance` is already a module; confirm `signature_image` and `ImageInfo` are `pub`.)

- [ ] **Step 6: tests** in create.rs:
  - `creates_doc_with_image`: addPage + image op + PNG blob → reloads; page `/Resources/XObject/BPI0` present; content has `/BPI0 Do`.
  - Update existing create tests to pass `&[]` as new images arg.
  Add an image_info test (can live in lib.rs or a small test in appearance.rs tests using the existing tiny image helper): asserts width/height for the tiny PNG.

- [ ] **Step 7:** `cargo test` all pass. **Step 8: commit** `feat(core): draw images on created pages + image_info`

---

### Task 3: WASM glue + TS image API

**Files:** Modify `src/core/wasm.ts`, `src/core/wasm-browser.ts`, `src/core/document.ts`, `src/core/errors.ts`, `src/generate/draw-queue.ts`, `src/generate/page.ts`, `src/index.ts`, `src/index.browser.ts`, `src/generate/index.ts`; Create `src/generate/image.ts`; Test `tests/draw-image.test.ts`

- [ ] **Step 1:** `source "$HOME/.cargo/env" && bun run build:wasm`.

- [ ] **Step 2: WASM glue.** In both `wasm.ts` and `wasm-browser.ts`:
  - import `image_info` and update the `apply_draw_ops`/`create_document` imports (signatures changed).
  - change `applyDrawOps(data, opsJson, images)` and `createDocument(opsJson, images)` to forward an `images: Uint8Array` param.
  - add `imageInfo(data: Uint8Array): string` wrapper (browser one calls `ensureInitialized()`).

- [ ] **Step 3: errors.** Add to `src/core/errors.ts`:

```ts
/** Thrown when image bytes are not a supported JPEG or PNG. */
export class InvalidImageError extends PdfError {}
```

(if other errors set `this.name`, match. Map core decode errors to this in document.ts where embed/save call the core — see Step 7.)

- [ ] **Step 4: `src/generate/image.ts`:**

```ts
/** An embedded image. Obtain one with `doc.embedJpg(bytes)` or `doc.embedPng(bytes)`. */
export class PdfImage {
  /** @internal */
  constructor(
    /** @internal raw image bytes, embedded into the PDF at save time */
    readonly bytes: Uint8Array,
    /** Intrinsic image width in pixels. */
    readonly width: number,
    /** Intrinsic image height in pixels. */
    readonly height: number,
  ) {}

  /** Return `{ width, height }` scaled by `factor` (for passing to drawImage). */
  scale(factor: number): { width: number; height: number } {
    return { width: this.width * factor, height: this.height * factor };
  }
}
```

- [ ] **Step 5: DrawQueue image ops.** In `src/generate/draw-queue.ts`:
  - add image op wire type: `type ImageOp = { op: "image"; page: number; x: number; y: number; width: number; height: number; imageOffset: number; imageLength: number };`
  - store pending images as `{ page, bytes, x, y, width, height }` in a separate array (offsets are computed at payload time, not push time).
  - add `pushImage(page, bytes: Uint8Array, opts: { x; y; width; height }): void`.
  - replace `toJson()`/`toCreateJson()` with payload builders that also assemble the blob:
    ```ts
    /** Build { opsJson, images } for the load-mode draw path. */
    toDrawPayload(): { opsJson: string; images: Uint8Array } { ... }
    /** Build { opsJson, images } for create_document. */
    toCreatePayload(): { opsJson: string; images: Uint8Array } { ... }
    ```
    Each concatenates all image byte arrays into one `Uint8Array`, assigns each image op its `imageOffset`/`imageLength`, and interleaves text + image ops in the order they were queued **per the op sequence**. IMPORTANT: text and image ops must preserve global insertion order within a page so draw order (z-order) is correct. Implementation: keep a single ordered `ops` array holding a tagged union of text ops and image-pending entries; at payload time, walk it once, emit text ops as-is and image entries with computed offsets, and build the blob. `toCreatePayload` prepends the addPage ops.
  - keep `length` reflecting total queued draw ops (text + image) so `save()`'s `> 0` guard still works.

- [ ] **Step 6: `PdfPage.drawImage`.** In `src/generate/page.ts`:

```ts
import { PdfImage } from "./image.js";

/** Options for {@link PdfPage.drawImage}. `(x, y)` is the lower-left corner. */
export interface DrawImageOptions {
  x: number;
  y: number;
  /** Drawn width in points. Defaults to the image's intrinsic pixel width. */
  width?: number;
  /** Drawn height in points. Defaults to the image's intrinsic pixel height. */
  height?: number;
}

// method on PdfPage:
drawImage(image: PdfImage, options: DrawImageOptions): void {
  const width = options.width ?? image.width;
  const height = options.height ?? image.height;
  for (const [v, name] of [[options.x, "x"], [options.y, "y"], [width, "width"], [height, "height"]] as const) {
    if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
  }
  if (width <= 0 || height <= 0) throw new RangeError("width and height must be > 0");
  this.queue.pushImage(this.index, image.bytes, { x: options.x, y: options.y, width, height });
}
```

- [ ] **Step 7: document.ts embed + save.**
  - Add to `CoreWasm`: `imageInfo(data: Uint8Array): string;` and update `applyDrawOps`/`createDocument` to the 2-arg/`images` forms.
  - Add methods:
    ```ts
    /** Embed a JPEG image for later drawing. */
    async embedJpg(bytes: Uint8Array): Promise<PdfImage> { return this.embedImage(bytes); }
    /** Embed a PNG image for later drawing. */
    async embedPng(bytes: Uint8Array): Promise<PdfImage> { return this.embedImage(bytes); }

    private embedImage(bytes: Uint8Array): PdfImage {
      let info: { width: number; height: number };
      try {
        info = JSON.parse(this.wasm.imageInfo(bytes));
      } catch (e) {
        throw toInvalidImageError(e);
      }
      return new PdfImage(bytes, info.width, info.height);
    }
    ```
    (`embedJpg`/`embedPng` share one decoder — the core sniffs the format from magic bytes, so no per-format branching is needed; the two names exist for API familiarity / pdf-lib parity. Note this in a comment.)
  - In `save()`: load-mode draw branch uses `this.drawQueue.toDrawPayload()` → `this.wasm.applyDrawOps(bytes, opsJson, images)`. create-mode uses `toCreatePayload()` → `this.wasm.createDocument(opsJson, images)`. Wrap core errors so an image decode failure surfaces as `InvalidImageError` when the message indicates an image problem, else `PdfCoreError` (extend `toPdfError`, or add a small `toInvalidImageError` for the embed path and let save() use the existing `toPdfError`). Keep it simple: a decode failure at embed time is the primary guard; at save time the bytes already decoded once, so `toPdfError` is fine there.
  - Import `PdfImage` from `../generate/image.js`, `InvalidImageError` from `./errors.js`, add a `toInvalidImageError(e)` helper in errors.ts that wraps into `InvalidImageError` (message from the core).

- [ ] **Step 8: exports.** Add to BOTH entries' re-export blocks and to `src/generate/index.ts`:

```ts
export { PdfImage } from "./generate/image.js";       // (../generate/... path adjusted per file)
export type { DrawImageOptions } from "./generate/page.js";
```

and to entries (not generate, since it's an error type tied to core) add `export { InvalidImageError } from "./core/errors.js";`. Add `InvalidImageError` to the existing error re-export list.

- [ ] **Step 9: tests** `tests/draw-image.test.ts`. Use a tiny valid PNG (a 1x1 or small RGBA PNG as a `Uint8Array` literal — generate the bytes; or read an existing image fixture if one exists, else inline). Cases:
  - `embedPng returns intrinsic size`: embed → `image.width`/`height` match the known PNG dimensions.
  - `embed rejects non-image`: `doc.embedPng(new Uint8Array([1,2,3]))` rejects with `InvalidImageError`.
  - `drawImage on loaded page round-trips`: load FICHA, embed, `getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80 })`, save → output > original, reloads, content contains `Do`.
  - `drawImage on created page`: create, addPage, drawImage, save, reload → 1 page; content has `Do`.
  - `drawImage default size uses intrinsic`: omit width/height → no throw, op queued.
  - `drawImage validates`: width 0 / non-finite x → throw.
  - `scale helper`: `img.scale(0.5)` → half dims.

- [ ] **Step 10:** `bun run typecheck && bun test` (expect 67 + ~7), `bun run build:js`, import all 5 entries, `bun run scripts/browser-entry-smoke.ts`. Iterate to green.
- [ ] **Step 11: commit** `feat: embedJpg/embedPng and drawImage`

---

### Final verification

- [ ] `cargo test` 0 fail; typecheck clean; `bun test` 0 fail.
- [ ] All 5 export entries resolve; `PdfImage`, `InvalidImageError`, `DrawImageOptions` exported from root; browser smoke passes.
- [ ] A created doc with an image and a loaded doc with a stamped image both reload via `PdfDocument.load`.
