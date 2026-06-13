# Milestone 21 — Create Documents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** `PdfDocument.create()` + `addPage(size)` to build PDFs from scratch, with `drawText` working on the new pages, via a new stateless `create_document` WASM export that reuses the M20 text-drawing machinery.

**Architecture:** Refactor the per-text-op content-stream emission and font-dict creation out of `apply_draw_ops_json` into shared free functions in `draw.rs`. A new `create.rs` builds a fresh lopdf `Document` skeleton (catalog → pages tree → pages) and uses those shared functions to render queued text. On the TS side `PdfDocumentBase` gains a "create mode": `addPage` queues page sizes, `save()` calls `createDocument` instead of the load-mode fill/flatten/draw path.

**Tech Stack:** Rust (lopdf 0.41), wasm-bindgen, TypeScript ESM, bun test.

**Spec:** `docs/superpowers/specs/2026-06-12-pdf-generation-design.md` (M21 row).

**Environment:** `source "$HOME/.cargo/env"` before cargo/wasm-pack. After Rust changes run `bun run build:wasm` then TS tasks. Baselines after M20 merge: `cargo test --manifest-path crates/core/Cargo.toml` = 50 pass; `bun test` = 59 pass / 4 skip / 0 fail; `bun run typecheck` clean.

**Reuse references:**
- `crates/core/src/draw.rs:145-176` — the BT…ET text block emission (to be extracted).
- `crates/core/src/draw.rs:129-138` — font dict shape (Type1/WinAnsiEncoding).
- `crates/core/src/draw.rs:25-50` — `STANDARD_14`, `fmt_num`.
- `src/core/document.ts` — `PdfDocumentBase`, `CoreWasm`, draw queue wiring, `save()`.
- `src/generate/page.ts`, `draw-queue.ts`, `fonts.ts` — existing generate module.

---

### Task 1: Rust — extract shared text emission in draw.rs

**Files:** Modify `crates/core/src/draw.rs`

Pure refactor, zero behavior change. Extract two free functions and make `STANDARD_14`, `fmt_num`, and the font-index lookup reusable from a sibling module (`pub(crate)`).

- [ ] **Step 1:** Make reusable items `pub(crate)`:
  - `pub(crate) const STANDARD_14`
  - `pub(crate) fn fmt_num`
  - Add `pub(crate) fn standard_14_index(font: &str) -> Option<usize> { STANDARD_14.iter().position(|&f| f == font) }`
  - Add `pub(crate) fn font_dict(base_font: &str) -> lopdf::Dictionary` returning the dict currently built inline at draw.rs:129-134.

- [ ] **Step 2:** Extract the text-block emitter. Add:

```rust
/// Append one self-contained `BT … ET` text block to `out`. `BT` resets the
/// text matrix to identity, so `(x, y)` is an absolute page position.
/// `font_key` is the resource name (without leading slash), e.g. "BPF0".
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_text_block(
    out: &mut Vec<u8>,
    font_key: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 3],
    text: &str,
    line_height: Option<f32>,
) {
    let leading = line_height.unwrap_or(size * 1.15);
    let [r, g, b] = color;
    out.extend_from_slice(b"BT\n");
    out.extend_from_slice(format!("/{font_key} {} Tf\n", fmt_num(size)).as_bytes());
    out.extend_from_slice(
        format!("{} {} {} rg\n", fmt_num(r), fmt_num(g), fmt_num(b)).as_bytes(),
    );
    out.extend_from_slice(format!("{} TL\n", fmt_num(leading)).as_bytes());
    out.extend_from_slice(format!("{} {} Td\n", fmt_num(x), fmt_num(y)).as_bytes());
    for (i, line) in text.split('\n').enumerate() {
        let escaped = escape_pdf_literal(&encode_winansi(line));
        let escaped_str = String::from_utf8_lossy(&escaped).into_owned();
        if i == 0 {
            out.extend_from_slice(format!("({escaped_str}) Tj\n").as_bytes());
        } else {
            out.extend_from_slice(format!("T*\n({escaped_str}) Tj\n").as_bytes());
        }
    }
    out.extend_from_slice(b"ET\n");
}
```

