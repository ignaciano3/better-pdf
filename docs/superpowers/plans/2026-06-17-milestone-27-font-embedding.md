# Milestone M27: Custom Font Embedding (TTF/OTF + Subsetting + Unicode) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed TrueType/OpenType fonts as PDF Type0/CIDFontType2 composite fonts with glyph subsetting and a ToUnicode CMap, enabling full Unicode (incl. CJK) text rendering, selectable/searchable text, and small output files — on both loaded (`apply_draw_ops`) and created (`create_document`) PDFs.

**Architecture:** Add a `fonts` module to the Rust core that parses fonts with `ttf-parser`, subsets with `subsetter`, and builds the Type0 object graph. Fonts flow from TS to WASM through a concatenated blob + a `fonts_json` descriptor table (exactly mirroring how images already flow). Text ops gain an optional `fontId` that references an embedded font by index; when set, text is emitted as 2-byte glyph-ID hex strings (`<00A4...> Tj`) instead of WinAnsi literals. The standard-14 path is untouched and remains the default.

**Tech Stack:** Rust 2024, `lopdf` 0.41, `ttf-parser` (font parsing/metrics/cmap), `subsetter` (Typst glyph subsetter — preserves original glyph IDs), `flate2`, `serde`; TypeScript ESM; Bun + cargo test.

## Global Constraints

- Op-queue architecture locked — WASM stateless; font bytes + descriptors serialized on `save()`.
- Draw features work on BOTH loaded and created PDFs — every text change lands in `draw.rs` AND `create.rs`, reusing shared `pub(crate)` helpers.
- Validate ALL ops before mutating — extend both engines' validation passes for `fontId` bounds and decodable font bytes.
- Standard-14 path must not regress — existing `font: String` (WinAnsi/Type1) behavior is the default; embedded fonts are opt-in via `fontId`.
- No size ceiling (user decision) — `ttf-parser` + `subsetter` accepted. Still keep deps wasm-compatible (`default-features = false` where applicable; both are `no_std`-friendly pure Rust).
- `subsetter::subset` **preserves original glyph IDs** — so the PDF uses `Encoding /Identity-H` + `CIDToGIDMap /Identity` and emits original gids. **Verify this with Task 6's test** rather than assuming; if the installed version renumbers, capture its returned gid map and translate.
- Subsetting default ON; `embedFont(bytes, {subset:false})` embeds the full font program.
- Every task ends green: `. ~/.cargo/env && cargo test` and `bun test` (rebuild `pkg-web` before bun tests).

## File Structure

- Create: `crates/core/src/fonts/mod.rs` — public entry `EmbeddedFont` builder: parse, subset, build Type0/CIDFontType2/FontDescriptor/FontFile2 dicts, return `{ font_dict_id, encode(text)->Vec<u8>, glyph_widths }`.
- Create: `crates/core/src/fonts/cmap.rs` — ToUnicode CMap stream builder.
- Modify: `crates/core/Cargo.toml` — add `ttf-parser`, `subsetter`.
- Modify: `crates/core/src/draw.rs` — `Text` op gains `font_id`; new `emit_text_block_cid`; `apply_draw_ops_json` gains `fonts`/`fonts_json` params; build + register embedded fonts.
- Modify: `crates/core/src/create.rs` — `Text` op gains `font_id`; `create_document_json` gains `fonts`/`fonts_json` params; same embed path on the plain `Document`.
- Modify: `crates/core/src/lib.rs` — update `apply_draw_ops`/`create_document` signatures; new `measure_text_embedded`; export in `fuzz_api`.
- Modify: `crates/core/src/appearance.rs` — reuse `encode_winansi`/escaping unchanged; add nothing unless ToUnicode needs escaping helpers.
- Modify (TS): `src/generate/font.ts`, `src/generate/draw-queue.ts`, `src/generate/page.ts`, `src/core/document.ts`, `src/core/wasm.ts`, `src/core/wasm-browser.ts`.
- Tests: Rust `#[cfg(test)]` in `fonts/mod.rs`, `draw.rs`, `create.rs`; TS `test/font-embedding.test.ts`. Fixture: a small open-licensed TTF (see Task 1).

## Interfaces (cross-task contract)

- `fonts/mod.rs`:
  - `pub struct EmbeddedFontInput<'a> { pub data: &'a [u8], pub subset: bool, pub used_chars: std::collections::BTreeSet<char> }`
  - `pub struct BuiltFont { pub gid_for: std::collections::HashMap<char, u16>, pub units_per_em: u16 }`
  - `pub fn build_embedded_font(doc_add: &mut dyn FnMut(lopdf::Object) -> lopdf::ObjectId, input: &EmbeddedFontInput) -> Result<(lopdf::ObjectId, BuiltFont), String>` — adds all font objects via the `doc_add` closure (works for both `Document` and `IncrementalDocument.new_document`), returns the Type0 font dict id + a char→gid map for the content stream.
  - `pub fn measure_embedded(data: &[u8], size: f32, text: &str) -> Result<f32, String>`
