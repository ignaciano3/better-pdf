# Milestone M33: SVG / Vector Paths Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Draw arbitrary vector paths — `page.drawSvgPath(d, opts)` and `page.drawPolygon(points, opts)` — for icons, charts, and logos, on loaded and created PDFs.

**Architecture:** A `path` op carries a list of primitive segments (move/line/cubic/close) plus fill/stroke/strokeWidth/opacity. Rust emits PDF path operators (`m`/`l`/`c`/`h`) and a paint op (reusing the existing `paint_op` + ExtGState opacity machinery). The TS layer parses the SVG path `d` string into those primitives (converting H/V/S/Q/T and relative commands to absolute move/line/cubic; arcs `A`/`a` are rejected with a clear error). `drawPolygon` builds a closed move+lines path directly. Coordinates are interpreted in PDF user space (origin bottom-left, y-up) — the same convention as every other draw op; SVG `d` values are used as-is (no auto-flip).

**Tech Stack:** Rust 2024, lopdf 0.41; TS ESM (hand-rolled path-`d` tokenizer, no dep); Bun + cargo.

## Global Constraints

- Only 4 primitive segment kinds cross the wire: `m` (move), `l` (line), `c` (cubic bezier), `z` (close). All SVG complexity (relative coords, H/V, S/T smooth, Q quadratic→cubic) is resolved in TS before sending.
- Arc commands (`A`/`a`) are NOT supported — `drawSvgPath` throws a clear error naming the command; document the limitation.
- Coordinates in PDF user space (y-up), same as all draw ops. SVG `d` coords used verbatim.
- fill and/or stroke (at least one should be set; if neither, the path is a no-op paint `n` — allowed but pointless). strokeWidth default 1. opacity 0..1 via ExtGState (reuse `extgstate_dict`/`register_extgstate`).
- Both engines. Validate before mutation: page in range; all coords finite; opacity 0..1; strokeWidth >= 0; colors finite.
- Every task green: cargo + bun + typecheck. No root Cargo.toml. Rebuild wasm before bun. pkg-web gitignored. Tests in `tests/`. Branch `m33-svg-paths`; not on master.

## File Structure

- Modify: `crates/core/src/draw.rs` — `DrawOp::Path` + `Seg` enum + `pub(crate) emit_path`; validate; apply.
- Modify: `crates/core/src/create.rs` — `CreateOp::Path`; validate; apply (reuse `emit_path`).
- Create: `src/generate/svg-path.ts` — `parseSvgPath(d: string): Segment[]` (the tokenizer/converter).
- Modify: `src/generate/draw-queue.ts` — `PathOp` + `pushPath`; `src/generate/page.ts` — `drawSvgPath`, `drawPolygon`.
- Tests: draw.rs/create.rs `#[cfg(test)]`, `tests/svg-path.test.ts` (parser unit tests + round-trip).

## Interfaces (cross-task contract)

- Wire segment objects: `{"t":"m","x":..,"y":..}`, `{"t":"l","x":..,"y":..}`, `{"t":"c","x1":..,"y1":..,"x2":..,"y2":..,"x":..,"y":..}`, `{"t":"z"}`.
- Wire op: `{"op":"path","page":i,"segments":[...],"fill":[r,g,b]?,"stroke":[r,g,b]?,"strokeWidth":w?,"opacity":o?}`.
- `DrawOp::Path`/`CreateOp::Path` fields: `page: usize, segments: Vec<Seg>, fill: Option<[f32;3]>, stroke: Option<[f32;3]>, #[serde(rename="strokeWidth")] stroke_width: Option<f32>, opacity: Option<f32>`.
- `Seg` enum (serde `#[serde(tag="t", rename_all="lowercase")]`): `M{x,y}`, `L{x,y}`, `C{x1,y1,x2,y2,x,y}`, `Z`.
- TS: `parseSvgPath(d) -> Segment[]` where `Segment` mirrors the wire seg objects. `page.drawSvgPath(d: string, opts: {fill?: Color; stroke?: Color; strokeWidth?: number; opacity?: number}): void`. `page.drawPolygon(points: {x:number;y:number}[], opts: {...same..., closed?: boolean}): void`. DrawQueue `pushPath(op)`.

---

### Task 1: Rust — path op + emit_path (both engines)

**Files:** `crates/core/src/draw.rs`, `crates/core/src/create.rs`.

- [ ] **Step 1: Write failing tests**

