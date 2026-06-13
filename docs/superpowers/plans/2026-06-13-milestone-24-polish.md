# Milestone 24 — Polish (measure_text, docs, bench, release) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Finish the generation feature set: text measurement (`measure_text` + `PdfFont.widthOfTextAtSize` + `doc.getFont`), then ship docs (README + migration guide), generation benchmarks, changelog, and a 0.2.0 version bump.

**Architecture:** `measure_text` is a thin stateless WASM export reusing `appearance::{standard_14_widths, string_width, encode_winansi}`. `PdfFont` is a small TS class returned by `doc.getFont(StandardFonts)` whose `widthOfTextAtSize` calls the export; `drawText`'s `font` option is widened to accept `StandardFonts | PdfFont` (backward compatible). The rest is documentation and release hygiene.

**Tech Stack:** Rust (lopdf 0.41), wasm-bindgen, TypeScript ESM, bun test, pdf-lib (bench only).

**Spec:** `docs/superpowers/specs/2026-06-12-pdf-generation-design.md` (M24 row + §4 `getFont`/`widthOfTextAtSize`).

**Environment:** `source "$HOME/.cargo/env"`; `bun run build:wasm` after Rust. Baselines after M23 merge: cargo 79 pass, bun 85 pass / 4 skip / 0 fail, typecheck clean.

**Reuse references:**
- `crates/core/src/appearance.rs:33` `standard_14_widths(&str) -> Option<FontWidths>`; `:67` `string_width(&[u8], size, &FontWidths) -> f32`; `:74` `encode_winansi(&str) -> Vec<u8>`.
- `crates/core/src/lib.rs` — thin wasm-bindgen export pattern + `fuzz_api`.
- `src/generate/fonts.ts` — `StandardFonts` enum (values are the base-font names `measure_text` expects).
- `src/generate/page.ts` — `DrawTextOptions.font` currently `StandardFonts`.

---

### Task 1: Rust — measure_text export

**Files:** Modify `crates/core/src/appearance.rs` (or `lib.rs` only), `crates/core/src/lib.rs`

- [ ] **Step 1:** Add a measurement function. Put it in `appearance.rs` (it composes existing helpers there):

```rust
/// Width in points of `text` rendered in standard-14 `font` at `size`.
/// Errors if `font` is not a standard-14 base name.
pub fn measure_text_width(font: &str, size: f32, text: &str) -> Result<f32, String> {
    let widths = standard_14_widths(font).ok_or_else(|| format!("unknown font: {font}"))?;
    Ok(string_width(&encode_winansi(text), size, &widths))
}
```

- [ ] **Step 2:** Add the wasm-bindgen export in `lib.rs` (doc comment matching neighbors):

```rust
/// Width in points of `text` in standard-14 `font` at `size`.
#[wasm_bindgen]
pub fn measure_text(font: &str, size: f32, text: &str) -> Result<f32, JsError> {
    appearance::measure_text_width(font, size, text).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 3: tests** in appearance.rs `tests`:

```rust
#[test]
fn measures_helvetica_width() {
    let w = measure_text_width("Helvetica", 12.0, "Hello").unwrap();
    // Helvetica "Hello" at 12pt is ~28.7pt; assert a sane positive range.
    assert!(w > 20.0 && w < 40.0, "width was {w}");
}

#[test]
fn measure_scales_linearly_with_size() {
    let a = measure_text_width("Helvetica", 10.0, "ABCDEF").unwrap();
    let b = measure_text_width("Helvetica", 20.0, "ABCDEF").unwrap();
    assert!((b - 2.0 * a).abs() < 0.01);
}

#[test]
fn measure_empty_is_zero() {
    assert_eq!(measure_text_width("Helvetica", 12.0, "").unwrap(), 0.0);
}

#[test]
fn measure_unknown_font_errors() {
    assert!(measure_text_width("Comic Sans", 12.0, "x").unwrap_err().contains("font"));
}
```

- [ ] **Step 4:** `cargo test` — expect 79 + 4 = 83 pass. **Step 5:** Add `measure_text_width` to the `fuzz_api` re-export if you want fuzz parity (optional; skip if it complicates — note it). **Step 6: commit** `feat(core): add measure_text`

---

### Task 2: TS — PdfFont, getFont, widthOfTextAtSize

**Files:** Modify `src/core/wasm.ts`, `src/core/wasm-browser.ts`, `src/core/document.ts`, `src/generate/page.ts`, `src/generate/draw-queue.ts`, `src/index.ts`, `src/index.browser.ts`, `src/generate/index.ts`; Create `src/generate/font.ts`; Test `tests/measure-text.test.ts`

- [ ] **Step 1:** `source "$HOME/.cargo/env" && bun run build:wasm`.

- [ ] **Step 2: WASM glue.** In `wasm.ts` and `wasm-browser.ts`: import `measure_text`, add wrapper `measureText(font: string, size: number, text: string): number { return measure_text(font, size, text); }` (browser version calls `ensureInitialized()`).

- [ ] **Step 3: `src/generate/font.ts`:**

```ts
import { StandardFonts } from "./fonts.js";