- `cmap.rs`: `pub fn to_unicode_cmap(gid_to_unicode: &[(u16, char)]) -> Vec<u8>`
- `draw.rs`: `pub(crate) fn emit_text_block_cid(out: &mut Vec<u8>, font_key: &str, x: f32, y: f32, size: f32, color: [f32;3], gids_per_line: &[Vec<u16>], line_height: Option<f32>)`
- WASM (lib.rs):
  - `apply_draw_ops(data, ops_json, images, fonts, fonts_json) -> Vec<u8>`
  - `create_document(ops_json, images, fonts, fonts_json, fields_json) -> Vec<u8>`
  - `measure_text_embedded(font: &[u8], size: f32, text: &str) -> f32`
- Wire formats:
  - `fonts_json`: `[{"offset":0,"length":40212,"subset":true}, ...]` — indexed by `fontId`.
  - `DrawOp::Text` / `CreateOp::Text` gain `#[serde(default, rename = "fontId")] font_id: Option<usize>`; `font` becomes `#[serde(default)] font: String` (empty when `fontId` set).

---

### Task 1: Add deps + test font fixture

**Files:**
- Modify: `crates/core/Cargo.toml`
- Create: `tests/fixtures/fonts/NotoSans-Regular.subset.ttf` (or a small open font)

- [ ] **Step 1: Add crates to Cargo.toml**

```toml
# under [dependencies]
ttf-parser = { version = "0.25", default-features = false, features = ["std", "opentype-layout", "glyph-names"] }
subsetter = "0.2"
```
> If `subsetter` 0.2 API differs from this plan, fetch current docs (context7 `/org/subsetter` or crates.io) before Task 5. Confirm `ttf-parser` minor version resolves; pin to whatever `cargo add` selects.

- [ ] **Step 2: Acquire a small open-licensed TTF fixture**

Place a permissively-licensed font (OFL) at `tests/fixtures/fonts/`. Preference order:
1. A pre-subset Noto Sans (Latin + a few CJK glyphs) checked into the repo, ≤ ~200KB.
2. If no network, copy a system font: `find /usr/share/fonts -name 'DejaVuSans.ttf'` and use that (DejaVu is permissively licensed). Record the path/source in a sibling `LICENSE.txt`.

Run: `ls -la tests/fixtures/fonts/`
Expected: a `.ttf` present.

- [ ] **Step 3: Verify build still compiles**

Run: `. ~/.cargo/env && cargo build -p better-pdf-core`
Expected: compiles (new deps download + build).

- [ ] **Step 4: Commit**

```bash
git checkout -b m27-font-embedding
git add crates/core/Cargo.toml crates/core/Cargo.lock tests/fixtures/fonts/
git commit -m "build(fonts): add ttf-parser + subsetter deps and test font fixture

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Parse font + measure advance widths

**Files:** Create `crates/core/src/fonts/mod.rs`; register `mod fonts;` in `lib.rs`.

**Interfaces:** Produces `measure_embedded` and an internal `FaceMetrics` used by later tasks.

- [ ] **Step 1: Write failing test**

```rust
// crates/core/src/fonts/mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &[u8] = include_bytes!("../../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");

    #[test]
    fn measures_text_width_positive_and_scales_with_size() {
        let w12 = measure_embedded(FONT, 12.0, "Hello").unwrap();
        let w24 = measure_embedded(FONT, 24.0, "Hello").unwrap();
        assert!(w12 > 0.0);
        assert!((w24 - 2.0 * w12).abs() < 0.01, "width must scale linearly with size");
    }

    #[test]
    fn measure_rejects_garbage_font() {
        assert!(measure_embedded(b"not a font", 12.0, "x").is_err());
    }
}
```

- [ ] **Step 2: Run — expect FAIL (module/function undefined)**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::tests::measures_text_width`
Expected: FAIL (unresolved `measure_embedded`).

- [ ] **Step 3: Implement parse + measure**

```rust
//! Embed TrueType/OpenType fonts as PDF Type0/CIDFontType2 composite fonts.
use ttf_parser::Face;

/// Width in points of `text` rendered in `font` at `size`. Sums horizontal
/// advances of each char's glyph, scaled by size / unitsPerEm. Chars with no
/// glyph contribute the font's default advance (or 0 if none).
pub fn measure_embedded(font: &[u8], size: f32, text: &str) -> Result<f32, String> {
    let face = Face::parse(font, 0).map_err(|e| format!("invalid font: {e}"))?;
    let upem = face.units_per_em() as f32;
    if upem == 0.0 {
        return Err("font has zero unitsPerEm".to_string());
    }
    let mut units = 0u32;
    for ch in text.chars() {
        if let Some(gid) = face.glyph_index(ch) {
            units += face.glyph_hor_advance(gid).unwrap_or(0) as u32;
        }
    }
    Ok(units as f32 * size / upem)
}
```
Add `mod fonts;` to `lib.rs` (private module is fine for now; the wasm fn re-exports).

