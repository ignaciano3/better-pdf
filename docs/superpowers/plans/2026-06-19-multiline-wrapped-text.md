# Multi-line / Wrapped Text Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `maxWidth` option to `page.drawText` that word-wraps text to fit a given width, for both standard-14 and embedded fonts.

**Architecture:** Wrapping happens entirely in TypeScript by inserting `\n` at greedy word boundaries; the Rust renderer already splits on `\n` and advances baselines via the PDF `T*` operator, so no Rust change is needed. Width is measured with the same path the renderer uses: `PdfFont.widthOfTextAtSize` for `PdfFont` handles, and an injected standard-14 measurer for bare `StandardFonts` strings.

**Tech Stack:** TypeScript (library API in `src/generate`), Bun test runner, existing WASM measurement (`measure_text` / `measure_text_embedded`).

## Global Constraints

- Coordinates use PDF convention (origin bottom-left); `y` is the baseline of the first line. Unchanged.
- Wrapping must not alter output when `maxWidth` is omitted — existing `drawText` behavior is byte-for-byte preserved.
- Explicit `\n` in the input text are hard breaks and must be preserved; wrapping applies within each hard-break paragraph.
- A single word wider than `maxWidth` overflows onto its own line (no mid-word breaking in v1).
- No new dependencies. No Rust changes.
- Measurement for wrapping must use the same metric as rendering: standard-14 → WinAnsi width tables (`wasm.measureText`); embedded → glyph advances (`wasm.measureTextEmbedded` via `PdfFont.widthOfTextAtSize`).
- Version bump for this feature: **0.14.0** (new backward-compatible API). Adjust if a different milestone ships first.

---

### Task 1: Pure word-wrap function

**Files:**
- Create: `src/generate/wrap-text.ts`
- Test: `tests/wrap-text.test.ts`

**Interfaces:**
- Produces: `export function wrapText(text: string, maxWidth: number, measure: (s: string) => number): string` — returns the input with `\n` inserted at greedy word boundaries so each line's measured width is `<= maxWidth` (except single words that exceed it). Existing `\n` are preserved as hard breaks.

- [ ] **Step 1: Write the failing test**

```ts
// tests/wrap-text.test.ts
import { expect, test } from "bun:test";
import { wrapText } from "../src/generate/wrap-text.js";

// Deterministic measurer: 1 unit per character.
const len = (s: string): number => s.length;

test("greedy wraps words to fit maxWidth", () => {
  expect(wrapText("aaa bbb ccc", 7, len)).toBe("aaa bbb\nccc");
});

test("preserves explicit newlines as hard breaks", () => {
  expect(wrapText("aa\nbb cc", 3, len)).toBe("aa\nbb\ncc");
});

test("a single word wider than maxWidth overflows on its own line", () => {
  expect(wrapText("aaaaaa bb", 3, len)).toBe("aaaaaa\nbb");
});

test("text that already fits is returned unchanged", () => {
  expect(wrapText("hi there", 100, len)).toBe("hi there");
});

test("empty string returns empty string", () => {
  expect(wrapText("", 10, len)).toBe("");
});

test("collapses runs of spaces inside a paragraph to single spaces", () => {
  // words are split on spaces and rejoined with a single space
  expect(wrapText("aa   bb", 100, len)).toBe("aa bb");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/wrap-text.test.ts`
Expected: FAIL — `Cannot find module '../src/generate/wrap-text.js'`.

- [ ] **Step 3: Write minimal implementation**

```ts
// src/generate/wrap-text.ts

/**
 * Word-wrap `text` so each line's measured width is `<= maxWidth`. Existing
 * `\n` characters are preserved as hard breaks; wrapping is applied within each
 * resulting paragraph. A single word wider than `maxWidth` is placed on its own
 * line (no mid-word breaking). Runs of spaces collapse to a single space.
 *
 * @param measure - returns the rendered width of a string at the caller's font/size.
 */
export function wrapText(
  text: string,
  maxWidth: number,
  measure: (s: string) => number,
): string {
  return text
    .split("\n")
    .map((para) => wrapParagraph(para, maxWidth, measure))
    .join("\n");
}

function wrapParagraph(
  para: string,
  maxWidth: number,
  measure: (s: string) => number,
): string {
  const words = para.split(/\s+/).filter((w) => w.length > 0);
  if (words.length === 0) return "";
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    const candidate = current === "" ? word : `${current} ${word}`;
    if (current === "" || measure(candidate) <= maxWidth) {
      current = candidate;
    } else {
      lines.push(current);
      current = word;
    }
  }
  if (current !== "") lines.push(current);
  return lines.join("\n");
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun test tests/wrap-text.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/generate/wrap-text.ts tests/wrap-text.test.ts
git commit -m "feat(text): add pure wrapText word-wrap helper"
```