/** A standard-14 font handle for measuring text. Obtain with `doc.getFont(...)`. */
export class PdfFont {
  /** @internal */
  constructor(
    /** The standard-14 base font name (also a {@link StandardFonts} value). */
    readonly name: StandardFonts,
    private readonly measure: (font: string, size: number, text: string) => number,
  ) {}

  /** Width in points of `text` at `size` in this font. */
  widthOfTextAtSize(text: string, size: number): number {
    if (!Number.isFinite(size) || size <= 0) {
      throw new RangeError(`size must be > 0, got ${size}`);
    }
    return this.measure(this.name, size, text);
  }
}
```

- [ ] **Step 4: `getFont` on `PdfDocumentBase`** (src/core/document.ts):
  - Add `measureText(font: string, size: number, text: string): number;` to the `CoreWasm` interface.
  - Import `PdfFont` from `../generate/font.js` and `StandardFonts` from `../generate/fonts.js`.
  - Add:
    ```ts
    /** Get a standard-14 font handle for measuring or drawing text. */
    getFont(font: StandardFonts): PdfFont {
      return new PdfFont(font, (f, s, t) => this.wasm.measureText(f, s, t));
    }
    ```
  - This works in both load and create modes (pure measurement, no bytes needed).

- [ ] **Step 5: widen `drawText` font option** (src/generate/page.ts) to accept a `PdfFont` too, backward compatible:
  - Change `DrawTextOptions.font?: StandardFonts;` to `font?: StandardFonts | PdfFont;`.
  - In `drawText`, resolve the name: `const fontName = options.font instanceof PdfFont ? options.font.name : (options.font ?? StandardFonts.Helvetica);` and pass `fontName` (a string) to `this.queue.pushText`. (The queue already stores `font: string`.)
  - Import `PdfFont` from `./font.js`.
  - Existing behavior with `StandardFonts` or omitted unchanged.

- [ ] **Step 6: exports.** Add to BOTH root entries' re-export blocks and `src/generate/index.ts`:

```ts
export { PdfFont } from "./generate/font.js";   // adjust path in generate/index.ts to "./font.js"
```

- [ ] **Step 7: tests** `tests/measure-text.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument, StandardFonts } from "../src/index.ts";

async function doc() {
  return PdfDocument.create();
}