- [ ] **Step 4: Run — expect PASS**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::tests`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/fonts/mod.rs crates/core/src/lib.rs
git commit -m "feat(fonts): parse fonts and measure advance widths via ttf-parser

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: ToUnicode CMap builder

**Files:** Create `crates/core/src/fonts/cmap.rs`; `mod cmap;` in `fonts/mod.rs`.

- [ ] **Step 1: Write failing test**

```rust
// crates/core/src/fonts/cmap.rs
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cmap_contains_codespace_and_bfchar() {
        let bytes = to_unicode_cmap(&[(3u16, 'A'), (10u16, 'é')]);
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("begincodespacerange"));
        assert!(s.contains("<0000> <FFFF>"));
        assert!(s.contains("beginbfchar"));
        // gid 3 -> U+0041 'A'
        assert!(s.contains("<0003> <0041>"), "cmap was:\n{s}");
        // gid 10 -> U+00E9 'é'
        assert!(s.contains("<000A> <00E9>"), "cmap was:\n{s}");
    }
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::cmap`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
//! Build a ToUnicode CMap stream mapping CID (== glyph id, Identity encoding)
//! to Unicode scalar values, so viewers can copy/search rendered text.

/// `gid_to_unicode`: (glyph id used as 2-byte CID, original char). Returns the
/// decompressed CMap program; the caller wraps it in a stream.
pub fn to_unicode_cmap(gid_to_unicode: &[(u16, char)]) -> Vec<u8> {
    let mut s = String::new();
    s.push_str("/CIDInit /ProcSet findresource begin\n");
    s.push_str("12 dict begin\nbegincmap\n");
    s.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    s.push_str("/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n");
    s.push_str("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    // bfchar in batches of <=100 per the CMap spec.
    for chunk in gid_to_unicode.chunks(100) {
        s.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, ch) in chunk {
            let cp = *ch as u32;
            // Encode Unicode as UTF-16BE hex (handles BMP + supplementary).
            let mut buf = [0u16; 2];
            let utf16 = ch.encode_utf16(&mut buf);
            let hex: String = utf16.iter().map(|u| format!("{u:04X}")).collect();
            let _ = cp;
            s.push_str(&format!("<{gid:04X}> <{hex}>\n"));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}
```

- [ ] **Step 4: Run — expect PASS**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::cmap`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/fonts/cmap.rs crates/core/src/fonts/mod.rs
git commit -m "feat(fonts): ToUnicode CMap builder for embedded fonts

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Build Type0 font object graph (full font, no subset yet)

**Files:** `crates/core/src/fonts/mod.rs`.

**Interfaces:** Produces `build_embedded_font` (Identity-H Type0 + CIDFontType2 + descriptor + FontFile2 + ToUnicode) returning `(font_dict_id, BuiltFont)`. This task embeds the **full** font program (subset wired in Task 5).

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn builds_type0_font_graph() {
    use lopdf::{Document, Object};
    let mut doc = Document::with_version("1.7");
    let mut add = |o: Object| doc.add_object(o);
    let used: std::collections::BTreeSet<char> = "Hé".chars().collect();
    let input = EmbeddedFontInput { data: FONT, subset: false, used_chars: used };
    let (font_id, built) = build_embedded_font(&mut add, &input).unwrap();

    let type0 = doc.get_object(font_id).unwrap().as_dict().unwrap();
    assert_eq!(type0.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
    assert_eq!(type0.get(b"Encoding").unwrap().as_name().unwrap(), b"Identity-H");
    assert!(type0.has(b"ToUnicode"));
    // descendant CIDFontType2 present
    let desc = type0.get(b"DescendantFonts").unwrap().as_array().unwrap();
    let cid_ref = desc[0].as_reference().unwrap();
    let cid = doc.get_object(cid_ref).unwrap().as_dict().unwrap();
    assert_eq!(cid.get(b"Subtype").unwrap().as_name().unwrap(), b"CIDFontType2");
    assert_eq!(cid.get(b"CIDToGIDMap").unwrap().as_name().unwrap(), b"Identity");
    // glyph map covers used chars
    assert!(built.gid_for.contains_key(&'H'));
    assert!(built.gid_for.contains_key(&'é'));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::tests::builds_type0`
Expected: FAIL.

- [ ] **Step 3: Implement `build_embedded_font`**