---

### Task 2: Wire `maxWidth` into `drawText` (standard + embedded fonts)

**Files:**
- Modify: `src/generate/page.ts` — `DrawTextOptions` interface (lines 11-26), `PdfPage` constructor (lines 162-176), `drawText` (lines 183-221)
- Modify: `src/core/document.ts` — the three `new PdfPage(...)` construction sites (`loadPages` ~line 441, create-mode `addPage` ~line 301, load-mode `addPage` ~line 312)
- Test: `tests/draw-text.test.ts` (append new tests), `tests/font-embedding.test.ts` (append one test)

**Interfaces:**
- Consumes: `wrapText(text, maxWidth, measure)` from Task 1; `PdfFont.widthOfTextAtSize(text, size)` (existing, `src/generate/font.ts:35`).
- Produces: `DrawTextOptions.maxWidth?: number`; `PdfPage` constructor gains a 7th param `measureStd?: (font: string, size: number, text: string) => number`.

**Context:** `PdfPage` does NOT hold the WASM binding. Standard-14 width measurement (`wasm.measureText`) must be injected from `PdfDocument` (which holds `this.wasm`). For `PdfFont` handles, `font.widthOfTextAtSize` already routes to the correct WASM measurer (standard or embedded), so no injection is needed in that branch.

- [ ] **Step 1: Write the failing test**

Append to `tests/draw-text.test.ts`:

```ts
test("maxWidth wraps a long line into multiple Tj lines (standard font)", async () => {
  const doc = await load();
  // A long string with no explicit newlines; small maxWidth forces wrapping.
  doc.getPage(0).drawText("the quick brown fox jumps over the lazy dog", {
    x: 40,
    y: 650,
    size: 12,
    maxWidth: 80,
  });
  const out = await doc.save();
  const s = new TextDecoder("latin1").decode(out);
  // More than one (...) Tj means it wrapped onto multiple lines.
  const tjCount = (s.match(/\) Tj/g) ?? []).length;
  expect(tjCount).toBeGreaterThan(1);
});

test("maxWidth respects explicit newlines as hard breaks", async () => {
  const doc = await load();
  doc.getPage(0).drawText("alpha beta\ngamma delta", {
    x: 40,
    y: 650,
    size: 12,
    maxWidth: 1000, // wide enough that only the explicit \n breaks
  });
  const out = await doc.save();
  const s = new TextDecoder("latin1").decode(out);
  expect(s).toContain("(alpha beta) Tj");
  expect(s).toContain("(gamma delta) Tj");
});

test("maxWidth must be a positive finite number", async () => {
  const doc = await load();
  const page = doc.getPage(0);
  expect(() => page.drawText("x", { x: 10, y: 10, size: 12, maxWidth: 0 })).toThrow(
    RangeError,
  );
  expect(() =>
    page.drawText("x", { x: 10, y: 10, size: 12, maxWidth: -5 }),
  ).toThrow(RangeError);
  expect(() =>
    page.drawText("x", { x: 10, y: 10, size: 12, maxWidth: Infinity }),
  ).toThrow(RangeError);
});

test("omitting maxWidth leaves a single-line draw unchanged", async () => {
  const doc = await load();
  doc.getPage(0).drawText("unwrapped single line", { x: 40, y: 650, size: 12 });
  const out = await doc.save();
  const s = new TextDecoder("latin1").decode(out);
  expect(s).toContain("(unwrapped single line) Tj");
});
```

> Note: `load()` is the existing helper at the top of `tests/draw-text.test.ts` that loads the FICHA fixture. Reuse it; do not redefine it.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/draw-text.test.ts`
Expected: FAIL — `maxWidth` is not yet handled (no wrapping → `tjCount` is 1; and no validation → the `toThrow` cases fail).

- [ ] **Step 3a: Add `maxWidth` to `DrawTextOptions`**

In `src/generate/page.ts`, add to the `DrawTextOptions` interface (after the `opacity?` field, around line 25):

```ts
  /** Opacity 0..1. Default 1 (fully opaque). */
  opacity?: number;
  /**
   * Maximum line width in PDF points. When set, text is word-wrapped to fit:
   * `\n` are kept as hard breaks, and a word wider than `maxWidth` overflows
   * onto its own line. Must be a positive finite number.
   */
  maxWidth?: number;
