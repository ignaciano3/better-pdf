# Milestone 23 — Vector Graphics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Draw vector primitives — `drawLine`, `drawRectangle`, `drawEllipse` — with fill color, border color/width, and opacity, on both loaded and created pages.

**Architecture:** New op variants (`line`, `rectangle`, `ellipse`) on the existing `DrawOp`/`CreateOp` enums. They flow through the same draw queue and the same `apply_draw_ops`/`create_document` WASM exports — **no new export, no WASM signature change, no glue change**. Each shape emits PDF path operators wrapped in `q … Q`; opacity is realized with an `/ExtGState` resource (`/ca` + `/CA`) referenced as `/GSn gs`. The colors reuse the M20 `rgb`/`grayscale` helpers.

**Tech Stack:** Rust (lopdf 0.41), TypeScript ESM, bun test.

**Spec:** `docs/superpowers/specs/2026-06-12-pdf-generation-design.md` (M23 row).

**Environment:** `source "$HOME/.cargo/env"`; `bun run build:wasm` after Rust changes. Baselines after M22 merge: cargo 65 pass, bun 76 pass / 4 skip / 0 fail, typecheck clean.

**Reuse references:**
- `crates/core/src/draw.rs` — `fmt_num`, `emit_image_op` (q/Q + operator pattern), `register_xobject`/`set_xobject` (mirror for ExtGState), per-page op dispatch, q/Q content wrap.
- `crates/core/src/create.rs` — per-page single ordered pass accumulating content + resource sub-dicts (Font, XObject); add ExtGState the same way.
- `src/generate/draw-queue.ts` — ordered `drawOps` list; add shape ops as plain (no-bytes) entries alongside text.
- `src/generate/color.ts` — `Color`, `rgb`, `grayscale`.

**Op wire contracts** (TS produces, Rust consumes; all coordinates PDF points, origin bottom-left; colors RGB 0..1):

```jsonc
{ "op": "line", "page": 0, "x1": 50, "y1": 100, "x2": 250, "y2": 100, "thickness": 2, "color": [1,0,0], "opacity": 0.5 }
{ "op": "rectangle", "page": 0, "x": 50, "y": 100, "width": 200, "height": 80,
  "color": [0.9,0.9,0.9], "borderColor": [0,0,0], "borderWidth": 1, "opacity": 1 }
{ "op": "ellipse", "page": 0, "x": 150, "y": 140, "xScale": 100, "yScale": 40,
  "color": [0,0,1], "borderColor": [0,0,0], "borderWidth": 1, "opacity": 1 }
```

- `line`: always stroked; `color` is the stroke color (default black if omitted), `thickness` default 1.
- `rectangle`/`ellipse`: `color` = fill (optional), `borderColor` + `borderWidth` = stroke (optional). `(x,y)` is the rectangle's lower-left; for ellipse `(x,y)` is the **center**, `xScale`/`yScale` are the radii.
- `opacity` (optional, 0..1) applies to both fill and stroke via ExtGState. Omitted ⇒ fully opaque, no ExtGState emitted.

---

### Task 1: Rust — shape ops in draw.rs (existing pages)

**Files:** Modify `crates/core/src/draw.rs`

- [ ] **Step 1: add op variants.** Add to `DrawOp` (use `Option` for optional fields; serde camelCase renames where needed):

```rust
    Line {
        page: usize,
        x1: f32, y1: f32, x2: f32, y2: f32,
        thickness: Option<f32>,
        color: Option<[f32; 3]>,
        opacity: Option<f32>,
    },
    Rectangle {
        page: usize,
        x: f32, y: f32, width: f32, height: f32,
        color: Option<[f32; 3]>,
        #[serde(rename = "borderColor")]
        border_color: Option<[f32; 3]>,
        #[serde(rename = "borderWidth")]
        border_width: Option<f32>,
        opacity: Option<f32>,
    },
    Ellipse {
        page: usize,
        x: f32, y: f32,
        #[serde(rename = "xScale")]
        x_scale: f32,
        #[serde(rename = "yScale")]
        y_scale: f32,
        color: Option<[f32; 3]>,
        #[serde(rename = "borderColor")]
        border_color: Option<[f32; 3]>,
        #[serde(rename = "borderWidth")]
        border_width: Option<f32>,
        opacity: Option<f32>,
    },
```