```rust
use std::collections::{BTreeSet, HashMap};
use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};
use crate::fonts::cmap::to_unicode_cmap;

pub struct EmbeddedFontInput<'a> {
    pub data: &'a [u8],
    pub subset: bool,
    pub used_chars: BTreeSet<char>,
}

pub struct BuiltFont {
    pub gid_for: HashMap<char, u16>,
    pub units_per_em: u16,
}

/// Build the full Type0 object graph and return (Type0 dict id, BuiltFont).
/// `doc_add` adds an object to whichever document we're writing into.
pub fn build_embedded_font(
    doc_add: &mut dyn FnMut(Object) -> ObjectId,
    input: &EmbeddedFontInput,
) -> Result<(ObjectId, BuiltFont), String> {
    let face = Face::parse(input.data, 0).map_err(|e| format!("invalid font: {e}"))?;
    let upem = face.units_per_em();
    if upem == 0 { return Err("font has zero unitsPerEm".into()); }
    let scale = 1000.0 / upem as f32; // PDF glyph space is /1000 em

    // Map used chars -> glyph ids; collect the gid set for subsetting (Task 5).
    let mut gid_for: HashMap<char, u16> = HashMap::new();
    let mut gids: BTreeSet<u16> = BTreeSet::new();
    gids.insert(0); // .notdef
    for &ch in &input.used_chars {
        if let Some(g) = face.glyph_index(ch) {
            gid_for.insert(ch, g.0);
            gids.insert(g.0);
        }
    }

    // Font program (full for now; Task 5 swaps in the subset).
    let program: Vec<u8> = input.data.to_vec();

    // /W width array: [ gid [w] gid2 [w2] ... ] in /1000 em, only for used gids.
    let mut w_array: Vec<Object> = Vec::new();
    for (_, &g) in gid_for.iter() {
        let adv = face.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0);
        w_array.push(Object::Integer(g as i64));
        w_array.push(Object::Array(vec![Object::Real((adv as f32 * scale).round())]));
    }

    // FontFile2 stream with /Length1 = uncompressed program length.
    let len1 = program.len() as i64;
    let mut ff_dict = Dictionary::new();
    ff_dict.set("Length1", Object::Integer(len1));
    let ff_stream = Stream::new(ff_dict, program).with_compression(true);
    let ff_id = doc_add(Object::Stream(ff_stream));

    // Derive a PostScript-safe base name.
    let base = font_postscript_name(&face).unwrap_or_else(|| "Embedded".to_string());
    let base_name = if input.subset { format!("AAAAAA+{base}") } else { base.clone() };

    let bbox = face.global_bounding_box();
    let flags = 4i64; // Symbolic; refine if needed.
    let descriptor = dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(base_name.as_bytes().to_vec()),
        "Flags" => Object::Integer(flags),
        "FontBBox" => Object::Array(vec![
            Object::Real(bbox.x_min as f32 * scale),
            Object::Real(bbox.y_min as f32 * scale),
            Object::Real(bbox.x_max as f32 * scale),
            Object::Real(bbox.y_max as f32 * scale),
        ]),
        "ItalicAngle" => Object::Real(face.italic_angle()),
        "Ascent" => Object::Real(face.ascender() as f32 * scale),
        "Descent" => Object::Real(face.descender() as f32 * scale),
        "CapHeight" => Object::Real(face.capital_height().unwrap_or(face.ascender()) as f32 * scale),
        "StemV" => Object::Integer(80),
        "FontFile2" => Object::Reference(ff_id),
    };
    let descriptor_id = doc_add(Object::Dictionary(descriptor));

    let cid_font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
        "BaseFont" => Object::Name(base_name.as_bytes().to_vec()),
        "CIDSystemInfo" => Object::Dictionary(dictionary!{
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        }),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "CIDToGIDMap" => Object::Name(b"Identity".to_vec()),
        "DW" => Object::Integer(1000),
        "W" => Object::Array(w_array),
    };
    let cid_id = doc_add(Object::Dictionary(cid_font));

    // ToUnicode
    let mut pairs: Vec<(u16, char)> = gid_for.iter().map(|(c, g)| (*g, *c)).collect();
    pairs.sort_by_key(|(g, _)| *g);
    let tu = to_unicode_cmap(&pairs);
    let tu_id = doc_add(Object::Stream(Stream::new(Dictionary::new(), tu).with_compression(true)));

    let type0 = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type0".to_vec()),
        "BaseFont" => Object::Name(base_name.as_bytes().to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(cid_id)]),
        "ToUnicode" => Object::Reference(tu_id),
    };
    let type0_id = doc_add(Object::Dictionary(type0));

    Ok((type0_id, BuiltFont { gid_for, units_per_em: upem }))
}

fn font_postscript_name(face: &Face) -> Option<String> {
    face.names().into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
        .and_then(|n| n.to_string())
}
```
> If `with_compression` is not a `Stream` method in lopdf 0.41, mirror `button_xobject`'s construction in `create.rs` (it uses `.with_compression(false)`), so the method exists. Confirm and use it.

- [ ] **Step 4: Run — expect PASS**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::tests::builds_type0`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/fonts/mod.rs
git commit -m "feat(fonts): build Type0/CIDFontType2 object graph with ToUnicode

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Glyph subsetting

**Files:** `crates/core/src/fonts/mod.rs`.

- [ ] **Step 1: Write failing test (subset smaller, gids preserved)**

```rust
#[test]
fn subsetting_shrinks_and_preserves_gids() {
    // Build with subset=true and assert the embedded FontFile2 is smaller than
    // the original font, and that the glyph for 'H' still resolves in the subset.
    use lopdf::{Document, Object};
    let mut doc = Document::with_version("1.7");
    let mut add = |o: Object| doc.add_object(o);
    let used: std::collections::BTreeSet<char> = "Hé".chars().collect();
    let input = EmbeddedFontInput { data: FONT, subset: true, used_chars: used };
    let (font_id, built) = build_embedded_font(&mut add, &input).unwrap();

    // Walk Type0 -> DescendantFonts -> FontDescriptor -> FontFile2
    let type0 = doc.get_object(font_id).unwrap().as_dict().unwrap();
    let cid = doc.get_object(type0.get(b"DescendantFonts").unwrap().as_array().unwrap()[0].as_reference().unwrap()).unwrap().as_dict().unwrap();
    let fd = doc.get_object(cid.get(b"FontDescriptor").unwrap().as_reference().unwrap()).unwrap().as_dict().unwrap();
    let ff = doc.get_object(fd.get(b"FontFile2").unwrap().as_reference().unwrap()).unwrap().as_stream().unwrap();
    let subset_len: i64 = ff.dict.get(b"Length1").unwrap().as_i64().unwrap();
    assert!((subset_len as usize) < FONT.len(), "subset ({subset_len}) should be < original ({})", FONT.len());

    // The subset font must still parse and contain the gid we recorded for 'H'.
    let raw = ff.decompressed_content().unwrap_or_else(|_| ff.content.clone());
    let face = ttf_parser::Face::parse(&raw, 0).unwrap();
    let h_gid = built.gid_for[&'H'];
    assert!(face.glyph_hor_advance(ttf_parser::GlyphId(h_gid)).is_some(),
        "gid {h_gid} must survive subsetting with the same id");
}
```

- [ ] **Step 2: Run — expect FAIL (full font still embedded, Length1 == original)**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::tests::subsetting_shrinks`
Expected: FAIL.

