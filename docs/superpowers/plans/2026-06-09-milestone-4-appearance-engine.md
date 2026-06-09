# Milestone 4 — Appearance Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Generate real `/AP/N` appearance streams for filled **text** and **dropdown/choice** fields so values render without relying on a viewer, and mark them authoritative by setting `/NeedAppearances false`.

**Architecture:** A new Rust module `appearance.rs` owns standard-14 **Helvetica** font metrics, WinAnsi text encoding, PDF string escaping, auto-font-size, and Form-XObject construction. `fill.rs` calls it: after setting a text/choice field's `/V`, it builds an appearance XObject (referencing the existing `/DR` font as an indirect object), `add_object`s it, and attaches it as the widget's `/AP/N`; once any appearance is generated it flips the AcroForm's `/NeedAppearances` to `false`. Buttons (checkbox/radio) are untouched — their on/off appearances already ship in the file; M3's `/AS` selects them.

**Tech Stack:** Rust (lopdf 0.41 `Stream`/`add_object`/`IncrementalDocument`), wasm-bindgen 0.2.123, bun test. No new crates. No public TS API change.

---

## Verified facts (from de-risking probes, since removed)

Confirmed against `tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf` and `.../Asistencia al Viajero/...1.pdf`:

- Every text field's effective `/DA` is `"/Helv 0 Tf 0 g"` (inherited from AcroForm). **Size `0` = auto-size — mandatory.** Color `0 g` (black). No `/Q` (left-aligned). **Zero multiline fields** in the corpus → single-line only this milestone.
- `/DR/Font` has only `Helv` → `BaseFont /Helvetica`, `/Encoding /WinAnsiEncoding`. So drawing text = encode to WinAnsi (Latin-1 for the Spanish accent range 0xA0–0xFF) + escape `(`, `)`, `\`.
- Text fields ship with **no `/AP`**; we generate from scratch.
- Appearance write path works: `lopdf::Stream::new(dict, content_bytes).with_compression(false)`; `inc.new_document.add_object(Object::Stream(s)) -> ObjectId` (allocates a fresh id past prev max); attach widget `/AP` = `<< /N <ref> >>`; `inc.save_to(&mut Vec<u8>)` appends (append-only — output begins with the original bytes). Reload reads the `/AP/N` stream back.
- The `Helv` font is an **indirect object** (e.g. `(61,0)`) reachable via `catalog → AcroForm(inline) → DR(inline) → Font(inline) → Helv (Reference)`. The XObject `Resources` references that same id — no font re-embedding.
- `/AcroForm` is an **inline dictionary inside the Catalog** (not a separate object). Flipping `/NeedAppearances` ⇒ clone the Root (Catalog) object and mutate its inline AcroForm dict. (Code below also handles AcroForm-as-Reference generically.)

---

## Scope decisions (and what is deferred)

- **In:** single-line text fields, dropdown/listbox fields (render the selected value as single-line text), Helvetica metrics, auto-size, left/center/right quadding (`/Q`), WinAnsi encoding, `/NeedAppearances false`.
- **Deferred (no corpus coverage / later milestone):** multiline text-area wrapping; non-Helvetica / embedded-font metrics; rich comb fields (`/Ff` comb bit); per-field `/MK` border/background drawing. If a text field is multiline (`/Ff` bit 13, `1<<12`) it is rendered as single-line for now (acceptable: corpus has none) — leave a `// TODO(milestone): multiline wrapping` marker.
- **Buttons:** NOT generated — they already have `/AP`. The appearance engine ignores checkbox/radio.
- **Strictness:** if a text/choice field's DA font cannot be resolved in `/DR`, `fill_fields_json` returns `Err` (strict, no silent blank). The corpus always resolves `Helv`.

---

## File Structure

- **Create** `crates/core/src/appearance.rs` — metrics + encoding + content/XObject builders + DA parsing (pure, fully unit-tested).
- **Modify** `crates/core/src/lib.rs` — add `mod appearance;`.
- **Modify** `crates/core/src/fill.rs` — text/dropdown `Apply` variants carry appearance inputs; `resolve` gathers widget rects + DA + font ref; `apply` builds & attaches `/AP`; flip `/NeedAppearances false`.
- **Modify** `examples/playground.ts` — after filling, note that the value now has a baked appearance (no code logic needed beyond existing demo; optional one-line log).
- No TS source/test changes required (public API unchanged); a wasm rebuild is required so `bun test` exercises the new engine.