```rust
// draw.rs
#[test]
fn draws_path_with_fill_and_stroke() {
    let json = r#"[{"op":"path","page":0,"segments":[
        {"t":"m","x":50,"y":50},{"t":"l","x":150,"y":50},
        {"t":"c","x1":160,"y1":60,"x2":160,"y2":140,"x":150,"y":150},
        {"t":"z"}],"fill":[1,0,0],"stroke":[0,0,0],"strokeWidth":2}]"#;
    let out = apply_draw_ops_json(FICHA, json, &[], &[], "[]").unwrap();
    let s = last_draw_stream_content(&out);
    assert!(s.contains("50 50 m"), "content: {s}");
    assert!(s.contains(" l"), "content: {s}");
    assert!(s.contains(" c"), "content: {s}");
    assert!(s.contains("\nh\n") || s.contains(" h"), "close: {s}");
    assert!(s.contains('B'), "fill+stroke should paint with B: {s}"); // B = fill+stroke
    assert!(s.contains("1 0 0 rg"), "fill color: {s}");
    assert!(s.contains("2 w"), "stroke width: {s}");
}

#[test]
fn path_fill_only_uses_f() {
    let json = r#"[{"op":"path","page":0,"segments":[{"t":"m","x":0,"y":0},{"t":"l","x":10,"y":0},{"t":"l","x":10,"y":10},{"t":"z"}],"fill":[0,0,1]}]"#;
    let out = apply_draw_ops_json(FICHA, json, &[], &[], "[]").unwrap();
    let s = last_draw_stream_content(&out);
    assert!(s.split_whitespace().any(|w| w == "f"), "fill-only path should paint with f: {s}");
}

#[test]
fn path_opacity_registers_extgstate() {
    let json = r#"[{"op":"path","page":0,"segments":[{"t":"m","x":0,"y":0},{"t":"l","x":10,"y":10}],"stroke":[0,0,0],"opacity":0.5}]"#;
    let out = apply_draw_ops_json(FICHA, json, &[], &[], "[]").unwrap();
    let s = last_draw_stream_content(&out);
    assert!(s.contains("/BPG"), "opacity should reference an ExtGState: {s}");
}

#[test]
fn path_rejects_non_finite_coord() {
    let json = r#"[{"op":"path","page":0,"segments":[{"t":"m","x":0,"y":0},{"t":"l","x":1e999,"y":0}],"stroke":[0,0,0]}]"#;
    // 1e999 parses to inf in JSON; if serde rejects first that's also fine — assert err
    assert!(apply_draw_ops_json(FICHA, json, &[], &[], "[]").is_err());
}
```
Mirror one create.rs test: a created page with a path op → content has `m`/`l`/paint.

- [ ] **Step 2: Run — expect FAIL**

- [ ] **Step 3: Implement**

- Add `Seg` enum and `DrawOp::Path` (draw.rs, `#[serde(rename="path")]` on the variant) / `CreateOp::Path` (create.rs, camelCase auto).
- `pub(crate) fn emit_path(out: &mut Vec<u8>, gs_key: Option<&str>, segments: &[Seg], fill: Option<[f32;3]>, stroke: Option<[f32;3]>, stroke_width: Option<f32>)` in draw.rs (reused by create.rs):
  ```
  q
  [/{gs_key} gs]?
  [r g b rg]?  (fill)
  [r g b RG]?  (stroke)
  [{stroke_width} w]?  (when stroke)
  for seg: M -> "{x} {y} m", L -> "{x} {y} l", C -> "{x1} {y1} {x2} {y2} {x} {y} c", Z -> "h"
  {paint_op(fill.is_some(), stroke.is_some())}
  Q
  ```
  Use `fmt_num`. paint_op already exists (B/f/S/n).
- Validation pass (both engines): `page < page_count`; every seg coord `is_finite()`; if `opacity` Some → 0..1; if `stroke_width` Some → finite & >= 0; fill/stroke colors finite. (Empty segments → allow or error; require at least 1 segment — error "path must have at least one segment".)
- Per-page processing: like rectangle/ellipse ops — if `opacity` is Some, register an ExtGState (`BPG{gs_counter}`, reuse `extgstate_dict`/`register_extgstate`/`extgstates_on_page`) and pass its key to `emit_path`; append to `stream_content` in op order. Add `Path` to the page_ops grouping match.

- [ ] **Step 4: Run — expect PASS, full suite**

- [ ] **Step 5: Commit**

```bash
git checkout -b m33-svg-paths
git add crates/core/src/draw.rs crates/core/src/create.rs
git commit -m "feat(paths): vector path op (move/line/cubic/close) with fill/stroke/opacity

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: TS — SVG path parser + drawSvgPath + drawPolygon

**Files:** `src/generate/svg-path.ts` (new), `src/generate/draw-queue.ts`, `src/generate/page.ts`.

- [ ] **Step 1: Rebuild wasm.**

- [ ] **Step 2: Failing parser unit tests + round-trip** (`tests/svg-path.test.ts`)

```ts
import { expect, test } from "bun:test";
import { parseSvgPath } from "../src/generate/svg-path.js";
import { PdfDocument, rgb } from "../src/index.js";