- [ ] **Step 3: Implement subsetting in `build_embedded_font`**

Replace the `let program = input.data.to_vec();` line with:

```rust
let program: Vec<u8> = if input.subset {
    let glyph_ids: Vec<u16> = gids.iter().copied().collect();
    // subsetter preserves original glyph ids for retained glyphs.
    subsetter::subset(input.data, 0, glyph_ids.iter().copied())
        .map_err(|e| format!("subset failed: {e}"))?
} else {
    input.data.to_vec()
};
```
> Adjust the `subsetter::subset` call to the actual 0.2 signature. As of the planned version it is roughly `subset(data: &[u8], index: u32, glyphs: impl IntoIterator<Item=u16>) -> Result<Vec<u8>, Error>` and retains original gids. **Before coding, confirm the exact signature and the gid-preservation guarantee** (crates.io docs / `cargo doc`). If it returns a remap, store it and translate gids in `gid_for` accordingly (and update the `/W` array + content emission to use remapped gids). The Step-1 test is the gate — make it pass for whatever the real API does.

- [ ] **Step 4: Run — expect PASS**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core fonts::tests`
Expected: PASS (all fonts tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/fonts/mod.rs
git commit -m "feat(fonts): subset embedded fonts to used glyphs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: CID text emission helper

**Files:** `crates/core/src/draw.rs` (add `emit_text_block_cid`, `pub(crate)`).

- [ ] **Step 1: Write failing test**

```rust
// in draw.rs tests module
#[test]
fn cid_text_block_emits_hex_glyph_string() {
    let mut out = Vec::new();
    // two lines, gids per line
    emit_text_block_cid(&mut out, "BPE0", 50.0, 700.0, 12.0, [0.0,0.0,0.0],
        &[vec![0x0048u16, 0x00E9u16], vec![0x0041u16]], None);
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("/BPE0 12 Tf"), "content: {s}");
    assert!(s.contains("<0048 00E9>") || s.contains("<004800E9>"), "hex glyph string missing: {s}");
    assert_eq!(s.matches(" Tj").count(), 2, "one Tj per line: {s}");
    assert!(s.contains("BT") && s.contains("ET"));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core draw::tests::cid_text_block`
Expected: FAIL.

- [ ] **Step 3: Implement (sibling of `emit_text_block`)**

```rust
/// Like `emit_text_block`, but for a Type0/Identity-H font: each line is a list
/// of 2-byte glyph ids, emitted as a hex string `<....>`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_text_block_cid(
    out: &mut Vec<u8>,
    font_key: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 3],
    gids_per_line: &[Vec<u16>],
    line_height: Option<f32>,
) {
    let leading = line_height.unwrap_or(size * 1.15);
    let [r, g, b] = color;
    out.extend_from_slice(b"BT\n");
    out.extend_from_slice(format!("/{font_key} {} Tf\n", fmt_num(size)).as_bytes());
    out.extend_from_slice(format!("{} {} {} rg\n", fmt_num(r), fmt_num(g), fmt_num(b)).as_bytes());
    out.extend_from_slice(format!("{} TL\n", fmt_num(leading)).as_bytes());
    out.extend_from_slice(format!("{} {} Td\n", fmt_num(x), fmt_num(y)).as_bytes());
    for (i, line) in gids_per_line.iter().enumerate() {
        let mut hex = String::with_capacity(line.len() * 4);
        for gid in line { hex.push_str(&format!("{gid:04X}")); }
        if i == 0 {
            out.extend_from_slice(format!("<{hex}> Tj\n").as_bytes());
        } else {
            out.extend_from_slice(format!("T*\n<{hex}> Tj\n").as_bytes());
        }
    }
    out.extend_from_slice(b"ET\n");
}
```

- [ ] **Step 4: Run — expect PASS**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core draw::tests::cid_text_block`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/draw.rs
git commit -m "feat(fonts): CID hex-glyph text emission helper

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Wire embedded fonts into `apply_draw_ops_json` (loaded PDFs)

**Files:** `crates/core/src/draw.rs`, `crates/core/src/lib.rs`.

**Interfaces:** `apply_draw_ops_json(data, ops_json, images, fonts, fonts_json)`. `DrawOp::Text` gains `#[serde(default, rename="fontId")] font_id: Option<usize>`; `font` becomes `#[serde(default)]`.

- [ ] **Step 1: Add a `FontDesc` parse + Text op field**

In `draw.rs`:
```rust
#[derive(Deserialize)]
struct FontDesc { offset: usize, length: usize, #[serde(default = "default_true")] subset: bool }
fn default_true() -> bool { true }
```
Change `DrawOp::Text`: add `#[serde(default, rename = "fontId")] font_id: Option<usize>,` and annotate `font` with `#[serde(default)]`.

- [ ] **Step 2: Write failing integration test**