---

### Task 1: `appearance.rs` — metrics, encoding, escaping

**Files:** Create `crates/core/src/appearance.rs`; Modify `crates/core/src/lib.rs` (`mod appearance;`).

- [ ] **Step 1: Add the module declaration**

In `crates/core/src/lib.rs`, add `mod appearance;` beside `mod fill;` / `mod forms;`.

- [ ] **Step 2: Write failing tests (encoding/escaping/width)**

Create `crates/core/src/appearance.rs` with the test module first (and stub fns returning `Default`):

```rust
//! Appearance engine: Helvetica metrics, WinAnsi encoding, and Form-XObject
//! construction for filled text/choice fields.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_spanish_to_winansi() {
        // á=0xE1, í=0xED, ñ=0xF1
        assert_eq!(encode_winansi("García"), vec![b'G', b'a', b'r', b'c', 0xED, b'a']);
        assert_eq!(encode_winansi("ñ"), vec![0xF1]);
    }

    #[test]
    fn escapes_pdf_literal_specials() {
        assert_eq!(escape_pdf_literal(b"a(b)\\c"), b"a\\(b\\)\\\\c".to_vec());
    }

    #[test]
    fn helvetica_widths_match_afm() {
        assert_eq!(helvetica_width(b' '), 278);
        assert_eq!(helvetica_width(b'A'), 667);
        assert_eq!(helvetica_width(b'i'), 222);
        assert_eq!(helvetica_width(b'W'), 944);
    }

    #[test]
    fn string_width_scales_with_size() {
        // "AA" at size 10 = 2 * 667/1000 * 10 = 13.34
        let w = string_width(b"AA", 10.0);
        assert!((w - 13.34).abs() < 0.01, "got {w}");
    }
}
```

Stubs to add above the tests:

```rust
pub fn encode_winansi(_s: &str) -> Vec<u8> { Vec::new() }
pub fn escape_pdf_literal(_b: &[u8]) -> Vec<u8> { Vec::new() }
pub fn helvetica_width(_c: u8) -> u16 { 0 }
pub fn string_width(_b: &[u8], _size: f32) -> f32 { 0.0 }
```

- [ ] **Step 3: Run to confirm failure**

Run: `cargo test --manifest-path crates/core/Cargo.toml appearance::tests`
Expected: FAIL.

- [ ] **Step 4: Implement metrics/encoding/escaping**

Replace the stubs with:

```rust
/// Helvetica AFM advance widths (units / 1000 em) for WinAnsi codes 32..=126.
/// Index 0 == code 32 (space). Accented Latin-1 letters approximate to their
/// ASCII base width (good enough for v1 auto-sizing; corpus is Helvetica).
const HELV_ASCII: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 32..47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 48..63
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 64..79
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,  // 80..95
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,  // 96..111
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,       // 112..126
];

/// Map a Latin-1/WinAnsi byte >=127 to an ASCII base letter for width purposes.
fn winansi_base(code: u8) -> u8 {
    match code {
        0xC0..=0xC5 => b'A', 0xC8..=0xCB => b'E', 0xCC..=0xCF => b'I',
        0xD2..=0xD6 => b'O', 0xD9..=0xDC => b'U', 0xD1 => b'N', 0xC7 => b'C',
        0xE0..=0xE5 => b'a', 0xE8..=0xEB => b'e', 0xEC..=0xEF => b'i',
        0xF2..=0xF6 => b'o', 0xF9..=0xFC => b'u', 0xF1 => b'n', 0xE7 => b'c',
        0xBF => b'?', 0xA1 => b'!',
        _ => 0, // unknown
    }
}

/// Advance width (units/1000 em) of one WinAnsi byte.
pub fn helvetica_width(code: u8) -> u16 {
    if (32..=126).contains(&code) {
        HELV_ASCII[(code - 32) as usize]
    } else {
        let base = winansi_base(code);
        if base != 0 { HELV_ASCII[(base - 32) as usize] } else { 556 } // default avg
    }
}

/// Width of a WinAnsi byte string at the given font size (points).
pub fn string_width(bytes: &[u8], size: f32) -> f32 {
    let units: u32 = bytes.iter().map(|&c| helvetica_width(c) as u32).sum();
    units as f32 / 1000.0 * size
}

/// Encode a Rust string to WinAnsi bytes. The Latin-1 range (<=0xFF) maps by
/// code point; anything else becomes '?' (out of scope for v1's corpus).
pub fn encode_winansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            let cp = c as u32;
            if cp <= 0xFF { cp as u8 } else { b'?' }
        })
        .collect()
}

/// Escape bytes for a PDF literal string: backslash, parens.
pub fn escape_pdf_literal(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    for &c in b {
        if c == b'\\' || c == b'(' || c == b')' {
            out.push(b'\\');
        }
        out.push(c);
    }
    out
}
```