- [ ] **Step 2: emit helpers.** Add `pub(crate)` free functions. The `gs_key: Option<&str>` is the ExtGState resource name (without slash) when opacity is set.

```rust
fn paint_op(has_fill: bool, has_stroke: bool) -> &'static str {
    match (has_fill, has_stroke) {
        (true, true) => "B",   // fill + stroke
        (true, false) => "f",  // fill only
        (false, true) => "S",  // stroke only
        (false, false) => "n", // no-op paint (path discarded)
    }
}

pub(crate) fn emit_line(out: &mut Vec<u8>, gs_key: Option<&str>, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 3]) {
    let [r, g, b] = color;
    out.extend_from_slice(b"q\n");
    if let Some(k) = gs_key { out.extend_from_slice(format!("/{k} gs\n").as_bytes()); }
    out.extend_from_slice(format!("{} w\n", fmt_num(thickness)).as_bytes());
    out.extend_from_slice(format!("{} {} {} RG\n", fmt_num(r), fmt_num(g), fmt_num(b)).as_bytes());
    out.extend_from_slice(format!("{} {} m\n", fmt_num(x1), fmt_num(y1)).as_bytes());
    out.extend_from_slice(format!("{} {} l\n", fmt_num(x2), fmt_num(y2)).as_bytes());
    out.extend_from_slice(b"S\nQ\n");
}

pub(crate) fn emit_rectangle(out: &mut Vec<u8>, gs_key: Option<&str>, x: f32, y: f32, w: f32, h: f32, fill: Option<[f32; 3]>, border: Option<[f32; 3]>, border_width: Option<f32>) {
    out.extend_from_slice(b"q\n");
    if let Some(k) = gs_key { out.extend_from_slice(format!("/{k} gs\n").as_bytes()); }
    if let Some([r, g, b]) = fill {
        out.extend_from_slice(format!("{} {} {} rg\n", fmt_num(r), fmt_num(g), fmt_num(b)).as_bytes());
    }
    if let Some([r, g, b]) = border {
        out.extend_from_slice(format!("{} {} {} RG\n", fmt_num(r), fmt_num(g), fmt_num(b)).as_bytes());
        out.extend_from_slice(format!("{} w\n", fmt_num(border_width.unwrap_or(1.0))).as_bytes());
    }
    out.extend_from_slice(format!("{} {} {} {} re\n", fmt_num(x), fmt_num(y), fmt_num(w), fmt_num(h)).as_bytes());
    out.extend_from_slice(paint_op(fill.is_some(), border.is_some()).as_bytes());
    out.extend_from_slice(b"\nQ\n");
}

pub(crate) fn emit_ellipse(out: &mut Vec<u8>, gs_key: Option<&str>, cx: f32, cy: f32, rx: f32, ry: f32, fill: Option<[f32; 3]>, border: Option<[f32; 3]>, border_width: Option<f32>) {
    // 4-segment cubic Bézier approximation of an ellipse. k = 4/3*(sqrt(2)-1).
    let k = 0.552_284_8_f32;
    let (ox, oy) = (rx * k, ry * k);
    out.extend_from_slice(b"q\n");
    if let Some(key) = gs_key { out.extend_from_slice(format!("/{key} gs\n").as_bytes()); }
    if let Some([r, g, b]) = fill {
        out.extend_from_slice(format!("{} {} {} rg\n", fmt_num(r), fmt_num(g), fmt_num(b)).as_bytes());
    }
    if let Some([r, g, b]) = border {
        out.extend_from_slice(format!("{} {} {} RG\n", fmt_num(r), fmt_num(g), fmt_num(b)).as_bytes());
        out.extend_from_slice(format!("{} w\n", fmt_num(border_width.unwrap_or(1.0))).as_bytes());
    }
    // Start at right vertex, go counter-clockwise.
    out.extend_from_slice(format!("{} {} m\n", fmt_num(cx + rx), fmt_num(cy)).as_bytes());
    out.extend_from_slice(format!("{} {} {} {} {} {} c\n", fmt_num(cx + rx), fmt_num(cy + oy), fmt_num(cx + ox), fmt_num(cy + ry), fmt_num(cx), fmt_num(cy + ry)).as_bytes());
    out.extend_from_slice(format!("{} {} {} {} {} {} c\n", fmt_num(cx - ox), fmt_num(cy + ry), fmt_num(cx - rx), fmt_num(cy + oy), fmt_num(cx - rx), fmt_num(cy)).as_bytes());
    out.extend_from_slice(format!("{} {} {} {} {} {} c\n", fmt_num(cx - rx), fmt_num(cy - oy), fmt_num(cx - ox), fmt_num(cy - ry), fmt_num(cx), fmt_num(cy - ry)).as_bytes());
    out.extend_from_slice(format!("{} {} {} {} {} {} c\n", fmt_num(cx + ox), fmt_num(cy - ry), fmt_num(cx + rx), fmt_num(cy - oy), fmt_num(cx + rx), fmt_num(cy)).as_bytes());
    out.extend_from_slice(paint_op(fill.is_some(), border.is_some()).as_bytes());
    out.extend_from_slice(b"\nQ\n");
}
```