describe("text measurement", () => {
  test("widthOfTextAtSize positive and scales with size", async () => {
    const d = await doc();
    const font = d.getFont(StandardFonts.Helvetica);
    const w12 = font.widthOfTextAtSize("Hello", 12);
    expect(w12).toBeGreaterThan(0);
    const w24 = font.widthOfTextAtSize("Hello", 24);
    expect(Math.abs(w24 - 2 * w12)).toBeLessThan(0.01);
  });

  test("empty string is zero width", async () => {
    const d = await doc();
    expect(d.getFont(StandardFonts.Courier).widthOfTextAtSize("", 12)).toBe(0);
  });

  test("Courier is monospaced", async () => {
    const d = await doc();
    const f = d.getFont(StandardFonts.Courier);
    expect(f.widthOfTextAtSize("MM", 10)).toBeCloseTo(2 * f.widthOfTextAtSize("M", 10), 5);
  });

  test("widthOfTextAtSize rejects non-positive size", async () => {
    const d = await doc();
    const f = d.getFont(StandardFonts.Helvetica);
    expect(() => f.widthOfTextAtSize("x", 0)).toThrow(RangeError);
  });

  test("getFont works on a loaded document too", async () => {
    const bytes = new Uint8Array(
      await Bun.file("tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf").arrayBuffer(),
    );
    const loaded = await PdfDocument.load(bytes);
    expect(loaded.getFont(StandardFonts.Helvetica).widthOfTextAtSize("Hi", 12)).toBeGreaterThan(0);
  });

  test("a PdfFont can be passed to drawText", async () => {
    const d = await doc();
    const font = d.getFont(StandardFonts.TimesBold);
    d.addPage().drawText("Times", { x: 10, y: 10, size: 12, font });
    const out = await d.save();
    expect(new TextDecoder("latin1").decode(out)).toContain("(Times) Tj");
    // Times-Bold base font registered
    expect(new TextDecoder("latin1").decode(out)).toContain("Times-Bold");
  });
});
```

- [ ] **Step 8:** `bun run typecheck && bun test` (85 + 6). `bun run build:js`, import all 5 entries + assert `PdfFont` on `.` and `./generate`, `bun run scripts/browser-entry-smoke.ts`. Iterate.
- [ ] **Step 9: commit** `feat: PdfFont, getFont, and widthOfTextAtSize`

---

### Task 3: Benchmarks for generation

**Files:** Modify `bench/bench.ts`

- [ ] **Step 1:** Read `bench/bench.ts` fully to learn its scenario/timing harness (it has a helper that times an iteration loop and prints per-scenario results, comparing against pdf-lib where applicable). Follow its exact structure and printing style.

- [ ] **Step 2:** Add generation scenarios (better-pdf high-level API; add a pdf-lib comparison only where pdf-lib has a direct equivalent — create+drawText and create+drawImage do; otherwise mark better-pdf-only, as the file already does for signatures):
  - "create + draw text": `PdfDocument.create()` → `addPage(PageSizes.A4)` → ~20 `drawText` calls → `save()`. pdf-lib equivalent: `PDFDocument.create()` + `drawText`.
  - "stamp text on existing": load the small fixture → `getPage(0).drawText(...)` a few times → `save()`. pdf-lib equivalent: load + `drawText`.
  - "create + draw image": create → addPage → `embedPng` a small image → `drawImage` → save. Use a small inline PNG (reuse the 1×1 PNG byte pattern used in tests, or a slightly larger generated one). pdf-lib equivalent: `embedPng` + `drawImage`.
  - "create + vector shapes": create → addPage → a handful of `drawRectangle`/`drawLine`/`drawEllipse` → save. Mark better-pdf-only (pdf-lib's shape API differs enough; comparing is optional).
  Import what you need (`PageSizes` etc.) from `../src/index.ts`.

- [ ] **Step 3:** `bun run bench` runs cleanly end-to-end and prints the new scenarios. (Numbers are informational; just confirm no errors and sane output.)
- [ ] **Step 4: commit** `bench: add PDF generation scenarios`

---

### Task 4: Docs + release (README, migration guide, changelog, version bump)

**Files:** Modify `README.md`, `docs/migrating-from-pdf-lib.md`, `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`

- [ ] **Step 1: README.** Update the status/intro line (it currently says the package focuses on existing AcroForms only — broaden it: still AcroForm-first, now also generates and draws). Add a **Generating & drawing** section after the existing form usage, with concise runnable examples mirroring the real API:
  - create a document, addPage(PageSizes.A4), getFont, drawText (with x/y/size/color), save.
  - stamp text/image on an existing PDF: load → embedPng → getPage(0).drawImage / drawText → save.
  - vector: drawRectangle/drawLine/drawEllipse with color/borderColor/opacity.
  - mention `widthOfTextAtSize` for layout, the `./generate` subpath, standard-14-only/WinAnsi limitation, and PDF coordinate convention (origin bottom-left). Keep examples copy-pasteable and consistent with the actual exported names. Also add `./forms` and `./generate` to any "Exports/Entry points" listing if present.

- [ ] **Step 2: migration guide.** In `docs/migrating-from-pdf-lib.md`, add a short section mapping pdf-lib generation APIs to better-pdf: `PDFDocument.create`→`PdfDocument.create`, `addPage`→`addPage(PageSizes.X)`, `embedJpg/embedPng`→same, `page.drawText/drawImage/drawRectangle/drawLine/drawEllipse`→same names, `rgb/grayscale`→same, `StandardFonts`→same, `font.widthOfTextAtSize`→same. Note differences: standard-14 only (no custom font embedding yet), no form-field creation on new docs, RGB/grayscale only, ellipse uses `xScale`/`yScale` radii with center `(x,y)`.

- [ ] **Step 3: CHANGELOG.** Under `## [Unreleased]`, add a `## [0.2.0] - 2026-06-13` section (move Unreleased content if any) with **Added** bullets summarizing M20–M24: page access (`getPageCount`/`getPages`/`getPage`), `drawText` on existing + created pages, `PdfDocument.create`/`addPage`/`PageSizes`, `embedJpg`/`embedPng`/`drawImage`, `drawLine`/`drawRectangle`/`drawEllipse` with opacity, `rgb`/`grayscale`/`StandardFonts`, `getFont`/`widthOfTextAtSize`, `./forms` + `./generate` subpath exports, `PageOutOfRangeError`/`InvalidImageError`. Keep the Keep-a-Changelog format already in the file.

- [ ] **Step 4: version bump.** Set `package.json` `"version"` to `0.2.0` and `crates/core/Cargo.toml` `version` to `0.2.0`. (Leave the lockfile to update on next build; run `source "$HOME/.cargo/env" && cargo update -p better-pdf-core --precise 0.2.0 2>/dev/null || true` is unnecessary — `cargo build`/test will refresh Cargo.lock; if Cargo.lock has a version entry for the crate, update it too so it stays consistent.)

- [ ] **Step 5:** Verify nothing broke: `bun run typecheck && bun test`, `source "$HOME/.cargo/env" && cargo test --manifest-path crates/core/Cargo.toml`, `bun run build:js`. All green.
- [ ] **Step 6: commit** `docs: document PDF generation; release 0.2.0`

---

### Final verification (whole milestone + whole feature)

- [ ] `cargo test` 0 fail; typecheck clean; `bun test` 0 fail.
- [ ] `bun run build:js`; all 5 exports-map entries resolve; `PdfFont` exported from root + `./generate`.
- [ ] `bun run scripts/browser-entry-smoke.ts` passes; `bun run bench` runs.
- [ ] `package.json` and `crates/core/Cargo.toml` both at 0.2.0; CHANGELOG has a 0.2.0 section; README documents generation.
- [ ] Spot-check `npm pack --dry-run` ships `dist/generate/*` and the rebuilt `pkg-web/*`.
