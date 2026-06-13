# Milestone 20 — Pages + drawText on Existing PDFs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read page info from loaded PDFs and draw text (standard-14 fonts) on their pages via incremental update, exposed through `doc.getPage(n).drawText(...)` and a new `./generate` subpath.

**Architecture:** Two new stateless WASM exports (`read_pages`, `apply_draw_ops`) following the existing thin-wrapper pattern in `crates/core/src/lib.rs:11-27`. Draw ops queue on the TS side (same pattern as the fill queue, `src/forms/fields.ts` `FillQueue`) and are applied at `save()` after fills/flattens. The duplicated `PdfDocument` in `src/index.ts`/`src/index.browser.ts` is deduplicated into `src/core/document.ts` first, so draw logic is written once.

**Tech Stack:** Rust (lopdf 0.41 `IncrementalDocument`, serde), wasm-bindgen, TypeScript ESM, bun test.

**Spec:** `docs/superpowers/specs/2026-06-12-pdf-generation-design.md`

**Environment notes:** `source "$HOME/.cargo/env"` before cargo/wasm-pack. After any Rust change run `bun run build:wasm` once, then TS tasks. Baseline: `bun test` 50 pass / 4 skip / 0 fail; `cargo test --manifest-path crates/core/Cargo.toml` 41 pass.

**Key reuse points (read these before implementing):**
- Incremental save: `crates/core/src/fill.rs:43-52` (`IncrementalDocument::create_from` + `save_to`).
- Appending a stream to page `/Contents` (handles single-stream and array): `crates/core/src/flatten.rs:183-191`.
- Resource registration on a page (inline vs referenced `/Resources`): `flatten.rs:265-305` (`register_xobject`/`set_xobject`).
- WinAnsi encoding: `appearance.rs:74` (`encode_winansi`); escaping for literal strings: `appearance.rs:152-177`.
- Standard-14 metrics: `appearance.rs:33-64` (`standard_14_widths`), widths in 1/1000 em.
- Page iteration: `Document::get_pages()` (`flatten.rs:97`), page index map: `forms.rs:62-68`.
- Errors: plain `String` errors mapped via `JsError::new` in lib.rs.

---

### Task 1: Rust — read_pages

**Files:**
- Create: `crates/core/src/pages.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Write failing Rust test** (in `pages.rs` `#[cfg(test)] mod tests`, following the include_bytes pattern of `forms.rs:278`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn reads_page_list() {
        let json = read_pages_json(FICHA).unwrap();
        let pages: Vec<PageInfo> = serde_json::from_str(&json).unwrap();
        assert!(!pages.is_empty());
        assert_eq!(pages[0].index, 0);
        // A4-ish or letter-ish: sane positive dimensions
        assert!(pages[0].width > 100.0 && pages[0].height > 100.0);
        assert_eq!(pages[0].rotation % 90, 0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read_pages_json(b"not a pdf").is_err());
    }
}
```

- [ ] **Step 2: Implement `pages.rs`:**

```rust
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PageInfo {
    pub index: usize,
    pub width: f32,
    pub height: f32,
    pub rotation: i64,
}

/// Resolve a page attribute that may be inherited via /Parent (PDF 32000-1 7.7.3.4).
fn inherited<'a>(doc: &'a Document, page_id: lopdf::ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut current = page_id;
    for _ in 0..32 {
        let dict = doc.get_dictionary(current).ok()?;
        if let Ok(v) = dict.get(key) {
            // Deref indirect references
            return match v {
                Object::Reference(r) => doc.get_object(*r).ok(),
                other => Some(other),
            };
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(r)) => current = *r,
            _ => return None,
        }
    }
    None
}

fn rect_f32(arr: &[Object]) -> Option<[f32; 4]> {
    if arr.len() != 4 { return None; }
    let mut out = [0f32; 4];
    for (i, o) in arr.iter().enumerate() {
        out[i] = match o {
            Object::Integer(n) => *n as f32,
            Object::Real(n) => *n,
            _ => return None,
        };
    }
    Some(out)
}