```rust
#[test]
fn draws_embedded_font_text() {
    const FONT: &[u8] = include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
    let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, FONT.len());
    let ops = r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"fontId":0,"color":[0,0,0],"text":"Héllo"}]"#;
    let out = apply_draw_ops_json(FICHA, ops, &[], FONT, &fonts_json).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, first) = doc.get_pages().into_iter().next().unwrap();
    let res = doc.get_dictionary(first).unwrap();
    // a BPE* (embedded) font key is registered
    // (resolve Resources/Font like the other tests do)
    let s = last_draw_stream_content(&out);
    assert!(s.contains("Tf") && s.contains(" Tj"));
    assert!(s.contains('<') && s.contains('>'), "should emit hex glyph string: {s}");
}
```
Update all existing `apply_draw_ops_json(...)` call sites + the `ops()` test helper to pass the two new args (`&[]`, `"[]"`).

- [ ] **Step 3: Run — expect FAIL (signature/behavior)**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core draw::tests::draws_embedded_font`
Expected: FAIL.

- [ ] **Step 4: Implement**

- Change `pub fn apply_draw_ops_json(data, ops_json, images, fonts, fonts_json)`.
- Parse `fonts_json: Vec<FontDesc>`; validate each `offset+length <= fonts.len()`.
- In the validation pass, for `Text { font, font_id, .. }`: if `font_id` is `Some(i)`, require `i < font_descs.len()`; else require `STANDARD_14.contains(font)`.
- First pass per embedded font id: gather `used_chars` (BTreeSet<char>) across all text ops referencing it.
- Build each embedded font once via `crate::fonts::build_embedded_font`, adding objects into `inc.new_document` (the `doc_add` closure is `|o| inc.new_document.add_object(o)`). Cache `(type0_id, BuiltFont)` by font id. Key = `format!("BPE{font_id}")`.
- In the emit loop, for embedded-font text ops: split `text` on `\n`, map each char→gid via `BuiltFont.gid_for` (skip chars with no glyph, or map to gid 0), call `emit_text_block_cid`.
- Register the Type0 dict in page resources via the existing `register_font` (same `/Font` subdict).
> Borrow note: `build_embedded_font` takes `&mut dyn FnMut(Object)->ObjectId`. To avoid borrow conflicts with `inc`, build all embedded fonts BEFORE the per-page mutation loop and store `(type0_id, BuiltFont)` in a map; the closure borrows `inc.new_document` only during that pre-pass.

- [ ] **Step 5: Update `lib.rs` wasm signature**

```rust
#[wasm_bindgen]
pub fn apply_draw_ops(data: &[u8], ops_json: &str, images: &[u8], fonts: &[u8], fonts_json: &str) -> Result<Vec<u8>, JsError> {
    draw::apply_draw_ops_json(data, ops_json, images, fonts, fonts_json).map_err(|e| JsError::new(&e))
}
```
Update `fuzz_api` re-export + the `fuzz/fuzz_targets/draw_ops.rs` target to pass the new args (`&[]`, `"[]"`).

- [ ] **Step 6: Run — expect PASS, then full crate tests**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core`
Expected: PASS (all, including updated existing draw tests).

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/draw.rs crates/core/src/lib.rs crates/core/fuzz/fuzz_targets/draw_ops.rs
git commit -m "feat(fonts): render embedded-font text in apply_draw_ops

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Wire embedded fonts into `create_document_json` (created PDFs)

**Files:** `crates/core/src/create.rs`, `crates/core/src/lib.rs`.

- [ ] **Step 1: Mirror the Text op change + signature**

`CreateOp::Text` gains `#[serde(default, rename="fontId")] font_id: Option<usize>` and `#[serde(default)] font`. Change `create_document_json(ops_json, images, fonts, fonts_json, fields_json)`.

- [ ] **Step 2: Write failing test**

```rust
#[test]
fn creates_doc_with_embedded_font() {
    const FONT: &[u8] = include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
    let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, FONT.len());
    let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":50,"y":700,"size":24,"fontId":0,"color":[0,0,0],"text":"日本語"}]"#;
    let out = create_document_json(ops, &[], FONT, &fonts_json, "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    let page = doc.get_dictionary(pid).unwrap();
    let res = page.get(b"Resources").unwrap().as_dict().unwrap();
    let fonts = res.get(b"Font").unwrap().as_dict().unwrap();
    let (_, fref) = fonts.iter().find(|(k,_)| k.starts_with(b"BPE")).expect("embedded font key");
    let f = doc.get_object(fref.as_reference().unwrap()).unwrap().as_dict().unwrap();
    assert_eq!(f.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
}
```
Update all existing `create_document_json(...)` call sites + `fuzz_targets/create_document.rs` to pass `&[]`, `"[]"` for the new args.