- [ ] **Step 3: ExtGState registration.** Add a helper mirroring `register_xobject` but for `/ExtGState`, plus a builder for the dict:

```rust
pub(crate) fn extgstate_dict(opacity: f32) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"ExtGState".to_vec()));
    d.set("ca", Object::Real(opacity)); // fill alpha
    d.set("CA", Object::Real(opacity)); // stroke alpha
    d
}

pub(crate) fn register_extgstate(inc: &mut IncrementalDocument, page_id: ObjectId, key: &str, gs_id: ObjectId) -> Result<(), String> {
    // same shape as register_xobject but the sub-dict key is "ExtGState"
}

fn set_extgstate(res: &mut Dictionary, key: &str, gs_id: ObjectId) {
    if !res.has(b"ExtGState") { res.set("ExtGState", Object::Dictionary(Dictionary::new())); }
    if let Ok(gs) = res.get_mut(b"ExtGState").and_then(Object::as_dict_mut) {
        gs.set(key.as_bytes().to_vec(), Object::Reference(gs_id));
    }
}
```

- [ ] **Step 4: validation + dispatch.** In `apply_draw_ops_json`:
  - Validation pass: for each shape op validate `page` in range. Validate `opacity` (if present) is finite and `0.0..=1.0` else Err "opacity must be in 0..1". Validate `border_width`/`thickness` (if present) finite and `>= 0`. (Color range is enforced by the TS `rgb`/`grayscale`; no need to re-check here, but reject non-finite color components defensively with Err "invalid color".)
  - Per-page emission: extend the op-dispatch to handle Line/Rectangle/Ellipse. When a shape has `opacity`, allocate/dedup an ExtGState per (page, opacity) — keep a `Vec<(f32 bits, ObjectId, String key)>` per page or a global counter `GS{n}`; create the object via `inc.new_document.add_object(Object::Dictionary(extgstate_dict(op)))`, emit with `gs_key=Some("GS{n}")`, and register on the page after the stream is built (alongside font/xobject registration). For ops with no opacity pass `None`.
  - line default thickness = `thickness.unwrap_or(1.0)`, color = `color.unwrap_or([0.0,0.0,0.0])`.

- [ ] **Step 5: tests** (add to draw.rs tests; reuse the `ops`/`last_draw_stream_content` helpers, passing `&[]` images):
  - `draws_line`: line op → content contains `m` and `l` and `S`; stroke color `1 0 0 RG` present.
  - `draws_rectangle_fill_and_border`: rectangle with fill+border → content has `re` and `B`.
  - `rectangle_fill_only_uses_f`: fill, no border → content has `re` and a standalone `f` paint.
  - `draws_ellipse`: ellipse → content has 4 `c` operators and a paint op.
  - `opacity_registers_extgstate`: rectangle with opacity 0.5 → page `/Resources/ExtGState/GS0` exists; its dict has `/ca 0.5`; content has `/GS0 gs`.
  - `opacity_out_of_range_errors`: opacity 1.5 → Err "opacity".
  - shapes compose with text/image in insertion order (one mixed test).