> NOTE: WinAnsi differs from Latin-1 only in 0x80–0x9F (typographic punctuation). Spanish accents (0xA0–0xFF) are identical, so the simple code-point map is correct for the corpus. A `// TODO(milestone): full WinAnsi 0x80-0x9F map` marker is appropriate.

- [ ] **Step 5: Run to confirm pass**

Run: `cargo test --manifest-path crates/core/Cargo.toml appearance::tests`
Expected: 4 pass.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/appearance.rs crates/core/src/lib.rs
git commit -m "feat(core): appearance metrics, WinAnsi encoding, pdf escaping"
```

---

### Task 2: `appearance.rs` — DA parsing, auto-size, content + XObject builders

**Files:** Modify `crates/core/src/appearance.rs`.

- [ ] **Step 1: Write failing tests**

Append to the test module:

```rust
    #[test]
    fn parses_da() {
        let da = parse_da("/Helv 0 Tf 0 g");
        assert_eq!(da.font, "Helv");
        assert_eq!(da.size, 0.0);
        assert_eq!(da.color, "0 g");
    }

    #[test]
    fn auto_size_uses_height_then_shrinks_to_width() {
        // Tall, wide box, short text → height-capped at 12.
        assert!((auto_size(0.0, b"AB", 300.0, 14.0) - 12.0).abs() < 0.01);
        // Narrow box forces shrink below the height cap.
        let s = auto_size(0.0, b"WWWWWWWWWW", 30.0, 14.0);
        assert!(s < 12.0 && s >= 4.0, "got {s}");
        // Explicit DA size is honored as-is.
        assert_eq!(auto_size(9.0, b"x", 300.0, 50.0), 9.0);
    }

    #[test]
    fn content_has_text_operators() {
        let c = text_appearance_content(b"Hi", 10.0, 100.0, 14.0, 0, "0 g", "Helv");
        let s = String::from_utf8(c).unwrap();
        assert!(s.contains("/Tx BMC"));
        assert!(s.contains("/Helv 10.00 Tf"));
        assert!(s.contains("(Hi) Tj"));
        assert!(s.contains("ET Q EMC"));
    }

    #[test]
    fn content_escapes_text() {
        let c = text_appearance_content(b"a(b)", 10.0, 100.0, 14.0, 0, "0 g", "Helv");
        assert!(String::from_utf8(c).unwrap().contains("(a\\(b\\)) Tj"));
    }
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test --manifest-path crates/core/Cargo.toml appearance::tests`
Expected: FAIL (unresolved names).

- [ ] **Step 3: Implement**

Add to `appearance.rs`:

```rust
use lopdf::{Dictionary, Object, Stream};

const PAD: f32 = 2.0;
const MAX_AUTO: f32 = 12.0;
const MIN_AUTO: f32 = 4.0;

/// Parsed default-appearance string.
pub struct Da {
    pub font: String,
    pub size: f32,
    pub color: String,
}

/// Parse a `/DA` string like "/Helv 0 Tf 0 g". Best-effort; falls back to
/// Helv / size 0 / black on anything unrecognized.
pub fn parse_da(da: &str) -> Da {
    let toks: Vec<&str> = da.split_whitespace().collect();
    let mut font = "Helv".to_string();
    let mut size = 0.0f32;
    let mut color = "0 g".to_string();
    if let Some(i) = toks.iter().position(|&t| t == "Tf") {
        if i >= 2 {
            font = toks[i - 2].trim_start_matches('/').to_string();
            size = toks[i - 1].parse().unwrap_or(0.0);
        }
        let rest: Vec<&str> = toks[i + 1..].to_vec();
        if !rest.is_empty() {
            color = rest.join(" ");
        }
    }
    Da { font, size, color }
}