- [ ] **Step 3:** Rewrite the body of `apply_draw_ops_json`'s per-op loop (draw.rs:111-179) to call `emit_text_block(&mut stream_content, &format!("BPF{font_idx}"), *x, *y, *size, *color, text, *line_height)`, and replace the inline font-dict (draw.rs:129-134) with `font_dict(font)`, and the `.position(...)` calls with `standard_14_index(font).unwrap()`. Keep all surrounding logic (validation, grouping, q/Q wrap, font registration) unchanged.

- [ ] **Step 4:** `source "$HOME/.cargo/env" && cargo test --manifest-path crates/core/Cargo.toml` — expect 50 pass, 0 fail (unchanged; the refactor must not alter output).
- [ ] **Step 5:** Commit `refactor(core): extract emit_text_block and font helpers in draw.rs`

---

### Task 2: Rust — create.rs + create_document export

**Files:** Create `crates/core/src/create.rs`; modify `crates/core/src/lib.rs`

**Create-ops JSON contract** (TS produces this in Task 4):

```json
[
  { "op": "addPage", "width": 595.28, "height": 841.89 },
  { "op": "text", "page": 0, "x": 50, "y": 780, "size": 24, "font": "Helvetica", "color": [0,0,0], "text": "Hi", "lineHeight": 28 }
]
```

`addPage` ops define pages in order (first addPage = page 0). `text` ops match the M20 text op shape and reference a page by index.

- [ ] **Step 1: Failing tests** in `create.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;

    #[test]
    fn creates_single_page_doc() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
        ).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        // Has a catalog with Pages
        let cat = doc.catalog().unwrap();
        assert!(cat.has(b"Pages"));
    }

    #[test]
    fn page_has_mediabox() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let mb = page.get(b"MediaBox").unwrap().as_array().unwrap();
        assert_eq!(mb.len(), 4);
        // width 595, height 842
        assert!((mb[2].as_float().unwrap() - 595.0).abs() < 0.5);
        assert!((mb[3].as_float().unwrap() - 842.0).abs() < 0.5);
    }

    #[test]
    fn text_drawn_on_created_page() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#,
        ).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        // Font registered
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let fonts = res.get(b"Font").unwrap().as_dict().unwrap();
        assert!(fonts.iter().any(|(k, _)| k.starts_with(b"BPF")));
        // Content contains the text
        let contents_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(contents_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&stream.content);
        assert!(s.contains("(Hello) Tj"), "content: {s}");
    }

    #[test]
    fn multiple_pages_in_order() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":100,"height":200},{"op":"addPage","width":300,"height":400}]"#,
        ).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let pages: Vec<_> = doc.get_pages().into_iter().collect();
        assert_eq!(pages.len(), 2);
        let p0 = doc.get_dictionary(pages[0].1).unwrap();
        let mb0 = p0.get(b"MediaBox").unwrap().as_array().unwrap();
        assert!((mb0[2].as_float().unwrap() - 100.0).abs() < 0.5);
    }

    #[test]
    fn errors_on_no_pages() {
        // A document with zero pages is invalid; text referencing a page must fail.
        let r = create_document_json(r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#);
        assert!(r.is_err());
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#);
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn output_parses_and_is_nonempty() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#).unwrap();
        assert!(out.starts_with(b"%PDF-"));
        assert!(out.len() > 100);
    }
}
```

- [ ] **Step 2: Implement `create.rs`.** Skeleton:

```rust
//! Build a new PDF document from scratch (pages + text), reusing the text
//! emission helpers from the draw engine.

use lopdf::{dictionary, Document, Object, Stream};
use serde::Deserialize;

use crate::draw::{emit_text_block, font_dict, standard_14_index};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum CreateOp {
    AddPage {
        width: f32,
        height: f32,
    },
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

pub fn create_document_json(ops_json: &str) -> Result<Vec<u8>, String> {
    let ops: Vec<CreateOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid create ops: {e}"))?;

    // Collect page sizes (in order) and validate.
    let pages: Vec<(f32, f32)> = ops
        .iter()
        .filter_map(|o| match o {
            CreateOp::AddPage { width, height } => Some((*width, *height)),
            _ => None,
        })
        .collect();
    if pages.is_empty() {
        return Err("cannot create a document with no pages".to_string());
    }
    // Validate text ops up front.
    for op in &ops {
        if let CreateOp::Text { page, font, .. } = op {
            if *page >= pages.len() {
                return Err(format!("page {page} out of range ({} pages)", pages.len()));
            }
            if standard_14_index(font).is_none() {
                return Err(format!("unknown font: {font}"));
            }
        }
    }

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let mut kids: Vec<Object> = Vec::new();
    for (page_index, (w, h)) in pages.iter().enumerate() {
        // Build content + font set for this page.
        let mut content = Vec::new();
        let mut fonts_used: Vec<usize> = Vec::new();
        for op in &ops {
            if let CreateOp::Text { page, x, y, size, font, color, text, line_height } = op {
                if *page != page_index { continue; }
                let idx = standard_14_index(font).unwrap();
                if !fonts_used.contains(&idx) { fonts_used.push(idx); }
                emit_text_block(&mut content, &format!("BPF{idx}"), *x, *y, *size, *color, text, *line_height);
            }
        }

        // Font resource dictionary.
        let mut font_res = lopdf::Dictionary::new();
        for idx in &fonts_used {
            let fid = doc.add_object(Object::Dictionary(font_dict(super::draw::STANDARD_14[*idx])));
            font_res.set(format!("BPF{idx}"), Object::Reference(fid));
        }
        let resources = dictionary! { "Font" => Object::Dictionary(font_res) };

        let content_id = doc.add_object(Object::Stream(Stream::new(lopdf::Dictionary::new(), content)));
        let page_dict = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), (*w).into(), (*h).into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        kids.push(Object::Reference(page_id));
    }

    let count = kids.len() as i64;
    let pages_dict = dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(count),
    };
    doc.set_object(pages_id, Object::Dictionary(pages_dict));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```

Notes for the implementer:
- Verify the lopdf 0.41 API for: reserving an id (`new_object_id`), `set_object`, `Document::with_version`, `doc.save_to(&mut Vec<u8>)`. Adjust to the actual API used elsewhere in the crate if names differ (check how forms.rs/flatten.rs construct/save). `f32` → `Object` via `.into()` may need `Object::Real(*w)`; use whichever the crate already uses.
- `STANDARD_14` is `pub(crate)` after Task 1; reference it as `crate::draw::STANDARD_14`.
- If `font_dict` is `pub(crate)` it takes `&str`; pass the base font name.

- [ ] **Step 3:** Export in lib.rs (`mod create;` + thin wrapper, doc comment matching neighbors):

```rust
#[wasm_bindgen]
pub fn create_document(ops_json: &str) -> Result<Vec<u8>, JsError> {
    create::create_document_json(ops_json).map_err(|e| JsError::new(&e))
}
```

Also add `create_document_json` to the `fuzz_api` re-export block (lib.rs:29-35).

- [ ] **Step 4:** `cargo test` — expect 57 pass (50 + 7). Iterate.
- [ ] **Step 5:** Add fuzz target `crates/core/fuzz/fuzz_targets/create_document.rs` mirroring `draw_ops.rs` (arbitrary str → `create_document_json`, no panic) and register `[[bin]]` in fuzz/Cargo.toml.
- [ ] **Step 6:** Commit `feat(core): add create_document for building PDFs from scratch`

---

### Task 3: WASM glue