- [ ] **Step 6:** `cargo test` all pass (65 + ~7). Iterate. **Step 7: commit** `feat(core): draw lines, rectangles, ellipses on existing pages`

---

### Task 2: Rust — shape ops in create.rs (created pages)

**Files:** Modify `crates/core/src/create.rs`

- [ ] **Step 1:** Add the same three variants (Line/Rectangle/Ellipse) to `CreateOp` (identical fields/renames as Task 1).
- [ ] **Step 2:** Validation: extend the create validation pass with the same shape checks (page range, opacity 0..1, border/thickness >= 0).
- [ ] **Step 3:** In the per-page ordered pass, dispatch shape ops to `crate::draw::{emit_line, emit_rectangle, emit_ellipse}`. For opacity, accumulate an `ExtGState` sub-dict on the page resources (mirror how Font/XObject sub-dicts are built): per distinct opacity create `extgstate_dict(op)` via `doc.add_object`, key `GS{n}`, set into the page's ExtGState resource dict, and pass `gs_key=Some("GS{n}")` to the emit fn. Include `/ExtGState` in the page Resources only when non-empty.
- [ ] **Step 4: tests:**
  - `creates_doc_with_rectangle`: addPage + rectangle (fill+border) → reload; content has `re` and `B`.
  - `creates_doc_with_opacity`: addPage + rectangle opacity 0.5 → page `/Resources/ExtGState/GS0` with `/ca 0.5`; content has `/GS0 gs`.
- [ ] **Step 5:** `cargo test` all pass. **Step 6: commit** `feat(core): draw vector shapes on created pages`

---

### Task 3: TS — drawLine/drawRectangle/drawEllipse

**Files:** Modify `src/generate/draw-queue.ts`, `src/generate/page.ts`, `src/index.ts`, `src/index.browser.ts`, `src/generate/index.ts`; Test `tests/draw-shapes.test.ts`

No WASM rebuild needed (no Rust export change). But you DO need Rust Tasks 1-2 merged into the same branch first (they are, sequentially). Run `bun run build:wasm` once if pkg-web predates the new op variants — actually the new variants are deserialized by the same `apply_draw_ops`/`create_document`; the WASM binary must include Task 1-2 code, so **rebuild wasm after Rust tasks**: `source "$HOME/.cargo/env" && bun run build:wasm`.

- [ ] **Step 1: build:wasm** (picks up Task 1-2 op variants).

- [ ] **Step 2: draw-queue.ts.** Add wire op types and push methods. Add to the ordered `drawOps` union these plain ops (no bytes):

```ts
export type LineOp = { op: "line"; page: number; x1: number; y1: number; x2: number; y2: number; thickness?: number; color?: [number, number, number]; opacity?: number };
export type RectangleOp = { op: "rectangle"; page: number; x: number; y: number; width: number; height: number; color?: [number, number, number]; borderColor?: [number, number, number]; borderWidth?: number; opacity?: number };
export type EllipseOp = { op: "ellipse"; page: number; x: number; y: number; xScale: number; yScale: number; color?: [number, number, number]; borderColor?: [number, number, number]; borderWidth?: number; opacity?: number };
```

Extend the union type the ordered list holds to include these (they serialize directly, no offset handling). Add `pushLine`, `pushRectangle`, `pushEllipse` methods that push the corresponding op object (omitting undefined optional fields so the JSON stays clean — or include them; Rust treats missing as None, but `color: undefined` serializes to absent in JSON.stringify, so spreading conditionally is cleanest). `buildDrawOps` passes these through unchanged (they have no `kind:"image"` tag). `length` already counts all entries.

- [ ] **Step 3: page.ts.** Add option interfaces + methods. Color params are `Color` (from color.js); convert to `[r,g,b]` tuple when pushing.

