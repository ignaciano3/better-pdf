# Multi-line Text-Field Fill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate wrapped, multi-line `/AP/N` appearance streams when filling existing AcroForm text fields that carry the Multiline flag (Ff bit 13, `1 << 12`), instead of the current single-line rendering.

**Architecture:** A pure `is_multiline(ff)` helper in `forms.rs` and a pure `wrap_lines` word-wrap function in `appearance.rs` (mirroring the TS `wrapText` semantics) feed a new `text_appearance_content_multiline` content-stream builder. `fill.rs` reads the Multiline flag in `resolve`, carries it on `ApInputs`, and branches in `draw_appearances` to call the multiline builder for multiline fields while leaving the single-line path unchanged.

**Tech Stack:** Rust (lopdf), wasm-bindgen, TS API, bun test.

## Global Constraints
- Version bump to 0.16.0 (in `package.json` and `crates/core/Cargo.toml`).
- Only `crates/core/src/fill.rs`, `crates/core/src/forms.rs`, `crates/core/src/appearance.rs`, their tests, and docs (README / limitations / CHANGELOG) change.
- Build the wasm package (`bun run build`) before running any TS tests.
- Run `source ~/.cargo/env` before every `cargo` invocation.

---

## Task 1: `is_multiline` flag helper + `wrap_lines` word-wrap (pure, unit-tested)

**Files:**
- `crates/core/src/forms.rs`
- `crates/core/src/appearance.rs`

**Interfaces:**
- Produces `pub(crate) fn is_multiline(ff: i64) -> bool` in `forms.rs` — true when Ff bit 13 (`1 << 12`) is set.
- Produces `pub fn wrap_lines(text: &[u8], size: f32, avail_w: f32, widths: &FontWidths) -> Vec<Vec<u8>>` in `appearance.rs` — splits on hard breaks (`\n`, normalizing `\r\n` and lone `\r`) then greedy word-wraps each paragraph by spaces so each line's `string_width <= avail_w`; a single word wider than `avail_w` is emitted on its own line (overflow, no mid-word break). An empty paragraph yields one empty line so blank lines are preserved.
- Consumes `string_width(bytes, size, &FontWidths)` (existing, line 67).

### Steps

- [ ] **1.1 Write the failing test for `is_multiline`.** Add this test to the `#[cfg(test)] mod tests` block in `crates/core/src/forms.rs` (create the block if it does not yet exist; if it exists, add the test and the `use super::is_multiline;` import alongside the other imports):

```rust
#[test]
fn is_multiline_reads_bit_13() {
    assert!(super::is_multiline(1 << 12));
    assert!(super::is_multiline((1 << 12) | (1 << 1)));
    assert!(!super::is_multiline(0));
    assert!(!super::is_multiline(1 << 11));
    assert!(!super::is_multiline(1 << 13));
}
```

- [ ] **1.2 Run it, expect FAIL.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml is_multiline_reads_bit_13`. Expect a compile error: `cannot find function 'is_multiline' in module 'super'` (or `in this scope`).

- [ ] **1.3 Implement `is_multiline`.** Add this immediately after `classify` (which ends at line 156) in `crates/core/src/forms.rs`:

```rust
/// True when a text field's Ff carries the Multiline flag (bit 13, `1 << 12`),
/// i.e. it is a text-area field that should render wrapped, multi-line text.
pub(crate) fn is_multiline(ff: i64) -> bool {
    ff & (1 << 12) != 0
}
```

- [ ] **1.4 Run it, expect PASS.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml is_multiline_reads_bit_13`. Expect 1 passed.

- [ ] **1.5 Write the failing tests for `wrap_lines`.** Add these tests inside the `#[cfg(test)] mod tests` block in `crates/core/src/appearance.rs` (it starts at line 750 and already has `use super::*;`):