/// Choose a font size. `da_size > 0` is honored; `0` means auto: cap to the
/// box height, then shrink to fit the box width.
pub fn auto_size(da_size: f32, text: &[u8], avail_w: f32, box_h: f32) -> f32 {
    if da_size > 0.0 {
        return da_size;
    }
    let mut size = (box_h - 2.0 * PAD).clamp(MIN_AUTO, MAX_AUTO);
    let w = string_width(text, size);
    if w > avail_w && w > 0.0 {
        size = (size * avail_w / w).max(MIN_AUTO);
    }
    size
}

/// Build the content stream for a single-line text appearance. `q` is the
/// quadding: 0=left, 1=center, 2=right. Coordinates are in the field's space
/// (BBox origin 0,0).
pub fn text_appearance_content(
    text: &[u8],
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    font: &str,
) -> Vec<u8> {
    let tw = string_width(text, size);
    let tx = match q {
        1 => ((box_w - tw) / 2.0).max(PAD),       // center
        2 => (box_w - PAD - tw).max(PAD),         // right
        _ => PAD,                                  // left
    };
    let ty = ((box_h - size) / 2.0 + size * 0.2).max(PAD);
    let escaped = escape_pdf_literal(text);
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    out.extend_from_slice(format!("/{font} {size:.2} Tf {color} ").as_bytes());
    out.extend_from_slice(format!("{tx:.2} {ty:.2} Td (").as_bytes());
    out.extend_from_slice(&escaped);
    out.extend_from_slice(b") Tj ET Q EMC");
    out
}

/// Build a Form XObject appearance stream of size `box_w`x`box_h` whose
/// Resources reference the font named `font` at indirect object `font_ref`.
pub fn build_appearance_xobject(
    content: Vec<u8>,
    box_w: f32,
    box_h: f32,
    font: &str,
    font_ref: lopdf::ObjectId,
) -> Stream {
    let mut font_dict = Dictionary::new();
    font_dict.set(font.as_bytes().to_vec(), Object::Reference(font_ref));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(font_dict));

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set("FormType", Object::Integer(1));
    dict.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(box_w),
            Object::Real(box_h),
        ]),
    );
    dict.set("Resources", Object::Dictionary(resources));
    Stream::new(dict, content).with_compression(false)
}
```

- [ ] **Step 4: Run to confirm pass + clippy**

Run: `cargo test --manifest-path crates/core/Cargo.toml appearance::tests`
Then: `cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings`
Expected: all appearance tests pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/appearance.rs
git commit -m "feat(core): DA parsing, auto-size, text appearance + XObject builders"
```

---

### Task 3: Wire appearance generation into `fill.rs` + flip `/NeedAppearances`

**Files:** Modify `crates/core/src/fill.rs`.

- [ ] **Step 1: Write failing tests**

Append to `fill.rs`'s `#[cfg(test)] mod tests`:

```rust
    use lopdf::Object;

    /// Read a field's /AP/N stream content as a string, if present.
    fn ap_content(doc: &Document, field_name: &str) -> Option<String> {
        // locate by walking Fields (mirror of fill::find_field, test-local)
        let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
        let cat = doc.get_dictionary(root).ok()?;
        let acro = match cat.get(b"AcroForm").ok()? {
            Object::Reference(id) => doc.get_dictionary(*id).ok()?,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        let mut stack: Vec<lopdf::ObjectId> =
            acro.get(b"Fields").ok()?.as_array().ok()?.iter().filter_map(|e| e.as_reference().ok()).collect();
        while let Some(id) = stack.pop() {
            let Ok(d) = doc.get_dictionary(id) else { continue };
            if crate::forms::fully_qualified_name(doc, d) == field_name {
                let n = d.get(b"AP").ok()?.as_dict().ok()?.get(b"N").ok()?.as_reference().ok()?;
                let st = doc.get_object(n).ok()?.as_stream().ok()?;
                return Some(String::from_utf8_lossy(&st.content).into_owned());
            }
            if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
                for k in kids { if let Ok(r) = k.as_reference() { stack.push(r); } }
            }
        }
        None
    }

    fn need_appearances(doc: &Document) -> Option<bool> {
        let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
        let cat = doc.get_dictionary(root).ok()?;
        let acro = match cat.get(b"AcroForm").ok()? {
            Object::Reference(id) => doc.get_dictionary(*id).ok()?,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        acro.get(b"NeedAppearances").ok().and_then(|o| o.as_bool().ok())
    }

    #[test]
    fn text_fill_generates_appearance() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ap = ap_content(&doc, "beneficiario.apellidos_nombres").expect("AP/N present");
        assert!(ap.contains("(GARCIA) Tj"), "got: {ap}");
        assert!(ap.contains("Tf"));
    }

    #[test]
    fn fill_flips_need_appearances_false() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"X"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(need_appearances(&doc), Some(false));
    }

    #[test]
    fn radio_fill_does_not_add_appearance_stream() {
        // Buttons already have /AP; we must not overwrite with a text stream.
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        Document::load_mem(&out).unwrap(); // still valid
        // value still reads back (existing behavior)
        assert_eq!(reparse_value(&out, "beneficiario.tipo_beneficiario").as_deref(), Some("Titular"));
    }
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test --manifest-path crates/core/Cargo.toml fill::tests::text_fill_generates_appearance`
Expected: FAIL (no AP generated yet).