```ts
import { type Color } from "./color.js";

/** Options for {@link PdfPage.drawLine}. */
export interface DrawLineOptions {
  start: { x: number; y: number };
  end: { x: number; y: number };
  /** Stroke width in points. Default 1. */
  thickness?: number;
  /** Stroke color. Default black. */
  color?: Color;
  /** Opacity 0..1. Default 1 (opaque). */
  opacity?: number;
}

/** Options for {@link PdfPage.drawRectangle}. `(x, y)` is the lower-left corner. */
export interface DrawRectangleOptions {
  x: number; y: number; width: number; height: number;
  /** Fill color. Omit for no fill. */
  color?: Color;
  /** Border color. Omit for no border. */
  borderColor?: Color;
  /** Border width in points. Default 1 when borderColor is set. */
  borderWidth?: number;
  /** Opacity 0..1. Default 1. */
  opacity?: number;
}

/** Options for {@link PdfPage.drawEllipse}. `(x, y)` is the center. */
export interface DrawEllipseOptions {
  x: number; y: number;
  /** Horizontal radius in points. */
  xScale: number;
  /** Vertical radius in points. */
  yScale: number;
  color?: Color;
  borderColor?: Color;
  borderWidth?: number;
  opacity?: number;
}
```

Methods validate: all coordinate/size numbers finite; `thickness`/`borderWidth` (if set) `>= 0`; `xScale`/`yScale` `> 0`; `opacity` (if set) finite and in `0..1`; rectangle width/height `> 0`. Throw `RangeError` on violation. Then push via the queue, converting `Color` → `[red, green, blue]` and only including provided optionals.

Example `drawEllipse`:
```ts
drawEllipse(options: DrawEllipseOptions): void {
  const { x, y, xScale, yScale } = options;
  for (const [v, n] of [[x,"x"],[y,"y"],[xScale,"xScale"],[yScale,"yScale"]] as const) {
    if (!Number.isFinite(v)) throw new RangeError(`${n} must be finite`);
  }
  if (xScale <= 0 || yScale <= 0) throw new RangeError("xScale and yScale must be > 0");
  validateOpacity(options.opacity);   // small shared helper in page.ts
  this.queue.pushEllipse(this.index, options);
}
```
Add a tiny `validateOpacity(o?: number)` and `validateBorderWidth(w?: number)` helper used by all three. Keep the existing drawText/drawImage untouched.

- [ ] **Step 4: exports.** Add to BOTH root entries and `src/generate/index.ts`:
```ts
export type { DrawLineOptions, DrawRectangleOptions, DrawEllipseOptions } from "./generate/page.js"; // adjust path per file
```

- [ ] **Step 5: tests** `tests/draw-shapes.test.ts` (import PdfDocument, rgb, PageSizes):
  - line on created page: create→addPage→drawLine({start,end,thickness:2,color:rgb(1,0,0)})→save→reload 1 page; latin1 contains " l\n" and " S".
  - rectangle fill+border on loaded page: load FICHA, drawRectangle with color+borderColor→save>original, reload, contains " re".
  - ellipse: create→addPage→drawEllipse({x:150,y:140,xScale:100,yScale:40,color:rgb(0,0,1)})→save→contains " c" (bezier) and reload ok.
  - opacity round-trips: rectangle with opacity:0.5 on created page → save → contains " gs" and reload ok.
  - validation: drawRectangle width 0 throws; drawEllipse xScale 0 throws; opacity 2 throws; thickness -1 throws.
  - shapes compose with text: drawText + drawRectangle on same created page both appear.

- [ ] **Step 6:** `bun run typecheck && bun test` (76 + ~10). `bun run build:js`, import all 5 entries + assert option types compile, `bun run scripts/browser-entry-smoke.ts`. Iterate.
- [ ] **Step 7: commit** `feat: drawLine, drawRectangle, drawEllipse`

---

### Final verification

- [ ] `cargo test` 0 fail; typecheck clean; `bun test` 0 fail.
- [ ] All 5 entries resolve; shape option types exported from root + ./generate; browser smoke passes.
- [ ] A created doc and a loaded doc each with a filled+bordered+semi-transparent rectangle reload via `PdfDocument.load`.