**Files:** Modify `src/core/wasm.ts`, `src/core/wasm-browser.ts`

- [ ] **Step 1:** `source "$HOME/.cargo/env" && bun run build:wasm`.
- [ ] **Step 2:** Add to `src/core/wasm.ts` (import `create_document`, add wrapper):

```ts
export function createDocument(opsJson: string): Uint8Array {
  return create_document(opsJson);
}
```

Add the same to `wasm-browser.ts` with the `ensureInitialized()` guard. Import `create_document` from pkg-web in both.

- [ ] **Step 3:** `bun run typecheck && bun test` — baseline green (59 pass).
- [ ] **Step 4:** Commit `feat: expose create_document through WASM glue`

---

### Task 4: TS — PdfDocument.create + addPage + PageSizes

**Files:** Create `src/generate/page-sizes.ts`; modify `src/core/document.ts`, `src/index.ts`, `src/index.browser.ts`, `src/generate/index.ts`, `src/generate/draw-queue.ts`; Test `tests/create.test.ts`

- [ ] **Step 1: Failing test** `tests/create.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument, PageSizes, StandardFonts, rgb } from "../src/index.ts";

describe("create", () => {
  test("create empty doc with one page", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage(PageSizes.A4);
    expect(page.index).toBe(0);
    expect(Math.round(page.width)).toBe(595);
    expect(Math.round(page.height)).toBe(842);
    expect(doc.getPageCount()).toBe(1);
    const out = await doc.save();
    expect(new TextDecoder("latin1").decode(out).slice(0, 5)).toBe("%PDF-");
  });

  test("default page size is A4", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage();
    expect(Math.round(page.width)).toBe(595);
    expect(Math.round(page.height)).toBe(842);
  });

  test("custom size tuple", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage([200, 300]);
    expect(page.width).toBe(200);
    expect(page.height).toBe(300);
  });

  test("draw text on created page round-trips", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.Letter).drawText("Hello PDF", {
      x: 72, y: 700, size: 18, font: StandardFonts.HelveticaBold, color: rgb(0, 0, 0),
    });
    const out = await doc.save();
    expect(new TextDecoder("latin1").decode(out)).toContain("(Hello PDF) Tj");
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });

  test("multiple pages", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawText("p0", { x: 10, y: 10, size: 10 });
    doc.getPage(1).drawText("p1", { x: 10, y: 10, size: 10 });
    expect(doc.getPageCount()).toBe(2);
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(2);
  });

  test("addPage on a loaded doc throws", async () => {
    const bytes = new Uint8Array(
      await Bun.file("tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf").arrayBuffer(),
    );
    const doc = await PdfDocument.load(bytes);
    expect(() => doc.addPage(PageSizes.A4)).toThrow();
  });

  test("save with no pages throws", async () => {
    const doc = await PdfDocument.create();
    await expect(doc.save()).rejects.toThrow();
  });
});
```

Run, see fail.

- [ ] **Step 2:** `src/generate/page-sizes.ts`:

```ts
/** Common page sizes in PDF points (1 pt = 1/72 inch), as [width, height]. */
export const PageSizes = {
  A3: [841.89, 1190.55],
  A4: [595.28, 841.89],
  A5: [419.53, 595.28],
  Letter: [612, 792],
  Legal: [612, 1008],
  Tabloid: [792, 1224],
} as const satisfies Record<string, readonly [number, number]>;

/** A page size as a [width, height] tuple in PDF points. */
export type PageSize = readonly [number, number];
```

- [ ] **Step 3:** Extend `src/generate/draw-queue.ts` to also carry addPage ops. Add an `AddPageOp` type and a `pushAddPage` method, and make `toJson()` (rename existing usage) emit a combined ordered list for create mode. Concretely add:

```ts
export type AddPageOp = { op: "addPage"; width: number; height: number };

// in DrawQueue:
private readonly pageOps: AddPageOp[] = [];

pushAddPage(width: number, height: number): void {
  this.pageOps.push({ op: "addPage", width, height });
}

/** Ops for create_document: addPage ops first, then all text ops. */
toCreateJson(): string {
  return JSON.stringify([...this.pageOps, ...this.ops]);
}
```

(keep the existing `toJson()` for load-mode draw — it serializes only `this.ops`.)

- [ ] **Step 4:** `src/core/document.ts` changes:
  - Add `createDocument(opsJson: string): Uint8Array;` to the `CoreWasm` interface.
  - Add field `private readonly created: boolean` set from constructor; add a protected constructor param. Simplest: add a second optional flag. Change constructor to:
    ```ts
    protected constructor(
      protected readonly bytes: Uint8Array,
      private readonly wasm: CoreWasm,
      private readonly mode: "load" | "create" = "load",
    ) {}
    ```
  - For create mode the `bytes` arg is an empty `Uint8Array()`.
  - Add `addPage(size: PageSize = PageSizes.A4): PdfPage`:
    ```ts
    addPage(size: PageSize = PageSizes.A4): PdfPage {
      if (this.mode !== "create") {
        throw new PdfError("addPage is only available on documents created with PdfDocument.create()");
      }
      const [width, height] = size;
      const index = this.createdPages.length;
      this.drawQueue.pushAddPage(width, height);
      const page = new PdfPage(index, width, height, 0, this.drawQueue);
      this.createdPages.push(page);
      return page;
    }
    ```
    with a `private readonly createdPages: PdfPage[] = [];` field.
  - `getPageCount`/`getPages`/`getPage`: in create mode operate over `createdPages` (do NOT call `readPages` on empty bytes); in load mode keep current behavior. e.g.:
    ```ts
    getPageCount(): number {
      return this.mode === "create" ? this.createdPages.length : this.loadPages().length;
    }
    ```
    and `getPage`/`getPages` branch likewise (reuse `createdPages` directly).
  - `save()`: branch at the top:
    ```ts
    if (this.mode === "create") {
      try {
        return this.wasm.createDocument(this.drawQueue.toCreateJson());
      } catch (e) {
        throw toPdfError(e);
      }
    }
    ```
    (leave the existing load-mode body unchanged below it.)
  - Import `PageSize`, `PageSizes` from `../generate/page-sizes.js`, and `PdfError` from `./errors.js`.

- [ ] **Step 5:** Add `create()` to BOTH entries. `src/index.ts`:

```ts
static async create(): Promise<PdfDocument> {
  return new PdfDocument(new Uint8Array(), wasm, "create");
}
```

`src/index.browser.ts`:

```ts
static async create(): Promise<PdfDocument> {
  await initializeWasm();
  return new PdfDocument(new Uint8Array(), wasm, "create");
}
```

Add doc comments (the create() one can be shared text). Add re-exports to both entries:

```ts
export { PageSizes } from "./generate/page-sizes.js";
export type { PageSize } from "./generate/page-sizes.js";
```

- [ ] **Step 6:** Add the same two exports to `src/generate/index.ts`.
- [ ] **Step 7:** `bun run typecheck && bun test` — expect 66 pass (59 + 7). Iterate. Then `bun run build:js && bun run scripts/browser-entry-smoke.ts`.
- [ ] **Step 8:** Commit `feat: PdfDocument.create, addPage, and PageSizes`

---

### Final verification (whole milestone)

- [ ] `cargo test` 0 fail; `bun run typecheck` clean; `bun test` 0 fail.
- [ ] `bun run build:js`; import every exports-map entry (`.`, `./browser`, `./forms`, `./generate`, `./typegen`) — all resolve; `PageSizes` present on `.` and `./generate`.
- [ ] `bun run scripts/browser-entry-smoke.ts` passes.
- [ ] Confirm spec M21 row satisfied: create(), addPage(PageSizes), create_document reusing draw machinery.