pub fn read_pages_json(data: &[u8]) -> Result<String, String> {
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    let mut pages = Vec::new();
    for (i, (_, page_id)) in doc.get_pages().iter().enumerate() {
        let media = inherited(&doc, *page_id, b"MediaBox")
            .and_then(|o| o.as_array().ok())
            .and_then(|a| rect_f32(a))
            .ok_or_else(|| format!("page {i}: missing or invalid MediaBox"))?;
        let rotation = inherited(&doc, *page_id, b"Rotate")
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        pages.push(PageInfo {
            index: i,
            width: (media[2] - media[0]).abs(),
            height: (media[3] - media[1]).abs(),
            rotation: rotation.rem_euclid(360),
        });
    }
    serde_json::to_string(&pages).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Export in `lib.rs`** (same thin-wrapper pattern as existing three):

```rust
mod pages;

#[wasm_bindgen]
pub fn read_pages(data: &[u8]) -> Result<String, JsError> {
    pages::read_pages_json(data).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 4: Run** `cargo test --manifest-path crates/core/Cargo.toml` — expect 43 pass (41 + 2).
- [ ] **Step 5: Commit** `feat(core): add read_pages`

---

### Task 2: Rust — apply_draw_ops (text)

**Files:**
- Create: `crates/core/src/draw.rs` (single file for M20; split into draw/ dir when images land in M22)
- Modify: `crates/core/src/lib.rs`

**Draw ops JSON contract** (the TS side in Task 5 must produce exactly this):

```json
[
  {
    "op": "text",
    "page": 0,
    "x": 50.0, "y": 700.0,
    "size": 24.0,
    "font": "Helvetica",
    "color": [0.0, 0.0, 0.0],
    "text": "Hello world",
    "lineHeight": 28.0
  }
]
```

`font` is a standard-14 base name (the values of the TS `StandardFonts` enum): Helvetica, Helvetica-Bold, Helvetica-Oblique, Helvetica-BoldOblique, Courier, Courier-Bold, Courier-Oblique, Courier-BoldOblique, Times-Roman, Times-Bold, Times-Italic, Times-BoldItalic. `color` is RGB 0..1. `lineHeight` optional (defaults to `1.15 * size`); `text` may contain `\n` → multiple lines, each `lineHeight` lower.

- [ ] **Step 1: Write failing Rust tests** (in `draw.rs` tests mod):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn ops(json: &str) -> Vec<u8> {
        apply_draw_ops_json(FICHA, json).unwrap()
    }

    #[test]
    fn output_is_incremental() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#);
        assert_eq!(&out[..FICHA.len()], FICHA);
        assert!(out.len() > FICHA.len());
    }

    #[test]
    fn page_contents_grow_and_balance() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#);
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        // Contents must now be an array: [q-wrap, ...orig, Q-wrap, draw]
        let contents = dict.get(b"Contents").unwrap();
        let arr = match contents {
            lopdf::Object::Array(a) => a.clone(),
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_array().unwrap().clone(),
            _ => panic!("expected contents array"),
        };
        assert!(arr.len() >= 3);
        // Draw stream contains our text encoded WinAnsi inside Tj
        let draw_id = arr.last().unwrap().as_reference().unwrap();
        let stream = doc.get_object(draw_id).unwrap().as_stream().unwrap();
        let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
        let s = String::from_utf8_lossy(&content);
        assert!(s.contains("(Hello) Tj"), "content was: {s}");
        assert!(s.contains("BT") && s.contains("ET"));
    }

    #[test]
    fn font_registered_in_page_resources() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Times-Bold","color":[0,0,0],"text":"x"}]"#);
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        let res = match dict.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap(),
            lopdf::Object::Dictionary(d) => d,
            _ => panic!(),
        };
        let fonts = match res.get(b"Font").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap(),
            lopdf::Object::Dictionary(d) => d,
            _ => panic!(),
        };
        assert!(fonts.iter().any(|(k, _)| k.starts_with(b"BPF")));
    }

    #[test]
    fn errors_on_bad_page() {
        let r = apply_draw_ops_json(FICHA, r#"[{"op":"text","page":999,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#);
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = apply_draw_ops_json(FICHA, r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#);
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn multiline_emits_multiple_tj() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":12,"font":"Helvetica","color":[0,0,0],"text":"a\nb"}]"#);
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        let arr = match dict.get(b"Contents").unwrap() {
            lopdf::Object::Array(a) => a.clone(),
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_array().unwrap().clone(),
            _ => panic!(),
        };
        let draw_id = arr.last().unwrap().as_reference().unwrap();
        let stream = doc.get_object(draw_id).unwrap().as_stream().unwrap();
        let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
        let s = String::from_utf8_lossy(&content);
        assert!(s.matches(" Tj").count() == 2, "content was: {s}");
    }
}
```

- [ ] **Step 2: Implement `draw.rs`.** Structure:

```rust
use lopdf::{dictionary, Document, IncrementalDocument, Object, Stream};
use serde::Deserialize;

use crate::appearance::encode_winansi;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum DrawOp {
    Text {
        page: usize,
        x: f32,
        y: f32,
        size: f32,
        font: String,
        color: [f32; 3],
        text: String,
        #[serde(rename = "lineHeight")]
        line_height: Option<f32>,
    },
}

const STANDARD_14: &[&str] = &[
    "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique",
    "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
    "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic",
];

pub fn apply_draw_ops_json(data: &[u8], ops_json: &str) -> Result<Vec<u8>, String> { ... }
```

Implementation requirements:
1. Parse ops (serde). Validate every `font` against `STANDARD_14` ("unknown font: {name}") and every `page` against page count ("page {n} out of range") BEFORE mutating anything.
2. Load via `IncrementalDocument::create_from(data.to_vec(), Document::load_mem(data)...)` — same as `fill.rs:43`.
3. Group ops by page. For each touched page:
   a. Build one draw content stream: for each text op emit
      ```
      BT
      /BPF{n} {size} Tf
      {r} {g} {b} rg
      {lh} TL
      {x} {y} Td
      ({escaped winansi line}) Tj
      T*  (for each subsequent line)
      ({line2}) Tj
      ET
      ```
      Escape `(`, `)`, `\` with backslash (see `appearance.rs:152-177`). `TL` is leading = `line_height.unwrap_or(size * 1.15)`. Numbers formatted with up to 2 decimals, trailing zeros trimmed.
   b. Wrap existing content for graphics-state safety: replace page `/Contents` with an array `[q_stream_ref, ...original_refs, Q_stream_ref, draw_stream_ref]` where q_stream contains exactly `q\n` and Q_stream `Q\n`. If `/Contents` was a single reference, the original_refs is that one ref; if an array, splice all. Share one q-stream and one Q-stream object across pages (create once). Follow the contents-handling pattern at `flatten.rs:183-191` but with the wrap.
   c. Register each used font once per page under `/Resources/Font` as `/BPF{n}` (n = stable index of font in STANDARD_14) with a font dict `{Type: Font, Subtype: Type1, BaseFont: {name}, Encoding: WinAnsiEncoding}` added via `inc.new_document.add_object(...)`. Handle inline vs referenced `/Resources` like `flatten.rs:265-305` — clone the page dict into the new document with `inc.opt_clone_object_to_new_document(page_id)` first (see how flatten.rs gets a mutable page).
4. Save with `inc.save_to(&mut out)` and return.

Numeric formatting helper: write `fn fmt_num(v: f32) -> String` producing e.g. `50`, `700.5`, `0.25`.

- [ ] **Step 3: Export in lib.rs:**

```rust
mod draw;

#[wasm_bindgen]
pub fn apply_draw_ops(data: &[u8], ops_json: &str) -> Result<Vec<u8>, JsError> {
    draw::apply_draw_ops_json(data, ops_json).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 4: Run** `cargo test --manifest-path crates/core/Cargo.toml` — expect 49 pass (43 + 6). Iterate until green.
- [ ] **Step 5: Add fuzz target** `crates/core/fuzz/fuzz_targets/draw_ops.rs` mirroring `fill.rs` fuzz target: feed arbitrary bytes as ops_json against FICHA, assert no panic (errors fine). Register in fuzz Cargo.toml like the existing four. Do NOT run the fuzzer locally; just `cargo check` it if the fuzz crate builds locally, otherwise skip the check and note it.
- [ ] **Step 6: Commit** `feat(core): add apply_draw_ops for text drawing`

---

### Task 3: Rebuild WASM + TS glue

**Files:**
- Modify: `src/core/wasm.ts`, `src/core/wasm-browser.ts`

- [ ] **Step 1:** `source "$HOME/.cargo/env" && bun run build:wasm` — regenerates pkg-web with the two new exports.
- [ ] **Step 2:** Add to `src/core/wasm.ts` (import `read_pages`, `apply_draw_ops` from pkg-web alongside existing):

```ts
export function readPages(data: Uint8Array): string {
  return read_pages(data);
}

export function applyDrawOps(data: Uint8Array, opsJson: string): Uint8Array {
  return apply_draw_ops(data, opsJson);
}
```

Add the same two to `src/core/wasm-browser.ts` with the `ensureInitialized()` guard the existing wrappers use.

- [ ] **Step 3:** `bun run typecheck && bun test` — baseline green (50 pass / 4 skip).
- [ ] **Step 4: Commit** `feat: expose read_pages and apply_draw_ops through WASM glue` (pkg-web is gitignored; only the two TS files change).

---

### Task 4: TS — dedupe PdfDocument into core/document.ts

**Files:**
- Create: `src/core/document.ts`
- Modify: `src/index.ts`, `src/index.browser.ts`

The duplicated class (only diff: browser `load()` awaits `initializeWasm()`) becomes one base class taking a bindings object; entries become thin subclasses. Public API unchanged.

- [ ] **Step 1: Create `src/core/document.ts`:**

```ts
import { PdfForm } from "../forms/form.js";
import { toPdfError } from "./errors.js";
import type { FormSchema, TypedPdfForm } from "../forms/schema.js";

/** WASM bindings a PdfDocument needs; satisfied by both wasm.ts and wasm-browser.ts. @internal */
export interface CoreWasm {
  readFields(data: Uint8Array): string;
  fillFields(data: Uint8Array, opsJson: string, images: Uint8Array): Uint8Array;
  flattenFields(data: Uint8Array, namesJson: string): Uint8Array;
  readPages(data: Uint8Array): string;
  applyDrawOps(data: Uint8Array, opsJson: string): Uint8Array;
}

export class PdfDocumentBase {
  private form?: PdfForm;

  /** @internal */
  protected constructor(
    protected readonly bytes: Uint8Array,
    private readonly wasm: CoreWasm,
  ) {}

  async save(): Promise<Uint8Array> {
    const form = this.form;
    let bytes = this.bytes;
    try {
      if (form && form.queue.length > 0) {
        const { opsJson, images } = form.queue.toPayload();
        bytes = this.wasm.fillFields(bytes, opsJson, images);
      }
      if (form && form.flattenQueue.length > 0) {
        bytes = this.wasm.flattenFields(bytes, JSON.stringify(form.flattenQueue));
      }
    } catch (e) {
      throw toPdfError(e);
    }
    if (bytes === this.bytes) {
      return this.bytes.slice();
    }
    return bytes;
  }

  getForm(): PdfForm;
  getForm<S extends FormSchema>(): TypedPdfForm<S>;
  getForm(): PdfForm {
    if (!this.form) this.form = new PdfForm(this.bytes, this.wasm.readFields);
    return this.form;
  }
}
```

Move the existing doc comments from `src/index.ts` onto the base class members (they are the canonical ones; keep the `@example`s).

- [ ] **Step 2: Shrink `src/index.ts`:**

```ts
import * as wasm from "./core/wasm.js";
import { PdfDocumentBase } from "./core/document.js";

export class PdfDocument extends PdfDocumentBase {
  static async load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument> {
    const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
    return new PdfDocument(bytes, wasm);
  }
}
```

Keep the class-level doc comment and the `load()` doc comment on the entry class (they differ per runtime). All re-exports at the bottom of index.ts stay exactly as-is.

- [ ] **Step 3: Shrink `src/index.browser.ts`** the same way, with:

```ts
static async load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument> {
  await initializeWasm();
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  return new PdfDocument(bytes, wasm);
}
```

(import `* as wasm from "./core/wasm-browser.js"` plus the named `initializeWasm`). Re-exports stay.

- [ ] **Step 4:** `bun run typecheck && bun test` — expect baseline green; also `bun run build:js && bun run scripts/browser-entry-smoke.ts` → "browser entry loaded 28 fields".
- [ ] **Step 5: Commit** `refactor: dedupe PdfDocument into core/document.ts`

---

### Task 5: TS — generate/ module: pages + drawText

**Files:**
- Create: `src/generate/color.ts`, `src/generate/fonts.ts`, `src/generate/draw-queue.ts`, `src/generate/page.ts`
- Modify: `src/core/document.ts`, `src/core/errors.ts`
- Test: `tests/pages.test.ts`, `tests/draw-text.test.ts`

- [ ] **Step 1: Write failing tests.**

`tests/pages.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument } from "../src/index.ts";
import { PageOutOfRangeError } from "../src/core/errors.ts";

const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

async function load() {
  const bytes = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
  return PdfDocument.load(bytes);
}

describe("pages", () => {
  test("getPageCount and getPages", async () => {
    const doc = await load();
    const count = doc.getPageCount();
    expect(count).toBeGreaterThan(0);
    expect(doc.getPages()).toHaveLength(count);
  });

  test("page size and rotation", async () => {
    const doc = await load();
    const page = doc.getPage(0);
    expect(page.width).toBeGreaterThan(100);
    expect(page.height).toBeGreaterThan(100);
    expect(page.rotation % 90).toBe(0);
  });

  test("getPage out of range throws", async () => {
    const doc = await load();
    expect(() => doc.getPage(999)).toThrow(PageOutOfRangeError);
    expect(() => doc.getPage(-1)).toThrow(PageOutOfRangeError);
  });

  test("getPage returns same instance", async () => {
    const doc = await load();
    expect(doc.getPage(0)).toBe(doc.getPage(0));
  });
});
```

`tests/draw-text.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument, rgb, StandardFonts } from "../src/index.ts";

const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

async function load() {
  const bytes = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
  return PdfDocument.load(bytes);
}

describe("drawText", () => {
  test("save appends incremental update containing the text", async () => {
    const doc = await load();
    const original = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
    doc.getPage(0).drawText("STAMPED", { x: 50, y: 700, size: 24 });
    const out = await doc.save();
    expect(out.length).toBeGreaterThan(original.length);
    expect(out.slice(0, original.length)).toEqual(original);
    expect(new TextDecoder("latin1").decode(out)).toContain("(STAMPED) Tj");
  });

  test("no draw ops -> save returns copy of original", async () => {
    const doc = await load();
    doc.getPage(0); // page access alone must not dirty the doc
    const out = await doc.save();
    const original = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
    expect(out).toEqual(original);
  });

  test("draw options: font, color, multiline", async () => {
    const doc = await load();
    doc.getPage(0).drawText("line1\nline2", {
      x: 40, y: 650, size: 12,
      font: StandardFonts.TimesRoman,
      color: rgb(1, 0, 0),
      lineHeight: 14,
    });
    const out = await doc.save();
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain("(line1) Tj");
    expect(s).toContain("(line2) Tj");
    expect(s).toContain("Times-Roman");
  });

  test("composes with form fill in one save", async () => {
    const doc = await load();
    const firstText = doc.getForm().getFields().find((f) => f.type === "text")!;
    doc.getForm().getTextField(firstText.name).setText("VALUE");
    doc.getPage(0).drawText("STAMP", { x: 30, y: 30, size: 10 });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    const field = reloaded.getForm().getFields().find((f) => f.name === firstText.name)!;
    expect(field.value).toBe("VALUE");
    expect(new TextDecoder("latin1").decode(out)).toContain("(STAMP) Tj");
  });

  test("output still parses as a PDF with same page count", async () => {
    const doc = await load();
    const before = doc.getPageCount();
    doc.getPage(0).drawText("x", { x: 10, y: 10, size: 8 });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(before);
  });

  test("invalid options throw before save", async () => {
    const doc = await load();
    const page = doc.getPage(0);
    expect(() => page.drawText("x", { x: 0, y: 0, size: 0 })).toThrow();
    expect(() => page.drawText("x", { x: 0, y: 0, size: -3 })).toThrow();
  });
});
```

Run both: expect FAIL (missing modules/methods).

- [ ] **Step 2: `src/generate/color.ts`:**

```ts
/** An RGB color with components in 0..1. Create with {@link rgb} or {@link grayscale}. */
export interface Color {
  readonly red: number;
  readonly green: number;
  readonly blue: number;
}

function clamp01(v: number, name: string): number {
  if (!Number.isFinite(v) || v < 0 || v > 1) {
    throw new RangeError(`${name} must be in 0..1, got ${v}`);
  }
  return v;
}

/** Create an RGB color. Components are in 0..1. */
export function rgb(red: number, green: number, blue: number): Color {
  return {
    red: clamp01(red, "red"),
    green: clamp01(green, "green"),
    blue: clamp01(blue, "blue"),
  };
}

/** Create a gray color; 0 is black, 1 is white. */
export function grayscale(level: number): Color {
  const v = clamp01(level, "level");
  return { red: v, green: v, blue: v };
}
```

- [ ] **Step 3: `src/generate/fonts.ts`:**

```ts
/** The 14 standard PDF fonts available without embedding. Text is limited to the WinAnsi charset. */
export enum StandardFonts {
  Helvetica = "Helvetica",
  HelveticaBold = "Helvetica-Bold",
  HelveticaOblique = "Helvetica-Oblique",
  HelveticaBoldOblique = "Helvetica-BoldOblique",
  Courier = "Courier",
  CourierBold = "Courier-Bold",
  CourierOblique = "Courier-Oblique",
  CourierBoldOblique = "Courier-BoldOblique",
  TimesRoman = "Times-Roman",
  TimesBold = "Times-Bold",
  TimesItalic = "Times-Italic",
  TimesBoldItalic = "Times-BoldItalic",
}
```

(Symbol/ZapfDingbats deliberately omitted: no WinAnsi text semantics; revisit in M24 if requested — note this in a comment.)

- [ ] **Step 4: `src/generate/draw-queue.ts`:**

```ts
import type { Color } from "./color.js";

/** @internal Wire format consumed by the Rust core's apply_draw_ops. */
export type DrawOp = {
  op: "text";
  page: number;
  x: number;
  y: number;
  size: number;
  font: string;
  color: [number, number, number];
  text: string;
  lineHeight?: number;
};

/** @internal */
export class DrawQueue {
  private readonly ops: DrawOp[] = [];

  get length(): number {
    return this.ops.length;
  }

  pushText(
    page: number,
    text: string,
    opts: { x: number; y: number; size: number; font: string; color: Color; lineHeight?: number },
  ): void {
    this.ops.push({
      op: "text",
      page,
      x: opts.x,
      y: opts.y,
      size: opts.size,
      font: opts.font,
      color: [opts.color.red, opts.color.green, opts.color.blue],
      text,
      ...(opts.lineHeight !== undefined ? { lineHeight: opts.lineHeight } : {}),
    });
  }

  toJson(): string {
    return JSON.stringify(this.ops);
  }
}
```

- [ ] **Step 5: `src/generate/page.ts`:**

```ts
import { StandardFonts } from "./fonts.js";
import { rgb, type Color } from "./color.js";
import type { DrawQueue } from "./draw-queue.js";

/** Options for {@link PdfPage.drawText}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawTextOptions {
  x: number;
  y: number;
  /** Font size in points. Must be > 0. */
  size: number;
  /** One of the 14 standard fonts. Defaults to Helvetica. */
  font?: StandardFonts;
  /** Text color. Defaults to black. */
  color?: Color;
  /** Distance between baselines for multiline text ("\n"). Defaults to 1.15 * size. */
  lineHeight?: number;
}

/**
 * A page of a {@link PdfDocument}. Drawing methods queue operations that are
 * applied when the document is saved.
 */
export class PdfPage {
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
  ) {}

  /**
   * Draw text on the page at `(x, y)` (baseline of the first line, origin
   * bottom-left). Standard-14 fonts only; characters outside WinAnsi are
   * rejected at save time by the core.
   */
  drawText(text: string, options: DrawTextOptions): void {
    if (!Number.isFinite(options.size) || options.size <= 0) {
      throw new RangeError(`size must be > 0, got ${options.size}`);
    }
    if (!Number.isFinite(options.x) || !Number.isFinite(options.y)) {
      throw new RangeError(`x and y must be finite numbers`);
    }
    if (options.lineHeight !== undefined && options.lineHeight <= 0) {
      throw new RangeError(`lineHeight must be > 0, got ${options.lineHeight}`);
    }
    this.queue.pushText(this.index, text, {
      x: options.x,
      y: options.y,
      size: options.size,
      font: options.font ?? StandardFonts.Helvetica,
      color: options.color ?? rgb(0, 0, 0),
      lineHeight: options.lineHeight,
    });
  }
}
```

- [ ] **Step 6: Error type.** Add to `src/core/errors.ts` (next to the others, same style):

```ts
/** Thrown when a page index is outside the document's page range. */
export class PageOutOfRangeError extends PdfError {
  constructor(readonly index: number, readonly pageCount: number) {
    super(`page ${index} out of range (document has ${pageCount} pages)`);
  }
}
```

- [ ] **Step 7: Wire into `PdfDocumentBase`** (src/core/document.ts). Add:

```ts
import { PdfPage } from "../generate/page.js";
import { DrawQueue } from "../generate/draw-queue.js";
import { PageOutOfRangeError } from "./errors.js";

// fields:
private pages?: PdfPage[];
private readonly drawQueue = new DrawQueue();

// methods:
/** Number of pages in the document. */
getPageCount(): number {
  return this.loadPages().length;
}

/** All pages, in document order. The same instances are returned every time. */
getPages(): PdfPage[] {
  return [...this.loadPages()];
}

/** Get one page by zero-based index. */
getPage(index: number): PdfPage {
  const pages = this.loadPages();
  const page = pages[index];
  if (page === undefined) throw new PageOutOfRangeError(index, pages.length);
  return page;
}

private loadPages(): PdfPage[] {
  if (!this.pages) {
    let infos: { index: number; width: number; height: number; rotation: number }[];
    try {
      infos = JSON.parse(this.wasm.readPages(this.bytes));
    } catch (e) {
      throw toPdfError(e);
    }
    this.pages = infos.map(
      (p) => new PdfPage(p.index, p.width, p.height, p.rotation, this.drawQueue),
    );
  }
  return this.pages;
}
```

And in `save()`, after the flatten step, before the `bytes === this.bytes` check:

```ts
if (this.drawQueue.length > 0) {
  bytes = this.wasm.applyDrawOps(bytes, this.drawQueue.toJson());
}
```

(inside the same try/catch so core errors become PdfCoreError).

- [ ] **Step 8: Export from root entries.** Add to BOTH `src/index.ts` and `src/index.browser.ts` re-export blocks:

```ts
export { PdfPage } from "./generate/page.js";
export type { DrawTextOptions } from "./generate/page.js";
export { StandardFonts } from "./generate/fonts.js";
export { rgb, grayscale } from "./generate/color.js";
export type { Color } from "./generate/color.js";
export { PageOutOfRangeError } from "./core/errors.js";
```

- [ ] **Step 9:** `bun run typecheck && bun test` — all green (baseline 50 + ~11 new). Iterate.
- [ ] **Step 10: Commit** `feat: page access and drawText on existing PDFs`

---

### Task 6: ./generate subpath + render check

**Files:**
- Create: `src/generate/index.ts`
- Modify: `package.json`, `scripts/render-check.ts`
- Test: `tests/generate-entry.test.ts`

- [ ] **Step 1: Failing test** `tests/generate-entry.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import * as gen from "../src/generate/index.ts";

describe("generate entry", () => {
  test("exports the drawing surface", () => {
    expect(gen.PdfPage).toBeDefined();
    expect(gen.StandardFonts).toBeDefined();
    expect(gen.rgb).toBeDefined();
    expect(gen.grayscale).toBeDefined();
    expect(gen.PageOutOfRangeError).toBeDefined();
  });

  test("runtime-neutral: no PdfDocument or WASM bindings", () => {
    expect("PdfDocument" in gen).toBe(false);
    expect("initializeWasm" in gen).toBe(false);
  });
});
```

- [ ] **Step 2: Barrel** `src/generate/index.ts`:

```ts
// Runtime-neutral subpath entry: drawing types and helpers without
// PdfDocument or any WASM import. PdfDocument comes from the package root
// (or /browser) entry.
export { PdfPage } from "./page.js";
export type { DrawTextOptions } from "./page.js";
export { StandardFonts } from "./fonts.js";
export { rgb, grayscale } from "./color.js";
export type { Color } from "./color.js";
export { PageOutOfRangeError } from "../core/errors.js";
```

- [ ] **Step 3: package.json** exports — add after `"./forms"`:

```json
    "./generate": {
      "types": "./dist/generate/index.d.ts",
      "import": "./dist/generate/index.js"
    },
```

- [ ] **Step 4: Render check.** Extend `scripts/render-check.ts`: after its existing checks, load FICHA, `drawText("RENDER CHECK", {x: 50, y: 50, size: 14})`, save, render page 1 with pdfjs-dist, and assert extracted text contains "RENDER CHECK". Follow the script's existing structure/style for loading pdfjs.
- [ ] **Step 5:** `bun run typecheck && bun test`, then `bun run build:js` + `node --input-type=module -e "import('./dist/generate/index.js').then(m=>console.log(!!m.PdfPage))"` → true. Run `bun run test:render` — expect pass (script needs pdfjs-dist; already a devDep).
- [ ] **Step 6: Commit** `feat: add ./generate subpath export and render check`

---

### Final verification (whole milestone)

- [ ] `bun run typecheck` clean; `bun test` 0 fail; `cargo test` 0 fail.
- [ ] `bun run build:js` then import every exports-map entry (`.`, `./browser`, `./forms`, `./generate`, `./typegen`) — all resolve.
- [ ] `bun run scripts/browser-entry-smoke.ts` passes.
- [ ] `bun run bench` still runs (no API break).