```rust
#[test]
fn wrap_lines_preserves_hard_breaks() {
    // Wide box so no soft wrapping happens; only the explicit \n splits.
    let lines = wrap_lines(b"alpha\nbeta", 10.0, 1000.0, &helvetica_widths());
    assert_eq!(lines, vec![b"alpha".to_vec(), b"beta".to_vec()]);
}

#[test]
fn wrap_lines_normalizes_crlf() {
    let lines = wrap_lines(b"alpha\r\nbeta\rgamma", 10.0, 1000.0, &helvetica_widths());
    assert_eq!(
        lines,
        vec![b"alpha".to_vec(), b"beta".to_vec(), b"gamma".to_vec()]
    );
}

#[test]
fn wrap_lines_greedy_word_wrap() {
    // "aaaa" at size 10 is 4 * 0.556 * 10 = 22.24pt wide; a ~30pt box fits one
    // word per line (two words + a space exceed 30pt), forcing a wrap.
    let lines = wrap_lines(b"aaaa aaaa", 10.0, 30.0, &helvetica_widths());
    assert_eq!(lines, vec![b"aaaa".to_vec(), b"aaaa".to_vec()]);
}

#[test]
fn wrap_lines_long_word_overflows_on_own_line() {
    // A single word wider than avail_w gets its own line; the short word wraps after.
    let lines = wrap_lines(b"wwwwwwwwww hi", 10.0, 30.0, &helvetica_widths());
    assert_eq!(lines, vec![b"wwwwwwwwww".to_vec(), b"hi".to_vec()]);
}

#[test]
fn wrap_lines_empty_string_is_single_empty_line() {
    assert_eq!(wrap_lines(b"", 10.0, 100.0, &helvetica_widths()), vec![Vec::<u8>::new()]);
}

#[test]
fn wrap_lines_blank_paragraph_preserved() {
    let lines = wrap_lines(b"a\n\nb", 10.0, 1000.0, &helvetica_widths());
    assert_eq!(lines, vec![b"a".to_vec(), Vec::<u8>::new(), b"b".to_vec()]);
}
```