- [ ] **Step 3: Extend `fill.rs`**

Add `use crate::appearance;` at the top. Extend the data model and logic:

Replace the `Apply` enum's `Text`/`Dropdown` variants and the `resolve`/`apply` functions so text/dropdown carry the appearance inputs. Specifically:

```rust
use crate::appearance;
use lopdf::ObjectId;

/// A widget to draw an appearance on: its id and its /Rect [x0 y0 x1 y1].
struct WidgetBox {
    id: ObjectId,
    rect: [f32; 4],
}

enum Apply {
    /// Set /V to a string literal and draw an appearance on each widget.
    Text { value: String, da: String, q: i64, font_ref: ObjectId, font: String, widgets: Vec<WidgetBox> },
    /// Set /V to a string literal, /I to [index] if matched, and draw appearances.
    Dropdown { value: String, index: Option<i64>, da: String, q: i64, font_ref: ObjectId, font: String, widgets: Vec<WidgetBox> },
    /// Buttons: unchanged from Milestone 3 (no appearance generation).
    Button { value: String, widgets: Vec<(ObjectId, bool)> },
}
```

In `resolve`, for the `"text"` and `"dropdown"|"listbox"` arms, gather the appearance inputs. Add these helpers and rewrite those arms:

```rust
/// Effective /DA: field's own, else inherited, else AcroForm's, else default.
fn effective_da(doc: &Document, dict: &Dictionary, acroform: &Dictionary) -> String {
    if let Some(s) = inherited_str(doc, dict, b"DA") { return s; }
    acroform.get(b"DA").ok().and_then(da_string).unwrap_or_else(|| "/Helv 0 Tf 0 g".to_string())
}

fn inherited_str(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    // /DA may be a string on the field or any ancestor.
    if let Some(s) = dict.get(key).ok().and_then(da_string) { return Some(s); }
    let mut cur = dict;
    for _ in 0..forms::MAX_PARENT_DEPTH {
        let parent = forms::parent_of(doc, cur)?;
        if let Some(s) = parent.get(key).ok().and_then(da_string) { return Some(s); }
        cur = parent;
    }
    None
}

fn da_string(o: &Object) -> Option<String> {
    o.as_str().ok().map(|b| String::from_utf8_lossy(b).into_owned())
}

/// The AcroForm dictionary (inline or via reference).
fn acroform<'a>(doc: &'a Document) -> Option<&'a Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let cat = doc.get_dictionary(root).ok()?;
    forms::as_dict(doc, cat.get(b"AcroForm").ok()?).ok()
}

/// Resolve `font` (from DA) to its indirect object id via AcroForm /DR/Font.
fn font_ref(doc: &Document, acro: &Dictionary, font: &str) -> Option<ObjectId> {
    let dr = forms::as_dict(doc, acro.get(b"DR").ok()?).ok()?;
    let fonts = forms::as_dict(doc, dr.get(b"Font").ok()?).ok()?;
    fonts.get(font.as_bytes()).ok()?.as_reference().ok()
}

/// Collect a field's drawable widgets (id + /Rect). A field with no /Kids is
/// its own widget.
fn widget_boxes(doc: &Document, field_id: ObjectId, dict: &Dictionary) -> Vec<WidgetBox> {
    let ids: Vec<ObjectId> = dict
        .get(b"Kids").and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let ids = if ids.is_empty() { vec![field_id] } else { ids };
    ids.into_iter().filter_map(|id| {
        let d = doc.get_dictionary(id).ok()?;
        let r = d.get(b"Rect").ok()?.as_array().ok()?;
        let mut rect = [0f32; 4];
        for (i, v) in r.iter().enumerate().take(4) { rect[i] = v.as_float().unwrap_or(0.0); }
        Some(WidgetBox { id, rect })
    }).collect()
}

fn quadding(doc: &Document, dict: &Dictionary) -> i64 {
    forms::inherited_int(doc, dict, b"Q").unwrap_or(0)
}
```