```

- [ ] **Step 3b: Inject a standard-14 measurer into `PdfPage`**

In `src/generate/page.ts`, change the constructor (lines 162-176) to accept and store a measurer:

```ts
  /**
   * Stable slot id used to resolve this page's final index at save time.
   * Loaded pages use their original index; appended pages use a negative
   * sentinel; created pages reuse `index`. Draw ops carry this, not `index`,
   * so a later insert/remove/move re-targets draws onto the right page.
   * @internal
   */
  private readonly _slot: number;

  /** @internal Measures standard-14 text width; injected by PdfDocument. */
  private readonly measureStd?: (font: string, size: number, text: string) => number;

  /** @internal */
  constructor(
    /** Zero-based page index. */
    readonly index: number,
    /** Page width in PDF points. */
    readonly width: number,
    /** Page height in PDF points. */
    readonly height: number,
    /** Page rotation in degrees (0, 90, 180, or 270). */
    readonly rotation: number,
    private readonly queue: DrawQueue,
    slot?: number,
    measureStd?: (font: string, size: number, text: string) => number,
  ) {
    this._slot = slot ?? index;
    this.measureStd = measureStd;
  }
```

- [ ] **Step 3c: Apply wrapping + validation in `drawText`**

In `src/generate/page.ts`, add the import at the top (with the other `./` imports):

```ts
import { wrapText } from "./wrap-text.js";
```

Then, inside `drawText`, after the existing `validateOpacity(options.opacity);` line and before the `const embeddedId = ...` line, insert:

```ts
    let text2 = text;
    if (options.maxWidth !== undefined) {
      if (!Number.isFinite(options.maxWidth) || options.maxWidth <= 0) {
        throw new RangeError(`maxWidth must be > 0, got ${options.maxWidth}`);
      }
      const size = options.size;
      const measure =
        options.font instanceof PdfFont
          ? (s: string) => (options.font as PdfFont).widthOfTextAtSize(s, size)
          : (s: string) => {
              if (!this.measureStd) {
                throw new Error(
                  "text measurement is unavailable on this page; cannot wrap text",
                );
              }
              const name =
                (options.font as StandardFonts | undefined) ?? StandardFonts.Helvetica;
              return this.measureStd(name, size, s);
            };
      text2 = wrapText(text, options.maxWidth, measure);
    }