- [ ] **1.6 Run it, expect FAIL.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml wrap_lines`. Expect a compile error: `cannot find function 'wrap_lines' in this scope`.

- [ ] **1.7 Implement `wrap_lines`.** Add this in `crates/core/src/appearance.rs` immediately after `string_width` (which ends at line 70):

```rust
/// Word-wrap WinAnsi `text` so each returned line's `string_width <= avail_w`.
/// Hard breaks (`\n`, with `\r\n` and lone `\r` normalized to `\n`) split first;
/// each resulting paragraph is greedily wrapped on ASCII spaces. A single word
/// wider than `avail_w` is placed on its own line (overflow, no mid-word break).
/// A blank paragraph yields one empty line, so blank lines survive. Mirrors the
/// TypeScript `wrapText` in `src/generate/wrap-text.ts`.
pub fn wrap_lines(text: &[u8], size: f32, avail_w: f32, widths: &FontWidths) -> Vec<Vec<u8>> {
    // Normalize CRLF / lone CR to LF, then split on LF into paragraphs.
    let mut normalized: Vec<u8> = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\r' {
            normalized.push(b'\n');
            if i + 1 < text.len() && text[i + 1] == b'\n' {
                i += 1;
            }
        } else {
            normalized.push(text[i]);
        }
        i += 1;
    }

    let mut out: Vec<Vec<u8>> = Vec::new();
    for para in normalized.split(|&b| b == b'\n') {
        let words: Vec<&[u8]> = para.split(|&b| b == b' ').filter(|w| !w.is_empty()).collect();
        if words.is_empty() {
            out.push(Vec::new());
            continue;
        }
        let mut current: Vec<u8> = Vec::new();
        for word in words {
            if current.is_empty() {
                current.extend_from_slice(word);
                continue;
            }
            let mut candidate = current.clone();
            candidate.push(b' ');
            candidate.extend_from_slice(word);
            if string_width(&candidate, size, widths) <= avail_w {
                current = candidate;
            } else {
                out.push(std::mem::take(&mut current));
                current.extend_from_slice(word);
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}
```

- [ ] **1.8 Run it, expect PASS.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml wrap_lines`. Expect 6 passed.

- [ ] **1.9 Clippy clean.** Run `source ~/.cargo/env && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`. Expect no warnings.

- [ ] **1.10 Commit.**

```bash
git add crates/core/src/forms.rs crates/core/src/appearance.rs
git commit -m "feat(forms,appearance): add is_multiline flag + wrap_lines word-wrap

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `text_appearance_content_multiline` content-stream builder (unit-tested)

**Files:**
- `crates/core/src/appearance.rs`

**Interfaces:**
- Produces `pub fn text_appearance_content_multiline(lines: &[Vec<u8>], size: f32, box_w: f32, box_h: f32, q: i64, color: &str, font: &str, widths: &FontWidths) -> Vec<u8>` — emits `/Tx BMC q BT /<font> <size> Tf <color>`, sets leading via `<leading> TL` (leading = `size * 1.15`), positions the first baseline near the top of the box (`ty = box_h - PAD - size`), then for each line applies horizontal quadding offset (`q`: 0=left, 1=center, 2=right) using `string_width` and emits `<tx> <ty> Td (line) Tj`, advancing `ty` by the leading per line. Ends with `ET Q EMC`.
- Consumes `string_width` (line 67), `escape_pdf_literal` (line 98), `PAD` const (line 109).

### Steps

- [ ] **2.1 Write the failing tests.** Add these tests inside the `#[cfg(test)] mod tests` block in `crates/core/src/appearance.rs` (after the Task-1 `wrap_lines` tests):

```rust
#[test]
fn multiline_content_emits_multiple_tj_and_tl() {
    let lines = vec![b"hello".to_vec(), b"world".to_vec()];
    let c = text_appearance_content_multiline(
        &lines,
        10.0,
        100.0,
        40.0,
        0,
        "0 g",
        "Helv",
        &helvetica_widths(),
    );
    let s = String::from_utf8(c).unwrap();
    assert!(s.contains("/Tx BMC"));
    assert!(s.contains("/Helv 10.00 Tf"));
    assert!(s.contains("TL"), "missing leading operator: {s}");
    assert_eq!(s.matches(" Tj").count(), 2, "expected two Tj: {s}");
    assert!(s.contains("(hello) Tj"));
    assert!(s.contains("(world) Tj"));
    assert!(s.ends_with("ET Q EMC"));
}

#[test]
fn multiline_content_escapes_text() {
    let lines = vec![b"a(b)".to_vec()];
    let c = text_appearance_content_multiline(
        &lines,
        10.0,
        100.0,
        40.0,
        0,
        "0 g",
        "Helv",
        &helvetica_widths(),
    );
    assert!(String::from_utf8(c).unwrap().contains("(a\\(b\\)) Tj"));
}

#[test]
fn multiline_content_quads_right() {
    // Right-quad: each line's tx = box_w - PAD - line_width, so a short line
    // sits well to the right (tx well above the left PAD of 2.0).
    let lines = vec![b"hi".to_vec()];
    let c = text_appearance_content_multiline(
        &lines,
        10.0,
        200.0,
        40.0,
        2,
        "0 g",
        "Helv",
        &helvetica_widths(),
    );
    let s = String::from_utf8(c).unwrap();
    // "hi" width = (222 + 556)/1000 * 10 = 7.78; tx = 200 - 2 - 7.78 = 190.22.
    assert!(s.contains("190.22"), "expected right-quad tx 190.22 in: {s}");
}
```

- [ ] **2.2 Run it, expect FAIL.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml multiline_content`. Expect a compile error: `cannot find function 'text_appearance_content_multiline' in this scope`.

- [ ] **2.3 Implement `text_appearance_content_multiline`.** Add this in `crates/core/src/appearance.rs` immediately after `text_appearance_content` (which ends at line 184):

```rust
/// Build the content stream for a wrapped, multi-line text appearance. `lines`
/// are pre-wrapped WinAnsi byte strings (see `wrap_lines`). `q` is the quadding
/// applied per line: 0=left, 1=center, 2=right. Text is top-aligned: the first
/// baseline sits near the top of the box (Acrobat-like), and successive lines
/// step down by the leading (`size * 1.15`). Coordinates are in the field's
/// space (BBox origin 0,0). Each line uses an absolute `Td` so its horizontal
/// quad offset is independent of the previous line.
#[allow(clippy::too_many_arguments)]
pub fn text_appearance_content_multiline(
    lines: &[Vec<u8>],
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    font: &str,
    widths: &FontWidths,
) -> Vec<u8> {
    let leading = size * 1.15;
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    out.extend_from_slice(format!("/{font} {size:.2} Tf {color} ").as_bytes());
    out.extend_from_slice(format!("{leading:.2} TL ").as_bytes());

    // First baseline near the top of the box; step down by the leading per line.
    let mut ty = box_h - PAD - size;
    for line in lines {
        let tw = string_width(line, size, widths);
        let tx = match q {
            1 => ((box_w - tw) / 2.0).max(PAD), // center
            2 => (box_w - PAD - tw).max(PAD),   // right
            _ => PAD,                           // left
        };
        let escaped = escape_pdf_literal(line);
        out.extend_from_slice(format!("{tx:.2} {ty:.2} Td (").as_bytes());
        out.extend_from_slice(&escaped);
        out.extend_from_slice(b") Tj ");
        // Reset the text matrix before the next absolute Td (Td is relative to
        // the line matrix, so undo this line's horizontal offset by moving back).
        out.extend_from_slice(format!("{:.2} 0 Td ", -tx).as_bytes());
        ty -= leading;
    }
    out.extend_from_slice(b"ET Q EMC");
    out
}
```

> Note: each line uses `tx ty Td` then `-tx 0 Td` so the running text matrix keeps `tx`-independence; the next line's `tx ty Td` is relative to the column-zero position. `ty` here is the absolute target baseline minus the accumulated offset — confirm with the test below; if the running-matrix bookkeeping proves fragile, the equivalent and simpler form is to emit a leading `1 0 0 1 0 <first_ty> Tm` and then per line `Tm`-reset; keep whichever the tests verify. The implementation above is what the Step 2.1 tests assert.

- [ ] **2.4 Run it, expect PASS.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml multiline_content`. Expect 3 passed. If `multiline_content_quads_right` fails because the running-matrix offset shifts `tx`, adjust the implementation so each line's emitted `tx` equals the absolute quad offset (the test asserts the literal `190.22`); the `-tx 0 Td` reset above keeps each line's `tx` absolute relative to column zero, so this should hold.

- [ ] **2.5 Clippy clean.** Run `source ~/.cargo/env && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`. Expect no warnings.

- [ ] **2.6 Commit.**

```bash
git add crates/core/src/appearance.rs
git commit -m "feat(appearance): add text_appearance_content_multiline builder

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Wire multiline into `fill.rs` (`ApInputs` / `resolve` / `draw_appearances`) with integration test

**Files:**
- `crates/core/src/fill.rs`

**Interfaces:**
- Modifies `struct ApInputs` (line 69) to add a `multiline: bool` field.
- Modifies `ap_inputs` (line 170) to accept `ff: i64` and set `multiline: forms::is_multiline(ff)`.
- Modifies `resolve` (line 100) to pass the already-read `ff` (line 104) into both `ap_inputs` call sites (`text`, line 142; and `dropdown`/`listbox`, line 159).
- Modifies `draw_appearances` (line 449) to branch on `ap.multiline`: when true, compute `avail_w = (w - 2.0 * PAD).max(1.0)`, choose a multiline size, call `appearance::wrap_lines` then `appearance::text_appearance_content_multiline`; otherwise keep the existing single-line path.
- Consumes `forms::is_multiline` (Task 1), `appearance::wrap_lines` + `appearance::text_appearance_content_multiline` (Tasks 1-2).

> Quadding only applies to text fields. Choice fields (dropdown/listbox) are never multiline, so `is_multiline(ff)` is naturally false for them — passing `ff` to `ap_inputs` for the choice path is correct and harmless.

### Steps

- [ ] **3.1 Write the failing integration test.** Add this test to the `#[cfg(test)] mod tests` block in `crates/core/src/fill.rs` (it starts at line 549; reuse the existing `FICHA` fixture, the `ap_content` helper at line 657, and the existing `use` imports). The test sets the Multiline flag on a real `FICHA` text field by editing its dictionary in-memory, re-serializes, then fills it with a value that must wrap in the field's rect width, and asserts the resulting `/AP/N` stream contains more than one `Tj`:

```rust
/// Set the Multiline flag (Ff bit 13) on a text field and return the bytes of
/// the modified document, so we can exercise the multiline fill path on a real
/// fixture field even though the corpus ships only single-line text fields.
fn with_multiline_flag(bytes: &[u8], field_name: &str) -> Vec<u8> {
    let mut doc = Document::load_mem(bytes).unwrap();
    let (id, _) = find_field(&doc, field_name).unwrap();
    let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
    d.set("Ff", Object::Integer(1 << 12));
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn multiline_text_fill_wraps_into_multiple_lines() {
    // Confirm the target field is wide-but-short enough to force a wrap. The
    // value is long with spaces so greedy wrapping must break it across lines.
    let base = with_multiline_flag(FICHA, "beneficiario.apellidos_nombres");
    let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"the quick brown fox jumps over the lazy dog several times to overflow"}]"#;
    let out = fill_fields_json(&base, ops, &[]).unwrap();
    Document::load_mem(&out).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let ap = ap_content(&doc, "beneficiario.apellidos_nombres").expect("AP/N present");
    assert!(ap.contains("TL"), "multiline AP should set leading: {ap}");
    assert!(
        ap.matches(" Tj").count() >= 2,
        "expected multiple Tj (wrapped lines), got: {ap}"
    );
}
```

- [ ] **3.2 Run it, expect FAIL.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml multiline_text_fill_wraps_into_multiple_lines`. Expect FAIL: only one ` Tj` (the single-line path), so the `>= 2` assertion fails with a message like `expected multiple Tj (wrapped lines)`.

> If this test instead fails because the chosen field's rect is too wide for the value to wrap, switch the value to a longer string or pick a narrower text field; verify by reading the field's `/Rect` width. The assertion target is "more than one wrapped line."

- [ ] **3.3 Add the `multiline` field to `ApInputs`.** In `crates/core/src/fill.rs`, change the struct at line 69:

```rust
/// Per-field appearance inputs shared by text and choice fields.
struct ApInputs {
    da: appearance::Da,
    q: i64,
    font_ref: ObjectId,
    font: String,
    widths: appearance::FontWidths,
    widgets: Vec<WidgetBox>,
    /// True for text-area fields (Ff Multiline bit); choice fields are always false.
    multiline: bool,
}
```

- [ ] **3.4 Thread `ff` through `ap_inputs`.** Change the `ap_inputs` signature and body (line 170):

```rust
fn ap_inputs(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
    name: &str,
    ff: i64,
) -> Result<ApInputs, String> {
    let acro = forms::acroform(doc).ok_or_else(|| "no AcroForm".to_string())?;
    let da_str = effective_da(doc, dict, acro);
    let da = appearance::parse_da(&da_str);
    let font_ref = font_ref(doc, acro, &da.font)
        .ok_or_else(|| format!("DA font '{}' not found in /DR for {}", da.font, name))?;
    Ok(ApInputs {
        q: quadding(doc, dict),
        font: da.font.clone(),
        widths: resolve_widths(doc, acro, &da.font),
        da,
        font_ref,
        widgets: widget_boxes(doc, field_id, dict),
        multiline: forms::is_multiline(ff),
    })
}
```

- [ ] **3.5 Pass `ff` at both call sites in `resolve`.** In `crates/core/src/fill.rs`, update the `text` arm (line 142) and the `dropdown`/`listbox` arm (line 159). `ff` is already in scope from line 104.

Text arm:

```rust
            "text" => Apply::Text {
                value: value.clone(),
                ap: ap_inputs(doc, field_id, dict, &op.name, ff)?,
            },
```

Dropdown/listbox arm:

```rust
                Apply::Dropdown {
                    value: value.clone(),
                    index,
                    ap: ap_inputs(doc, field_id, dict, &op.name, ff)?,
                }
```

- [ ] **3.6 Branch in `draw_appearances`.** Replace the body of `draw_appearances` (lines 449-480) with the branching version:

```rust
fn draw_appearances(
    inc: &mut IncrementalDocument,
    value: &str,
    ap: &ApInputs,
) -> Result<(), String> {
    let text = appearance::encode_winansi(value);
    for wb in &ap.widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let content = if ap.multiline {
            // Multiline: do not shrink-to-fit width (we wrap instead). Honor an
            // explicit DA size; for auto (size 0) use a fixed, height-clamped
            // default so wrapping has a stable measure.
            let size = if ap.da.size > 0.0 {
                ap.da.size
            } else {
                (h - 2.0).clamp(appearance::MIN_AUTO, appearance::MAX_AUTO)
            };
            let avail_w = (w - 4.0).max(1.0);
            let lines = appearance::wrap_lines(&text, size, avail_w, &ap.widths);
            appearance::text_appearance_content_multiline(
                &lines, size, w, h, ap.q, &ap.da.color, &ap.font, &ap.widths,
            )
        } else {
            let size = appearance::auto_size(ap.da.size, &text, (w - 4.0).max(1.0), h, &ap.widths);
            appearance::text_appearance_content(
                &text, size, w, h, ap.q, &ap.da.color, &ap.font, &ap.widths,
            )
        };
        let xobj = appearance::build_appearance_xobject(content, w, h, &ap.font, ap.font_ref);
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}
```

- [ ] **3.7 Expose `MIN_AUTO` / `MAX_AUTO` to `fill.rs`.** The branch in 3.6 references `appearance::MIN_AUTO` and `appearance::MAX_AUTO`, which are currently private (`const`, lines 110-111). Change their visibility in `crates/core/src/appearance.rs`:

```rust
pub(crate) const PAD: f32 = 2.0;
pub(crate) const MAX_AUTO: f32 = 12.0;
pub(crate) const MIN_AUTO: f32 = 4.0;
```

(`PAD` is promoted too for consistency; it is used inside `appearance.rs` already.)

- [ ] **3.8 Run the new test, expect PASS.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml multiline_text_fill_wraps_into_multiple_lines`. Expect 1 passed.

- [ ] **3.9 Run the full core test suite, expect PASS.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`. Expect all tests passing (the existing single-line `text_fill_generates_appearance` still passes since the non-multiline branch is unchanged).

- [ ] **3.10 Clippy clean.** Run `source ~/.cargo/env && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`. Expect no warnings.

- [ ] **3.11 Commit.**

```bash
git add crates/core/src/fill.rs crates/core/src/appearance.rs
git commit -m "feat(fill): generate wrapped appearances for multiline text fields

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Docs, CHANGELOG, and version bump to 0.16.0

**Files:**
- `README.md`
- `docs/site/src/content/docs/reference/limitations.md`
- `CHANGELOG.md`
- `package.json`
- `crates/core/Cargo.toml`

**Interfaces:** None (documentation + version metadata only).

### Steps

- [ ] **4.1 Update the README limitations list.** In `README.md`, replace the single-line limitation bullet (lines 741-743):

Old:
```
- Form text fields are filled single-line; multi-line (text-area) field
  appearances are not yet wrapped. (`drawText` itself honors `\n` and the
  `maxWidth` word-wrap option.)
```

New:
```
- Form text fields with the Multiline flag are filled with wrapped, top-aligned
  multi-line appearances (honoring `\n` hard breaks and per-line quadding);
  single-line fields are filled single-line. Mid-word breaking is not performed
  — a word wider than the field overflows onto its own line.
```

- [ ] **4.2 Update the docs-site limitations page.** In `docs/site/src/content/docs/reference/limitations.md`, remove the now-resolved bullet at line 14:

```
- Text fields are single-line; multi-line wrapping is not yet generated.
```

Then extend the existing **Multi-line text** bullet (lines 24-27) so it covers form fills too. Replace:

```
- **Multi-line text:** `drawText` honors `\n` as hard line breaks, and the
  `maxWidth` option word-wraps text to fit a given width (added in 0.14.0). A
  single word wider than `maxWidth` overflows onto its own line; mid-word
  breaking and text alignment are not yet supported.
```

With:

```
- **Multi-line text:** `drawText` honors `\n` as hard line breaks, and the
  `maxWidth` option word-wraps text to fit a given width (added in 0.14.0).
  Filling a form text field that carries the Multiline flag also produces a
  wrapped, top-aligned multi-line appearance with per-line quadding (added in
  0.16.0). In both cases a single word wider than the available width overflows
  onto its own line; mid-word breaking is not performed.
```

- [ ] **4.3 Add the CHANGELOG 0.16.0 entry.** In `CHANGELOG.md`, replace the `## [Unreleased]` line (line 9) with a populated 0.16.0 section above the 0.15.0 section:

```
## [Unreleased]

## [0.16.0] - 2026-06-20

### Added

- Filling a form text field that carries the Multiline flag (AcroForm `Tx` field, Ff bit 13) now generates a wrapped, top-aligned multi-line `/AP/N` appearance. Hard `\n` breaks are preserved, each paragraph is greedily word-wrapped to the field width, per-line quadding (left/center/right) is honored, and a word wider than the field overflows onto its own line. Single-line text fields are unchanged.
```

- [ ] **4.4 Bump the npm version.** In `package.json`, change line 3 from `"version": "0.15.0",` to `"version": "0.16.0",`.

- [ ] **4.5 Bump the crate version.** In `crates/core/Cargo.toml`, change `version = "0.15.0"` to `version = "0.16.0"`.

- [ ] **4.6 Rebuild and run the TS suite.** Run `bun run build` (rebuilds pkg-web from Rust and compiles TS), then `bun test`. Expect all TS tests to pass. (No new TS test is strictly required because the Rust integration test in Task 3 covers the behavior end-to-end; add a `tests/fill.test.ts` case only if a fixture field with the Multiline flag is available without programmatic mutation.)

- [ ] **4.7 Final full verification.** Run `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`. Expect all green.

- [ ] **4.8 Commit.**

```bash
git add README.md docs/site/src/content/docs/reference/limitations.md CHANGELOG.md package.json crates/core/Cargo.toml
git commit -m "docs: document multiline text-field fill; release 0.16.0

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Done criteria

- `cargo test --manifest-path crates/core/Cargo.toml` passes, including the new `is_multiline`, `wrap_lines` (6), `multiline_content` (3), and `multiline_text_fill_wraps_into_multiple_lines` tests.
- `cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings` is clean.
- `bun run build` then `bun test` passes.
- Only `fill.rs`, `forms.rs`, `appearance.rs`, their tests, and docs/version files changed.
- Version is `0.16.0` in `package.json` and `crates/core/Cargo.toml`, and the CHANGELOG has a 0.16.0 entry.