test("parses absolute M L Z", () => {
  expect(parseSvgPath("M10 20 L30 40 Z")).toEqual([
    {t:"m",x:10,y:20},{t:"l",x:30,y:40},{t:"z"},
  ]);
});
test("converts relative l to absolute", () => {
  expect(parseSvgPath("M10 10 l5 0")).toEqual([{t:"m",x:10,y:10},{t:"l",x:15,y:10}]);
});
test("converts H and V to line", () => {
  expect(parseSvgPath("M0 0 H10 V10")).toEqual([{t:"m",x:0,y:0},{t:"l",x:10,y:0},{t:"l",x:10,y:10}]);
});
test("converts quadratic Q to cubic c", () => {
  const segs = parseSvgPath("M0 0 Q5 10 10 0");
  expect(segs[0]).toEqual({t:"m",x:0,y:0});
  expect(segs[1].t).toBe("c"); // quadratic promoted to cubic
});
test("rejects arc commands", () => {
  expect(() => parseSvgPath("M0 0 A5 5 0 0 1 10 10")).toThrow();
});
test("drawSvgPath round-trips into a valid PDF", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawSvgPath("M50 50 L150 50 L100 150 Z", { fill: rgb(1,0,0) });
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(1);
});
test("drawPolygon closed", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawPolygon([{x:10,y:10},{x:50,y:10},{x:30,y:40}], { stroke: rgb(0,0,0), closed: true });
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(1);
});
```

- [ ] **Step 3: Implement**

- `src/generate/svg-path.ts`: `export type Segment = {t:"m";x:number;y:number} | {t:"l";x:number;y:number} | {t:"c";x1:number;y1:number;x2:number;y2:number;x:number;y:number} | {t:"z"}`. `export function parseSvgPath(d: string): Segment[]`. Tokenize commands + numbers (regex for command letters and signed/decimal/exponent numbers). Track current point + subpath start (for Z). Handle M/m, L/l, H/h, V/v, C/c, S/s, Q/q, T/t, Z/z (and implicit repeated coords after M→L, after a command letter). Conversions: relative→absolute (lowercase add current point); H/V→L (fill the missing coord from current point); Q/q (quadratic) → cubic via `c1 = p0 + 2/3*(qc-p0)`, `c2 = p1 + 2/3*(qc-p1)`; S/s (smooth cubic) → reflect previous cubic's second control point about current point; T/t (smooth quadratic) → reflect previous quadratic control point. On `A`/`a` throw `Error("SVG arc commands (A/a) are not supported")`. Throw on malformed input. Return the primitive Segment[].
- `draw-queue.ts`: `PathOp = {op:"path"; page:number; segments:Segment[]; fill?:[number,number,number]; stroke?:[number,number,number]; strokeWidth?:number; opacity?:number}`; `pushPath(op)` (plain op → both payloads).
- `page.ts`:
  - `drawSvgPath(d: string, opts: {fill?: Color; stroke?: Color; strokeWidth?: number; opacity?: number} = {}): void` — `const segments = parseSvgPath(d);` then push a path op with `fill`/`stroke` mapped to `[r,g,b]` tuples (use the same Color→tuple mapping `pushText`/shapes use), `strokeWidth`, `opacity`. Validate opacity 0..1 + strokeWidth>=0 (RangeError).
  - `drawPolygon(points: {x:number;y:number}[], opts: {fill?; stroke?; strokeWidth?; opacity?; closed?: boolean} = {}): void` — require >= 2 points (RangeError); build segments `[{t:"m",x,y}, {t:"l",...}, ..., (closed? {t:"z"})]`; push the path op.

- [ ] **Step 4: Run focused + full + typecheck + cargo. Green.**

- [ ] **Step 5: Commit** (`feat(paths): drawSvgPath + drawPolygon TS API with SVG path parser`)

---

### Task 3: Docs + version 0.11.0

**Files:** generating.md, limitations.md, from-pdf-lib.md, SKILL.md, README.md, CHANGELOG.md, package.json, Cargo.toml.

- [ ] **Step 1: Docs** — "Vector paths" section (drawSvgPath icon example + drawPolygon chart example). limitations.md: vector paths now SUPPORTED; note SVG arcs (A/a) unsupported and coordinates are PDF user space (y-up). from-pdf-lib.md: parity with pdf-lib `drawSvgPath`. SKILL.md + README.md.
- [ ] **Step 2: Version** 0.11.0 (package.json + Cargo.toml + the propagated Cargo.lock line). CHANGELOG 0.11.0: "Vector paths: `page.drawSvgPath()` (SVG path data) and `page.drawPolygon()` with fill/stroke/opacity, on loaded and created PDFs. (SVG arcs not yet supported.)"
- [ ] **Step 3: TypeDoc regen if clean.**
- [ ] **Step 4: Final verify (cargo + bun + typecheck) + commit** (`docs(paths): document vector paths; release 0.11.0`).

---

## Self-Review

**Spec coverage:** path op + emit_path both engines (T1), SVG parser + drawSvgPath + drawPolygon (T2), docs/version (T3). fill/stroke/opacity; paint_op B/f/S; ExtGState opacity; arcs rejected.

**Risk callouts:** (1) SVG parser correctness (relative, H/V, Q→cubic, S/T reflection) — unit-tested per conversion; (2) Q→cubic formula `c1=p0+2/3(qc-p0)`, `c2=p1+2/3(qc-p1)`; (3) only m/l/c/z reach Rust (parser does all conversion); (4) opacity ExtGState reuses the shape machinery (BPG keys).

**Type consistency:** `Seg`/`Segment` objects identical Rust↔TS (`t` discriminator: m/l/c/z). `path` op fields `segments/fill/stroke/strokeWidth/opacity` identical across DrawOp/CreateOp/TS. `emit_path` shared by both engines.