```

Then change the queue push to use `text2` instead of `text` (the call near line 210):

```ts
    this.queue.pushText(this._slot, text2, {
```

- [ ] **Step 3d: Pass the measurer from `PdfDocument` at all construction sites**

In `src/core/document.ts`:

`loadPages` (the `infos.map` call, ~line 441):

```ts
      this.pages = infos.map(
        (p) =>
          new PdfPage(
            p.index,
            p.width,
            p.height,
            p.rotation,
            this.drawQueue,
            p.index,
            (f, s, t) => this.wasm.measureText(f, s, t),
          ),
      );
```

create-mode `addPage` (~line 301):

```ts
      const page = new PdfPage(
        index,
        width,
        height,
        0,
        this.drawQueue,
        undefined,
        (f, s, t) => this.wasm.measureText(f, s, t),
      );
```

load-mode `addPage` (~line 312):

```ts
    const page = new PdfPage(
      index,
      width,
      height,
      0,
      this.drawQueue,
      slot,
      (f, s, t) => this.wasm.measureText(f, s, t),
    );
```

- [ ] **Step 4: Run tests + typecheck**

Run: `bun test tests/draw-text.test.ts && bunx tsc --noEmit`
Expected: PASS — all draw-text tests green, tsc clean.

- [ ] **Step 5: Add embedded-font wrap test**

Append to `tests/font-embedding.test.ts` (it already imports `pdfjs`, `readFileSync`, and the font fixture — reuse the existing fixture path constant used by other tests in that file):

```ts
test("maxWidth wraps text rendered with an embedded font", async () => {
  const doc = await PdfDocument.create();
  const fontBytes = readFileSync("tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
  const font = await doc.embedFont(fontBytes);
  const page = doc.addPage(PageSizes.A4);
  page.drawText("the quick brown fox jumps over the lazy dog", {
    x: 40,
    y: 700,
    size: 12,
    font,
    maxWidth: 80,
  });
  const out = await doc.save();
  const s = new TextDecoder("latin1").decode(out);
  // Embedded text renders as hex <....> Tj; wrapping yields more than one.
  const tjCount = (s.match(/> Tj/g) ?? []).length;
  expect(tjCount).toBeGreaterThan(1);
});
```

> Note: confirm `PageSizes` and `PdfDocument` are imported at the top of `tests/font-embedding.test.ts`; add them to the existing import if missing.

- [ ] **Step 6: Run the embedded test + full suite**

Run: `bun test tests/font-embedding.test.ts && bun test`
Expected: PASS, 0 fail.

- [ ] **Step 7: Commit**

```bash
git add src/generate/page.ts src/core/document.ts tests/draw-text.test.ts tests/font-embedding.test.ts
git commit -m "feat(text): word-wrap drawText via maxWidth option (standard + embedded fonts)"
```

---

### Task 3: Docs, changelog, version bump

**Files:**
- Modify: `docs/site/src/content/docs/reference/limitations.md` (the "Drawing APIs ... fonts" bullet, lines 12-20)
- Modify: `skills/better-pdf/SKILL.md` (drawText section)
- Modify: `CHANGELOG.md` (lines 9-11, the `[Unreleased]` / top release area)
- Modify: `package.json` (line 3, `"version"`)
- Modify: `crates/core/Cargo.toml` (line 3, `version`)
- Modify: `crates/core/Cargo.lock` (both `version = "..."` lines for `better-pdf-core`)

**Interfaces:** none (docs/metadata only).

- [ ] **Step 1: Update limitations doc**

In `docs/site/src/content/docs/reference/limitations.md`, replace the closing of the fonts bullet (the line `- Characters with no glyph in the font are silently skipped.`, line 20) by adding a sibling bullet immediately after it:

```md
- Characters with no glyph in the font are silently skipped.
- **Multi-line text:** `drawText` honors `\n` as hard line breaks, and the
  `maxWidth` option word-wraps text to fit a given width (added in 0.14.0). A
  single word wider than `maxWidth` overflows onto its own line; mid-word
  breaking and text alignment are not yet supported.
```

- [ ] **Step 2: Update the skill doc**

In `skills/better-pdf/SKILL.md`, find the `drawText` documentation and add a line documenting `maxWidth`:

```md
- `maxWidth` (number, optional): word-wrap text to this width in points. `\n` are kept as hard breaks. Works with standard-14 and embedded fonts.
```

Place it in the `drawText` options list, next to `lineHeight`/`rotate`/`opacity`. If no per-option list exists, add a short sentence under the `drawText` description.

- [ ] **Step 3: Update changelog**

In `CHANGELOG.md`, under `## [Unreleased]` (line 9), add a new release section:

```md
## [Unreleased]

## [0.14.0] - 2026-06-19

### Added

- `page.drawText` now accepts a `maxWidth` option that word-wraps text to fit the given width in points. Explicit `\n` remain hard breaks; a word wider than `maxWidth` overflows onto its own line. Works for both standard-14 and embedded fonts.
```

- [ ] **Step 4: Bump versions**

- `package.json` line 3: `"version": "0.14.0",`
- `crates/core/Cargo.toml` line 3: `version = "0.14.0"`
- Refresh `crates/core/Cargo.lock`:

```bash
source ~/.cargo/env
cargo update -p better-pdf-core --precise 0.14.0 --manifest-path crates/core/Cargo.toml
```

- [ ] **Step 5: Verify the whole build is green**

Run: `source ~/.cargo/env && bunx tsc --noEmit && bun test`
Expected: tsc clean; bun suite passes, 0 fail.

- [ ] **Step 6: Commit**

```bash
git add docs/site CHANGELOG.md package.json crates/core/Cargo.toml crates/core/Cargo.lock skills/better-pdf/SKILL.md
git commit -m "docs: document maxWidth text wrapping; release 0.14.0"
```

---

## Final Whole-Branch Review

After all tasks, dispatch the final code review (superpowers:requesting-code-review) on the most capable model, then use superpowers:finishing-a-development-branch to merge to master (`--no-ff`), per the project norm. The user pushes manually.

## Self-Review Notes

- **Spec coverage:** wrapping (`maxWidth`) ✅; hard `\n` preserved ✅; standard + embedded measurement ✅; no-`maxWidth` unchanged ✅; validation ✅; docs/version ✅.
- **Type consistency:** `wrapText(text, maxWidth, measure)` signature is identical in Task 1 (definition) and Task 2 (call). `PdfPage` constructor 7th param `measureStd` matches all three call sites. `DrawTextOptions.maxWidth?: number` matches the validation and call.
- **No Rust changes:** the renderer's existing `\n` → `T*` handling (`crates/core/src/draw.rs` `emit_text_block` / `emit_text_block_cid`) is reused; the plan adds no Rust code.