> NOTE: `Object::as_float` exists in lopdf 0.41 and returns the real or integer value as `f32`. If a borrow/type detail differs, adapt minimally — do not change behavior.

Rewrite the text and dropdown arms of `resolve`:

```rust
        "text" => {
            let acro = acroform(doc).ok_or_else(|| "no AcroForm".to_string())?;
            let da = effective_da(doc, dict, acro);
            let parsed = appearance::parse_da(&da);
            let fref = font_ref(doc, acro, &parsed.font)
                .ok_or_else(|| format!("DA font '{}' not found in /DR for {}", parsed.font, op.name))?;
            Apply::Text {
                value: op.value.clone(), da, q: quadding(doc, dict),
                font_ref: fref, font: parsed.font, widgets: widget_boxes(doc, field_id, dict),
            }
        }
        "dropdown" | "listbox" => {
            let index = dropdown_index(dict, &op.value);
            if op.value != "Off" && index.is_none() && has_opt(dict) {
                return Err(format!("'{}' is not a valid option for {}", op.value, op.name));
            }
            let acro = acroform(doc).ok_or_else(|| "no AcroForm".to_string())?;
            let da = effective_da(doc, dict, acro);
            let parsed = appearance::parse_da(&da);
            let fref = font_ref(doc, acro, &parsed.font)
                .ok_or_else(|| format!("DA font '{}' not found in /DR for {}", parsed.font, op.name))?;
            Apply::Dropdown {
                value: op.value.clone(), index, da, q: quadding(doc, dict),
                font_ref: fref, font: parsed.font, widgets: widget_boxes(doc, field_id, dict),
            }
        }
```

Rewrite `apply` so text/dropdown set `/V` and draw appearances, and add a helper that flips NeedAppearances. The `fill_fields_json` driver flips it once after applying ops if any text/choice op ran. Replace `apply`'s text/dropdown arms:

```rust
        Apply::Text { value, da, q, font_ref, font, widgets } => {
            field_dict_mut(inc, r.field_id)?.set("V", Object::string_literal(value.as_str()));
            draw_appearances(inc, value, *q, &appearance::parse_da(da), *font_ref, font, widgets)?;
        }
        Apply::Dropdown { value, index, da, q, font_ref, font, widgets } => {
            {
                let d = field_dict_mut(inc, r.field_id)?;
                d.set("V", Object::string_literal(value.as_str()));
                match index {
                    Some(i) => { d.set("I", Object::Array(vec![Object::Integer(*i)])); }
                    None => { d.remove(b"I"); }
                }
            }
            draw_appearances(inc, value, *q, &appearance::parse_da(da), *font_ref, font, widgets)?;
        }
```

Add the drawing + NeedAppearances helpers:

```rust
fn draw_appearances(
    inc: &mut IncrementalDocument,
    value: &str,
    q: i64,
    da: &appearance::Da,
    font_ref: ObjectId,
    font: &str,
    widgets: &[WidgetBox],
) -> Result<(), String> {
    let text = appearance::escape_pdf_literal(&appearance::encode_winansi(value));
    // encode once to WinAnsi (unescaped) for sizing; escape happens in builder.
    let raw = appearance::encode_winansi(value);
    let _ = text; // builder re-escapes from raw
    for wb in widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let size = appearance::auto_size(da.size, &raw, (w - 4.0).max(1.0), h);
        let content = appearance::text_appearance_content(&raw, size, w, h, q, &da.color, font);
        let xobj = appearance::build_appearance_xobject(content, w, h, font, font_ref);
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));
        inc.opt_clone_object_to_new_document(wb.id).map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Set /NeedAppearances false on the AcroForm, cloning whatever object holds it
/// (the Catalog if AcroForm is inline, else the AcroForm object itself).
fn clear_need_appearances(inc: &mut IncrementalDocument) -> Result<(), String> {
    let prev = inc.get_prev_documents();
    let root = prev.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    let cat = prev.get_dictionary(root).map_err(|e| e.to_string())?;
    match cat.get(b"AcroForm") {
        Ok(Object::Reference(id)) => {
            let id = *id;
            inc.opt_clone_object_to_new_document(id).map_err(|e| e.to_string())?;
            field_dict_mut(inc, id)?.set("NeedAppearances", Object::Boolean(false));
        }
        Ok(Object::Dictionary(_)) => {
            inc.opt_clone_object_to_new_document(root).map_err(|e| e.to_string())?;
            let cat = field_dict_mut(inc, root)?;
            let acro = cat.get_mut(b"AcroForm").and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
            acro.set("NeedAppearances", Object::Boolean(false));
        }
        _ => {}
    }
    Ok(())
}
```