- [ ] **Step 3: Run — expect FAIL**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core create::tests::creates_doc_with_embedded`
Expected: FAIL.

- [ ] **Step 4: Implement**

Same shape as Task 7 but on the plain `Document`: pre-build embedded fonts (`doc_add = |o| doc.add_object(o)`), gather used chars per font id, register `BPE{id}` into each page's `font_res` when an embedded-font text op lands on that page, and call `emit_text_block_cid`. Validation pass: bounds-check `font_id` and font byte ranges; if `font_id` is None, keep the `standard_14_index` check.

- [ ] **Step 5: Update `lib.rs` create signature + fuzz target**

```rust
#[wasm_bindgen]
pub fn create_document(ops_json: &str, images: &[u8], fonts: &[u8], fonts_json: &str, fields_json: &str) -> Result<Vec<u8>, JsError> {
    create::create_document_json(ops_json, images, fonts, fonts_json, fields_json).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 6: Run — full crate tests**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/create.rs crates/core/src/lib.rs crates/core/fuzz/fuzz_targets/create_document.rs
git commit -m "feat(fonts): render embedded-font text in create_document

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: `measure_text_embedded` WASM export

**Files:** `crates/core/src/lib.rs`.

- [ ] **Step 1: Implement + minimal test**

```rust
/// Width in points of `text` in an embedded font at `size`.
#[wasm_bindgen]
pub fn measure_text_embedded(font: &[u8], size: f32, text: &str) -> Result<f32, JsError> {
    fonts::measure_embedded(font, size, text).map_err(|e| JsError::new(&e))
}
```
(The core logic is already tested in Task 2; this is just the boundary.) Add to `fuzz_api` if useful.

- [ ] **Step 2: Build + commit**

Run: `. ~/.cargo/env && cargo test -p better-pdf-core && bun run build:wasm`
Expected: PASS + `pkg-web` exports `measure_text_embedded`, updated `apply_draw_ops`/`create_document`.
```bash
git add crates/core/src/lib.rs pkg-web
git commit -m "feat(fonts): measure_text_embedded wasm export + rebuild

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: TypeScript API — `embedFont`, font blob channel, `drawText` threading

**Files:** `src/generate/font.ts`, `src/generate/draw-queue.ts`, `src/generate/page.ts`, `src/core/document.ts`, `src/core/wasm.ts`, `src/core/wasm-browser.ts`.

**Interfaces:**
- `doc.embedFont(bytes: Uint8Array, opts?: { subset?: boolean }): Promise<PdfFont>` — registers font, returns a `PdfFont` carrying `_fontId` + bytes.
- `page.drawText(text, opts)` — `opts.font` accepts a `StandardFonts` value OR a `PdfFont` (embedded). Embedded → op carries `fontId`.
- `CoreWasm` gains `measureTextEmbedded(font, size, text)`; `applyDrawOps`/`createDocument` signatures gain `fonts`/`fontsJson`.

- [ ] **Step 1: Write failing TS test**

```ts
// test/font-embedding.test.ts
import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { readFileSync } from "node:fs";

const FONT = readFileSync("tests/fixtures/fonts/NotoSans-Regular.subset.ttf");

test("embed font and draw unicode text on a created page", async () => {
  const doc = await PdfDocument.create();
  const font = await doc.embedFont(FONT);
  const page = doc.addPage();
  page.drawText("Héllo 日本語", { x: 50, y: 700, size: 24, font });
  const bytes = await doc.save();
  expect(bytes.length).toBeGreaterThan(1000);
  // reload + sanity: a Type0 font exists
  const reopened = await PdfDocument.load(bytes);
  expect(reopened.getPageCount()).toBe(1);
});

test("widthOfTextAtSize works for embedded fonts", async () => {
  const doc = await PdfDocument.create();
  const font = await doc.embedFont(FONT);
  const w = font.widthOfTextAtSize("Hello", 12);
  expect(w).toBeGreaterThan(0);
});
```

- [ ] **Step 2: Run — expect FAIL (`embedFont` undefined)**

Run: `bun test test/font-embedding.test.ts`
Expected: FAIL.

- [ ] **Step 3: Extend the font blob channel in `draw-queue.ts`**

Add a font registry parallel to the image blob:
```ts
type FontEntry = { bytes: Uint8Array; subset: boolean };
// in DrawQueue:
private readonly fonts: FontEntry[] = [];
registerFont(bytes: Uint8Array, subset: boolean): number {
  const id = this.fonts.length;
  this.fonts.push({ bytes, subset });
  return id;
}
private buildFonts(): { fonts: Uint8Array; fontsJson: string } {
  const chunks: Uint8Array[] = []; let offset = 0;
  const table = this.fonts.map((f) => {
    const entry = { offset, length: f.bytes.length, subset: f.subset };
    chunks.push(f.bytes); offset += f.bytes.length; return entry;
  });
  const fonts = new Uint8Array(offset); let pos = 0;
  for (const c of chunks) { fonts.set(c, pos); pos += c.length; }
  return { fonts, fontsJson: JSON.stringify(table) };
}
```
Update `pushText` to accept an optional `fontId` and emit it on the op (omit `font` string when `fontId` is set, or send `font: ""`). Update `toDrawPayload`/`toCreatePayload` to also return `{ fonts, fontsJson }`.

- [ ] **Step 4: `PdfFont` embedded variant (`font.ts`)**

Give `PdfFont` an optional `_fontId?: number` and `_bytes?: Uint8Array`, and a constructor path for embedded fonts whose `widthOfTextAtSize` calls `measureTextEmbedded(this._bytes, size, text)`. Keep the standard-font path intact.

- [ ] **Step 5: `embedFont` in `document.ts`**

```ts
async embedFont(bytes: Uint8Array, opts: { subset?: boolean } = {}): Promise<PdfFont> {
  const id = this.drawQueue.registerFont(bytes, opts.subset ?? true);
  return PdfFont.embedded(id, bytes, (b, s, t) => this.wasm.measureTextEmbedded(b, s, t));
}
```
Update `save()` (both create and load branches) to pass `fonts`/`fontsJson` from the queue into `createDocument`/`applyDrawOps`.

- [ ] **Step 6: `drawText` accepts `PdfFont` (`page.ts`)**

When `opts.font` is a `PdfFont` with `_fontId !== undefined`, call `drawQueue.pushText(page, text, { ..., fontId: font._fontId })`; otherwise pass the standard-font base name string as today.

- [ ] **Step 7: Update `CoreWasm` + both wasm wrappers**

Add `measureTextEmbedded(font, size, text): number` and update `applyDrawOps`/`createDocument` signatures in `src/core/document.ts` (`CoreWasm`), `src/core/wasm.ts`, `src/core/wasm-browser.ts` to thread `fonts`/`fontsJson`.

- [ ] **Step 8: Run — expect PASS**

Run: `bun test test/font-embedding.test.ts`
Expected: PASS.

- [ ] **Step 9: Full suite**

Run: `. ~/.cargo/env && cargo test && bun test`
Expected: all green.

- [ ] **Step 10: Commit**

```bash
git add src/ test/font-embedding.test.ts
git commit -m "feat(fonts): embedFont + drawText(font) TS API with Unicode support

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Visual verification + edge cases

**Files:** `test/font-embedding.test.ts` (more cases).

- [ ] **Step 1: Edge-case tests**

Add tests: (a) char with no glyph in the font does not panic (skipped or .notdef); (b) `embedFont(badBytes)` → throws `InvalidImageError`-style error (reuse/extend `toPdfError`); (c) `subset:false` produces a larger file than `subset:true` for the same text; (d) embedded-font text on a **loaded** PDF (`apply_draw_ops` path) renders (mirror the create test against a fixture).

- [ ] **Step 2: Run all edge cases**

Run: `bun test test/font-embedding.test.ts && . ~/.cargo/env && cargo test`
Expected: PASS.

- [ ] **Step 3: Visual check (manual, via verify skill at integration time)**

Render a saved PDF with embedded Unicode text (pdf.js or system viewer) and confirm glyphs display and text is selectable. Document the check in the PR/commit body.

- [ ] **Step 4: Commit**

```bash
git add test/font-embedding.test.ts src/core/errors.ts
git commit -m "test(fonts): edge cases for embedded fonts + error handling

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Docs, skill, version bump

**Files:** `docs/site/src/**` (Starlight pages), `skills/better-pdf/SKILL.md`, `README.md`, `package.json`, `crates/core/Cargo.toml`, `CHANGELOG.md`.

- [ ] **Step 1: Docs site** — add a "Custom fonts" page/section under the generation docs covering `embedFont`, `{subset}`, Unicode, and `drawText({font})`. Include a runnable example. Ensure TypeDoc picks up the new public API (`embedFont`, `PdfFont.embedded`).

- [ ] **Step 2: Skill** — update `skills/better-pdf/SKILL.md` to document embedded fonts in the API surface and add a usage snippet (this is the LLM-facing usage guide).

- [ ] **Step 3: README** — add embedded fonts to the feature list + a short example.

- [ ] **Step 4: Version** — bump `package.json` and `crates/core/Cargo.toml` to `0.4.0`; add a `CHANGELOG.md` entry: "0.4.0 — custom TTF/OTF font embedding with subsetting and Unicode (Type0/CIDFontType2)."

- [ ] **Step 5: Final full build + suite**

Run: `. ~/.cargo/env && cargo test && bun run build:wasm && bun test`
Expected: all green; `pkg-web` current.

- [ ] **Step 6: Commit**

```bash
git add docs/ skills/ README.md package.json crates/core/Cargo.toml CHANGELOG.md
git commit -m "docs(fonts): document font embedding; release 0.4.0

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:** Parse (T2), ToUnicode (T3), Type0 graph (T4), subsetting (T5), CID emission (T6), loaded-PDF path (T7), created-PDF path (T8), measure (T9), TS API (T10), edge cases + visual (T11), docs/skill/version (T12). Covers the M27 acceptance: Unicode renders, selectable/searchable (ToUnicode), subset shrinks output, measure works, both engines, default subset-on with opt-out.

**Placeholder scan:** No TODO/TBD. Two flagged verification points (subsetter exact signature + gid preservation in T5; `with_compression` availability in T4) are explicit "verify against this test" instructions with a fallback, not placeholders — they exist because the external crate API must be confirmed at code time.

**Type consistency:** `build_embedded_font` / `BuiltFont` / `EmbeddedFontInput` / `emit_text_block_cid` / `to_unicode_cmap` / `measure_embedded` used consistently across T2–T9. WASM signatures `apply_draw_ops(data, ops_json, images, fonts, fonts_json)` and `create_document(ops_json, images, fonts, fonts_json, fields_json)` and `measure_text_embedded(font, size, text)` match between lib.rs (T7/T8/T9) and the TS `CoreWasm` (T10). Font resource key prefix `BPE{id}` is distinct from `BPF` (standard-14), `BPI` (image), `BPG` (extgstate). `fontId`/`font` serde defaults applied in both `DrawOp::Text` and `CreateOp::Text`.

**Risk callouts:** (1) subsetter gid behavior is the single biggest external unknown — T5's test gates it. (2) Existing call sites of `apply_draw_ops_json`/`create_document_json` and the two fuzz targets MUST be updated for the new args (T7/T8) or the crate won't compile — listed explicitly. (3) Borrow-checker: build all embedded fonts in a pre-pass before mutating `inc`/page dicts (noted in T7/T8).