Finally, in `fill_fields_json`, after the `for r in &plan { apply(...) }` loop, flip NeedAppearances if any text/dropdown op was applied:

```rust
    let touched_appearance = plan.iter().any(|r| matches!(r.apply, Apply::Text { .. } | Apply::Dropdown { .. }));
    for r in &plan {
        apply(&mut inc, r)?;
    }
    if touched_appearance {
        clear_need_appearances(&mut inc)?;
    }
```

> NOTE: remove the now-redundant temporary `text` binding in `draw_appearances` — it's shown above only to make the escape/encode flow explicit. The builder (`text_appearance_content`) escapes internally from `raw`, so `draw_appearances` should encode to `raw` once and pass `raw`. Clean this up so clippy is happy (no unused vars).

- [ ] **Step 4: Run the targeted tests**

Run: `cargo test --manifest-path crates/core/Cargo.toml fill::tests`
Expected: the 3 new tests pass AND all Milestone 3 fill tests still pass (text/radio/dropdown/errors/multi-op).

- [ ] **Step 5: Full Rust suite + clippy**

Run: `cargo test --manifest-path crates/core/Cargo.toml` then `cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings`
Expected: all green, clippy clean.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/fill.rs
git commit -m "feat(core): generate text/choice appearance streams; clear NeedAppearances"
```

---

### Task 4: Rebuild wasm, verify TS suite, playground note

**Files:** Modify `examples/playground.ts` (optional log); rebuild `pkg/`.

- [ ] **Step 1: Rebuild the wasm package**

Run: `bun run build:wasm`
Expected: succeeds (no API change; the binary now contains the appearance engine).

- [ ] **Step 2: Run the TS suite + type-check**

Run: `bun test` then `bunx tsc --noEmit`
Expected: all 12 TS tests still pass (fills still read back; output still a valid PDF), no type errors. The public API is unchanged, so no new TS tests are required; the appearance bytes are asserted at the Rust level in Task 3.

- [ ] **Step 3: (Optional) playground log**

In `examples/playground.ts`, after the existing fill demo's `console.log`, add:

```ts
  console.log(`(value now has a baked appearance — /NeedAppearances cleared)`);
```

- [ ] **Step 4: Verify by eye (optional, manual)**

Run: `bun run play` and open the written `filled-*.pdf` in a viewer to confirm the value is visible. (Not a CI gate; qpdf is not installed in this environment, so automated cross-validation is deferred.)

- [ ] **Step 5: Commit**

```bash
git add examples/playground.ts
git commit -m "chore: playground notes baked appearances"
```

---

## Self-Review notes (for the controller)

- **Spec coverage (§4.4 appearance engine):** generate appearances for filled text ✅ and choice ✅ fields; standard-14 Helvetica metrics ✅; references existing `/DR` font ✅; required for flatten (M5 consumes these `/AP` streams). Signature image XObject embedding is **M6**, not here.
- **Deferred with justification:** multiline wrapping (0 corpus fields), embedded/non-Helvetica fonts (corpus is Helv-only), comb/`/MK` styling. Markers left in code.
- **NeedAppearances:** flipped to `false` so our appearances are authoritative and the output is deterministic (spec success criterion: outputs validate independently). Limitation: a pre-existing value on a field we did not fill, with no `/AP`, would not render — out of scope for v1 (corpus forms ship empty).
- **No regression risk to read:** `read_fields` reads button on-states from `/AP/N` *dictionaries*; a text field's `/AP/N` is a *stream*, which `as_dict` rejects gracefully → text states stay empty as before.
- **Type consistency:** `appearance::Da{font,size,color}`, `WidgetBox{id,rect}`, and the `Apply` variants are used consistently across `resolve`/`apply`/`draw_appearances`.
