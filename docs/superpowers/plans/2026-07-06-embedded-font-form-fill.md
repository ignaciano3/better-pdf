# Embedded-Font Form Fill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `form.getTextField(name).setText("山田太郎", { font })` renders a correct embedded-font (Type0/CIDFontType2) appearance on plain and multiline text fields of any origin, with UTF-16BE `/V`, and missing glyphs throw instead of silently disappearing (fill AND drawText).

**Architecture:** The fill op gains an optional `fontId` indexing the same document-level font-descriptor list draw ops use. Embedded-font building is hoisted out of `draw_apply` into `apply.rs` so a font used by draw ops and fills builds exactly once per save with the union of used characters. The fill engine renders appearances via the existing `text_appearance_content_embedded` machinery (plus a new multiline variant), writes `/V`/`/DV` with `lopdf::text_string`, and wires the Type0 object into the field `/DA`, widget `/Resources`, and AcroForm `/DR` under the `BPF<n>` alias convention.

**Tech Stack:** Rust (lopdf, ttf-parser via `fonts/mod.rs`) compiled to WASM; TypeScript wrapper; `cargo test` + `bun:test`.

**Spec:** `docs/superpowers/specs/2026-07-05-embedded-font-form-fill-design.md`

## Global Constraints

- Ships as **1.11.0**; the `drawText` missing-glyph flip is a documented behavioral change with opt-out `{ onMissingGlyph: 'skip' }`.
- Standard-14 fills with no `font` option must remain **byte-identical** (existing `/V` WinAnsi invariant tests must stay green untouched).
- `{ font }` allowed on plain + multiline text fields only; comb/dropdown/listbox rejected at BOTH the TS boundary (`FieldTypeError`) and Rust validation (wire JSON is a trust boundary).
- All new output objects are appended (incremental save preserved).
- Rust error strings for missing glyphs MUST start with the exact prefix `missing glyphs` (TS maps on it).
- CI gates clippy (`-D warnings`), not fmt; do not reformat files you don't touch.
- Commands: Rust `cargo test --manifest-path crates/core/Cargo.toml`; clippy `cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`; TS `bun test` (needs `bun run build:wasm` after Rust changes).
- Test fixtures: fonts at `tests/fixtures/fonts/NotoSans-Regular.subset.ttf` (Rust: `include_bytes!("../../../tests/fixtures/...")`). For CJK tests use a CJK-capable font; if `tests/fixtures/fonts/` has none, add a subsetted Noto Sans JP (see Task 6 Step 1).

---

### Task 1: Hoist shared embedded-font building into `apply.rs`

Fonts must build once per save, shared by draw ops and (after Task 3) fills. Today `draw_apply` builds them internally. Extract a build step in `apply_all_json` that collects used chars from draw ops now (fill values join in Task 3), builds each font, and passes the built map into `draw_apply`.

**Files:**
- Modify: `crates/core/src/draw.rs` (split font building out of `draw_apply`)
- Modify: `crates/core/src/apply.rs:88-102` (build fonts before Phase B, pass map in)
- Test: inline `#[cfg(test)]` in `crates/core/src/apply.rs`

**Interfaces:**
- Produces: `pub(crate) struct FontDesc { pub offset: usize, pub length: usize, pub subset: bool }` (fields made `pub(crate)`).
- Produces: `pub(crate) fn build_document_fonts(doc_add: &mut dyn FnMut(Object) -> ObjectId, font_descs: &[FontDesc], fonts_blob: &[u8], used_per_font: &HashMap<usize, BTreeSet<char>>) -> Result<HashMap<usize, (ObjectId, BuiltFont)>, String>` in `draw.rs` — mirrors the create-path pre-pass at `create.rs:596-648`.
- Produces: `pub(crate) fn draw_used_chars(ops: &[DrawOp]) -> HashMap<usize, BTreeSet<char>>` (extracts the per-font char collection currently inline in `draw_apply`).
- Produces: `draw_apply` gains a `built: &HashMap<usize, (ObjectId, BuiltFont)>` parameter and no longer builds fonts itself.
- Consumes: `build_embedded_font`, `BuiltFont`, `EmbeddedFontInput` from `crates/core/src/fonts/mod.rs:11-28`.

- [ ] **Step 1: Write the failing test** — in `apply.rs` tests: a plan with two draw text ops sharing `fontId: 0` produces exactly ONE Type0 font object in the saved output.

```rust
#[test]
fn shared_font_builds_once_across_draw_ops() {
    const FONT: &[u8] =
        include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
    // Created base doc with one page, then apply two draw ops using the same font.
    let base = crate::create::create_document_json(
        r#"[{"op":"addPage","width":300,"height":300}]"#, &[], &[], "[]", "[]", false, false,
    ).unwrap();
    let plan = format!(
        r#"{{"draw":{{"ops":[
            {{"op":"text","page":0,"x":10,"y":40,"size":12,"text":"Ab","fontId":0}},
            {{"op":"text","page":0,"x":10,"y":20,"size":12,"text":"Cd","fontId":0}}
        ],"fonts":[{{"offset":0,"length":{},"subset":true}}]}}}}"#,
        FONT.len()
    );
    let out = apply_all_json(&base, &plan, &[], &[], FONT, false).unwrap();
    let doc = lopdf::Document::load_mem(&out).unwrap();
    let type0_count = doc.objects.values().filter(|o| {
        o.as_dict().ok()
            .and_then(|d| d.get(b"Subtype").ok())
            .and_then(|s| s.as_name().ok())
            == Some(b"Type0")
    }).count();
    assert_eq!(type0_count, 1, "font must build exactly once");
}
```

- [ ] **Step 2: Run test to verify it fails** — `cargo test --manifest-path crates/core/Cargo.toml shared_font_builds_once` . If it already PASSES (draw_apply may already dedupe per call), keep the test as a pinning regression and treat this task as pure refactor: proceed, and rely on Step 4's "all existing tests green" as the gate.

- [ ] **Step 3: Refactor.** In `draw.rs`: extract the used-char collection into `draw_used_chars(ops)`; extract the build loop into `build_document_fonts(...)` (body copied from the current in-`draw_apply` logic, which mirrors `create.rs:596-648` — iterate ids sorted, slice `&fonts_blob[fd.offset..fd.offset + fd.length]`, `build_embedded_font`); change `draw_apply(inc, ops, draw_images, fonts, font_descs)` to `draw_apply(inc, ops, draw_images, built)` looking up `built[&font_id]`. In `apply.rs` Phase B, before dispatching draw:

```rust
let built_fonts = if let Some(d) = &plan.draw {
    let used = draw::draw_used_chars(&d.ops);
    let mut add = |o: Object| inc.new_document.add_object(o);
    draw::build_document_fonts(&mut add, &d.fonts, fonts, &used)?
} else {
    Default::default()
};
// ... existing dispatch becomes:
// draw::draw_apply(&mut inc, &d.ops, draw_images, &built_fonts)?
```

Keep the `BPE{id}` draw alias behavior exactly as-is (alias registration stays wherever it lives in `draw_apply`; only the *build* moves).

- [ ] **Step 4: Run the full Rust suite + clippy** — `cargo test --manifest-path crates/core/Cargo.toml && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`. Expected: all PASS (pure refactor; no behavior change).

- [ ] **Step 5: Commit** — `git add crates/core/src/draw.rs crates/core/src/apply.rs && git commit -m "refactor(core): hoist embedded-font building out of draw_apply into apply pipeline"`

---

### Task 2: Missing-glyph detection + `onMissingGlyph` in draw/create text paths (Rust)

Replace the silent `filter_map` drops at `create.rs:1348-1355` and `draw.rs:1429-1436` with a checked encode. Default: error. Wire opt-out: `onMissingGlyph: "skip"` on the text op.

**Files:**
- Modify: `crates/core/src/fonts/mod.rs` (add checked GID encoder)
- Modify: `crates/core/src/draw.rs` (DrawOp text variant gains `on_missing_glyph`, use encoder)
- Modify: `crates/core/src/create.rs` (CreateOp::Text likewise)
- Test: inline tests in `fonts/mod.rs` and `draw.rs`

**Interfaces:**
- Produces (in `fonts/mod.rs`):

```rust
pub enum MissingGlyphPolicy { Error, Skip }

/// Map chars to GIDs per line ('\n'-split). `context` is e.g. "drawText on page 0"
/// or "field 'name'". Error format (STABLE, TS matches the prefix):
///   missing glyphs in font for {context}: "㐀" (U+3400), "丂" (U+4E02)
pub fn gids_per_line(
    built: &BuiltFont,
    text: &str,
    policy: MissingGlyphPolicy,
    context: &str,
) -> Result<Vec<Vec<u16>>, String>
```

- Excluded from the check: `\n` (line split), `\r`, `\t`, and any `char::is_control`. A missing *space* glyph IS an error under `Error` (a font without space can't render sentences). Offenders deduped, sorted by code point, max 8 listed then `… and N more`.
- Wire: `"onMissingGlyph": "skip"` (serde `Option<String>`, absent = error) on draw/create text ops.

- [ ] **Step 1: Write failing tests** in `fonts/mod.rs`:

```rust
#[test]
fn gids_per_line_errors_on_missing_glyph_with_codepoint() {
    let mut gid_for = std::collections::HashMap::new();
    gid_for.insert('A', 1u16);
    let built = BuiltFont { gid_for };
    let err = gids_per_line(&built, "A㐀", MissingGlyphPolicy::Error, "drawText on page 0")
        .unwrap_err();
    assert!(err.starts_with("missing glyphs"), "got: {err}");
    assert!(err.contains("U+3400"), "got: {err}");
    assert!(err.contains("drawText on page 0"), "got: {err}");
}

#[test]
fn gids_per_line_skip_matches_old_behavior_and_ignores_control_chars() {
    let mut gid_for = std::collections::HashMap::new();
    gid_for.insert('A', 1u16);
    let built = BuiltFont { gid_for };
    let lines = gids_per_line(&built, "A㐀\nA", MissingGlyphPolicy::Skip, "x").unwrap();
    assert_eq!(lines, vec![vec![1u16], vec![1u16]]);
    // control chars never error
    assert!(gids_per_line(&built, "A\tA", MissingGlyphPolicy::Error, "x").is_ok());
}
```

- [ ] **Step 2: Run to verify FAIL** — `cargo test --manifest-path crates/core/Cargo.toml gids_per_line` → compile error: `gids_per_line` not found.

- [ ] **Step 3: Implement** in `fonts/mod.rs`:

```rust
pub enum MissingGlyphPolicy { Error, Skip }

pub fn gids_per_line(
    built: &BuiltFont,
    text: &str,
    policy: MissingGlyphPolicy,
    context: &str,
) -> Result<Vec<Vec<u16>>, String> {
    let mut missing: std::collections::BTreeSet<char> = Default::default();
    let lines: Vec<Vec<u16>> = text
        .split('\n')
        .map(|line| {
            line.chars()
                .filter_map(|c| {
                    if c.is_control() {
                        return None;
                    }
                    match built.gid_for.get(&c) {
                        Some(g) => Some(*g),
                        None => {
                            missing.insert(c);
                            None
                        }
                    }
                })
                .collect()
        })
        .collect();
    if !missing.is_empty() && matches!(policy, MissingGlyphPolicy::Error) {
        let shown: Vec<String> = missing
            .iter()
            .take(8)
            .map(|c| format!("\"{c}\" (U+{:04X})", *c as u32))
            .collect();
        let more = missing.len().saturating_sub(8);
        let tail = if more > 0 { format!(", … and {more} more") } else { String::new() };
        return Err(format!(
            "missing glyphs in font for {context}: {}{tail}",
            shown.join(", ")
        ));
    }
    Ok(lines)
}
```

Then replace both `filter_map` sites: in `draw.rs:1429-1436` and `create.rs:1348-1355`, call `fonts::gids_per_line(built, text, policy, &format!("drawText on page {page}"))?` where `policy` comes from the op's new `on_missing_glyph` field (`Some("skip") => Skip`, else `Error`). Add to the text op structs (both files): `#[serde(default, rename = "onMissingGlyph")] on_missing_glyph: Option<String>,`.

- [ ] **Step 4: Run** — `cargo test --manifest-path crates/core/Cargo.toml` . Some existing draw/create tests using text with unsupported chars may now fail — inspect each: if the test *intends* a missing glyph, add `"onMissingGlyph":"skip"` to its op JSON; otherwise the failure is a real bug in this step. Then clippy. Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(core): error on missing glyphs in embedded-font text (opt-out onMissingGlyph=skip)"`

---

### Task 3: Embedded-font fill engine (Rust, `fill.rs`)

The core of the feature. `FillOp` gains `fontId`; `apply.rs` threads the built-font map and font metadata into fill; `ap_inputs`' Type0 guard is replaced; appearance renders via the embedded engine (single-line + multiline); `/V`/`/DV` become UTF-16BE; `/DA`, widget `/Resources`, and AcroForm `/DR` get the `BPF<n>` Type0 wiring.

**Files:**
- Modify: `crates/core/src/fill.rs` (FillOp, ApInputs, resolve, draw_appearances, validation)
- Modify: `crates/core/src/apply.rs` (fill values join `used_per_font`; pass built fonts + bytes into fill)
- Modify: `crates/core/src/appearance.rs` (multiline embedded content builder)
- Test: inline tests in `fill.rs`

**Interfaces:**
- Wire: `FillOp` (fill.rs:12-27) gains `#[serde(default)] font_id: Option<usize>` (camelCase → `fontId`). It indexes `plan.draw.fonts` — the SAME `FontDesc` list draw ops use. TS guarantees a `draw.fonts` section exists whenever any fill op carries `fontId` (Task 5).
- Produces (in `appearance.rs`): `pub fn text_appearance_content_embedded_multiline(lines: &[&str], size: f32, w: f32, h: f32, q: i64, color_op: &str, alias: &str, built: &BuiltFont, font_bytes: &[u8]) -> Result<String, String>` — multiline sibling of `text_appearance_content_embedded` (appearance.rs:299): same Identity-H hex `Tj` per line, baseline stepping like `text_appearance_content_multiline`.
- Consumes: `fonts::wrap_embedded(font_bytes, size, avail_w, text)` (fonts/mod.rs:195), `fonts::gids_per_line` (Task 2), built map from Task 1.
- `ApInputs` gains `embedded: Option<EmbeddedApFont>` where `struct EmbeddedApFont { alias: String, type0_id: ObjectId, built: BuiltFont-ref-or-index, bytes_range: (usize, usize) }` — concretely, store the `font_id: usize` and look up `(ObjectId, BuiltFont)` + bytes at apply time from the maps threaded through (avoids lifetime plumbing in `ApInputs`).
- Validation errors (exact strings, thrown from fill resolve):
  - non-text field with fontId: `embedded fonts are supported on plain and multiline text fields only (field '{name}')`
  - comb text field with fontId: same message.
  - fontId out of range: `font id {i} out of range`
- The old guard error for Type0-DA fill WITHOUT fontId becomes: `field '{name}' uses an embedded font; pass {{ font }} to setText with an embedded font`
- Glyph check: `fonts::gids_per_line(built, value, MissingGlyphPolicy::Error, &format!("field '{name}'"))` — fill has NO skip option; always Error. Runs during resolve/apply BEFORE any object is written (fill resolve is Phase A, pre-mutation, so a throw aborts the whole save with no partial output — this property comes free from the existing phase structure).

- [ ] **Step 1: Write failing tests** in `fill.rs` (via `apply_all_json`, since fontId fills only flow through the apply pipeline):

```rust
const NOTO: &[u8] = include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");

/// Base doc: one page + one standard-14 plain text field "n" (+ variants below).
fn base_with_field(fields: &str) -> Vec<u8> {
    crate::create::create_document_json(
        r#"[{"op":"addPage","width":300,"height":300}]"#, &[], &[], "[]", fields, false, false,
    ).unwrap()
}
fn fill_plan(op_json: &str, font_len: usize) -> String {
    format!(
        r#"{{"fill":[{op_json}],"draw":{{"ops":[],"fonts":[{{"offset":0,"length":{font_len},"subset":true}}]}}}}"#
    )
}

#[test]
fn fills_standard14_field_with_embedded_font() {
    let base = base_with_field(r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#);
    let plan = fill_plan(r#"{"name":"n","value":"Añb","fontId":0}"#, NOTO.len());
    let out = apply_all_json(&base, &plan, &[], &[], NOTO, false).unwrap();
    let doc = lopdf::Document::load_mem(&out).unwrap();
    // /V is UTF-16BE (BOM FE FF)
    let v = crate::forms::read_fields_json(&out).unwrap();
    assert!(v.contains(r#""value":"Añb""#), "round-trip via read_fields: {v}");
    // DA references BPF0 and /DR has it as Type0
    let field = doc.objects.values().find_map(|o| {
        let d = o.as_dict().ok()?;
        (d.get(b"T").ok()?.as_str().ok()? == b"n").then_some(d)
    }).unwrap();
    let da = field.get(b"DA").unwrap().as_str().unwrap();
    assert!(da.starts_with(b"/BPF0 "), "DA: {}", String::from_utf8_lossy(da));
}

#[test]
fn embedded_fill_multiline_wraps() {
    let base = base_with_field(r#"[{"type":"text","name":"m","page":0,"x":10,"y":10,"width":60,"height":60,"multiline":true}]"#);
    let plan = fill_plan(r#"{"name":"m","value":"aaaa bbbb cccc dddd","fontId":0}"#, NOTO.len());
    let out = apply_all_json(&base, &plan, &[], &[], NOTO, false).unwrap();
    // appearance stream contains >1 text-showing op (one hex Tj per wrapped line)
    let doc = lopdf::Document::load_mem(&out).unwrap();
    let ap = doc.objects.values().find_map(|o| {
        let s = o.as_stream().ok()?;
        let d = &s.dict;
        (d.get(b"Subtype").ok()?.as_name().ok()? == b"Form").then(|| s.decompressed_content().unwrap())
    }).unwrap();
    let tj_count = ap.windows(3).filter(|w| w == b"Tj\n" || &w[..2] == b"Tj").count();
    assert!(tj_count >= 2, "expected wrapped lines, got content: {}", String::from_utf8_lossy(&ap));
}

#[test]
fn embedded_fill_missing_glyph_errors_before_write() {
    let base = base_with_field(r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#);
    let plan = fill_plan(r#"{"name":"n","value":"日本語","fontId":0}"#, NOTO.len()); // Latin subset font
    let err = apply_all_json(&base, &plan, &[], &[], NOTO, false).unwrap_err();
    assert!(err.starts_with("missing glyphs"), "got: {err}");
    assert!(err.contains("field 'n'"), "got: {err}");
}

#[test]
fn embedded_fill_rejects_comb_and_choice() {
    let base = base_with_field(r#"[{"type":"text","name":"c","page":0,"x":10,"y":10,"width":200,"height":20,"comb":true,"maxLength":4}]"#);
    let plan = fill_plan(r#"{"name":"c","value":"ab","fontId":0}"#, NOTO.len());
    let err = apply_all_json(&base, &plan, &[], &[], NOTO, false).unwrap_err();
    assert!(err.contains("plain and multiline text fields only"), "got: {err}");
}

#[test]
fn refilling_builder_embedded_field_now_works() {
    // The fixture from the old rejects_filling_a_type0_da_font_field test — now with fontId it succeeds.
    let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, NOTO.len());
    let fields = r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20,"value":"A","fontId":0}]"#;
    let base = crate::create::create_document_json(
        r#"[{"op":"addPage","width":300,"height":300}]"#, &[], NOTO, &fonts_json, fields, false, false,
    ).unwrap();
    let plan = fill_plan(r#"{"name":"n","value":"B","fontId":0}"#, NOTO.len());
    let out = apply_all_json(&base, &plan, &[], &[], NOTO, false).unwrap();
    let v = crate::forms::read_fields_json(&out).unwrap();
    assert!(v.contains(r#""value":"B""#), "{v}");
}

#[test]
fn type0_da_fill_without_font_gives_actionable_error() {
    // Same base as above, but fill WITHOUT fontId.
    let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, NOTO.len());
    let fields = r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20,"value":"A","fontId":0}]"#;
    let base = crate::create::create_document_json(
        r#"[{"op":"addPage","width":300,"height":300}]"#, &[], NOTO, &fonts_json, fields, false, false,
    ).unwrap();
    let err = fill_fields_json(&base, r#"[{"name":"n","value":"B"}]"#, &[], false).unwrap_err();
    assert!(err.contains("pass { font }"), "got: {err}");
}
```

(Adjust the two object-scanning assertions to the actual lopdf accessors if the compiler objects — intent is fixed: DA prefix `/BPF0`, ≥2 Tj ops, UTF-16BE round-trip via `read_fields_json`.)

- [ ] **Step 2: Run to verify FAIL** — `cargo test --manifest-path crates/core/Cargo.toml --lib fill::` → new tests fail (unknown field `fontId` is silently ignored by serde default → guard error / missing DA), old `rejects_filling_a_type0_da_font_field` still passes.

- [ ] **Step 3: Implement.** In order:
  1. `FillOp`: add `#[serde(default)] font_id: Option<usize>`.
  2. `apply.rs`: extend the used-chars collection from Task 1 — before building fonts, iterate `plan.fill` ops: for each with `font_id: Some(i)`, extend `used_per_font[i]` with `value`/`default_value` chars. Build fonts even when `d.ops` is empty (the plan may carry fonts only for fills). Thread into fill: change `fill::fill_resolve(&doc, ops, fill_images)` to also receive `&built_fonts`, `&d.fonts` (for byte ranges) and the `fonts` blob. NOTE: font objects must be added to `inc.new_document` (they're new appended objects) — build them against `inc` before Phase A resolve reads, or restructure so resolve only *plans* and object creation happens in apply; follow whichever `apply.rs` already does for fill signature images.
  3. `fill.rs` validation (in resolve, before `ap_inputs`): fontId range check against `d.fonts.len()`; field-type check — allowed only when the resolved field is a text field with `!comb` (`ff` flags via existing `forms::is_comb`); exact error strings from Interfaces.
  4. `ap_inputs`: accept an `Option<usize>` fontId. When `Some`, skip the DA font resolution entirely (`font_ref`/`widths` unused for embedded) and record the embedded font. When `None` and DA font is Type0, keep the guard but with the new actionable message.
  5. `draw_appearances`: when embedded, per widget: glyph-check via `gids_per_line(built, value, Error, &format!("field '{name}'"))?`; single-line → `text_appearance_content_embedded(&val, size, w, h, q, &color_op, &alias, built, bytes)`; multiline → wrap with `fonts::wrap_embedded(bytes, size, (w-4.0).max(1.0), value)?` then new `text_appearance_content_embedded_multiline`. Auto-size (`da.size == 0`) via `measure_embedded` analog of `appearance::auto_size`. Build the XObject with `/Resources /Font << /BPF{i} <type0_ref> >>`.
  6. `/V`/`/DV`: when fontId present, `lopdf::text_string(value)` (already the `Apply::Text` shape at fill.rs:846-849 — branch on embedded).
  7. `/DA` on the field: `Object::string_literal(format!("/BPF{i} {size} Tf {color_op}"))`; merge `BPF{i} => Reference(type0_id)` into AcroForm `/DR /Font` (create the dicts if the loaded doc lacks them — modify the AcroForm dict in the incremental update, same mechanics `flatten.rs`/`fill.rs` already use for AP writes).
  8. `appearance.rs`: implement `text_appearance_content_embedded_multiline` by combining the line-loop/baseline math of `text_appearance_content_multiline` with the per-line hex-`Tj` emission of `text_appearance_content_embedded` (share a helper if trivial; don't force it).
  9. Delete/replace the old test `rejects_filling_a_type0_da_font_field` expectation ("not yet supported") — superseded by `type0_da_fill_without_font_gives_actionable_error`.

- [ ] **Step 4: Run** — full `cargo test` + clippy. Expected: PASS, including untouched WinAnsi `/V` invariant tests.

- [ ] **Step 5: Commit** — `git commit -am "feat(core): embedded-font form fill (fontId on fill ops, Type0 appearance, UTF-16BE /V)"`

---

### Task 4: Batched fill+flatten regression (Rust)

The 1.10.1-shaped bug: flatten must stamp the embedded appearance generated by a fill in the SAME batched save (apply-time resolution, `docs` memory: apply.rs Phase A resolves pre-mutation).

**Files:**
- Test: inline test in `crates/core/src/apply.rs`

**Interfaces:** none new — pure regression test.

- [ ] **Step 1: Write the test**

```rust
#[test]
fn embedded_fill_then_flatten_in_one_save_stamps_embedded_appearance() {
    const NOTO: &[u8] =
        include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
    let base = crate::create::create_document_json(
        r#"[{"op":"addPage","width":300,"height":300}]"#, &[], &[], "[]",
        r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        false, false,
    ).unwrap();
    let plan = format!(
        r#"{{"fill":[{{"name":"n","value":"Añb","fontId":0}}],"flatten":["n"],"draw":{{"ops":[],"fonts":[{{"offset":0,"length":{},"subset":true}}]}}}}"#,
        NOTO.len()
    );
    let out = apply_all_json(&base, &plan, &[], &[], NOTO, false).unwrap();
    let doc = lopdf::Document::load_mem(&out).unwrap();
    // Field is gone (flattened)...
    let fields = crate::forms::read_fields_json(&out).unwrap();
    assert!(!fields.contains(r#""name":"n""#), "field should be flattened: {fields}");
    // ...and the page content references the stamped Form XObject whose resources
    // carry the BPF0 Type0 font.
    let has_bpf = doc.objects.values().any(|o| {
        o.as_stream().ok()
            .map(|s| String::from_utf8_lossy(&format!("{:?}", s.dict).into_bytes()).contains("BPF0"))
            .unwrap_or(false)
    });
    assert!(has_bpf, "flattened output must carry the embedded-font appearance");
}
```

(If flatten stamps by moving the widget's `/AP` XObject, asserting any object dict mentions `BPF0` is sufficient; tighten to the page `/Resources` if easy.)

- [ ] **Step 2: Run** — expected PASS if Task 3 respected apply-time resolution; if it FAILS, the fix belongs in flatten's appearance re-resolution (see `flatten.rs` apply-time lookup added in 1.10.1) — flatten must read the widget `/AP` from `inc.new_document` state, not the pre-mutation doc.

- [ ] **Step 3: Commit** — `git commit -am "test(core): embedded fill + flatten in one batched save regression"`

---

### Task 5: TypeScript API — `setText({ font })`, wire plumbing, `MissingGlyphError`, `onMissingGlyph`

**Files:**
- Modify: `src/core/errors.ts` (new `MissingGlyphError`)
- Modify: `src/exports-common.ts` (re-export it)
- Modify: `src/forms/fields.ts` (setText/setDefaultText options, FillOp fontId, comb/type guards)
- Modify: `src/generate/page.ts` (DrawTextOptions.onMissingGlyph)
- Modify: `src/core/document.ts` (applyAll plan assembly: include `draw.fonts` whenever any fill op has fontId; map core "missing glyphs" errors)
- Test: `tests/embedded-font-fill.test.ts` (Task 6 holds the integration tests; this task carries unit-level ones)

**Interfaces:**
- Produces:

```ts
/** Thrown when text contains characters the embedded font has no glyph for. */
export class MissingGlyphError extends PdfError {
  constructor(readonly detail: string) {
    super(detail); // detail is the core message: 'missing glyphs in font for …: "㐀" (U+3400)'
  }
}
```

- `setText(value: string, opts?: { font?: PdfFont }): void` and `setDefaultText(value: string, opts?: { font?: PdfFont }): void` on `PdfTextField` (fields.ts:209-238). Behavior: if `opts.font` present → read `opts.font[kFontId]`; `undefined` (standard-14 handle) → throw `PdfError("setText({ font }) requires an embedded font from doc.embedFont(); for standard-14 fonts omit the option")`; if `this.info.comb` → `FieldTypeError`; queue `{ name, value, fontId }`.
- `FillOp` union (fields.ts:34-41): value/defaultValue variants gain `fontId?: number`; `FillQueue.toPayload()` passes `fontId` through on the wire object (it already spreads non-image ops — verify the object shape includes it).
- Only `PdfTextField` gets the option — checkbox/radio/dropdown/listbox/signature classes are untouched, so choice fields are rejected by the type system; the Rust guard (Task 3) covers untyped callers.
- `DrawTextOptions` gains `/** 'throw' (default): error on characters missing from an embedded font. 'skip': old behavior. */ onMissingGlyph?: 'throw' | 'skip'` — threaded onto the draw op wire as `onMissingGlyph: "skip"` only when set to `'skip'`.
- `document.ts` save/applyAll assembly: when any queued fill op has `fontId !== undefined`, the plan's `draw` section MUST be present with the document's full `fonts` descriptor list (even with `ops: []`), and the fonts blob passed as usual. Error mapping: wrap core error strings starting with `missing glyphs` into `MissingGlyphError` at every WASM call site that can throw it (`apply_all`, `fill_fields`, `create_document`, `apply_draw_ops`) — do it in the shared error-translation helper if one exists; otherwise add `function translateCoreError(e: unknown): never` in `src/core/errors.ts` and use it at those call sites.

- [ ] **Step 1: Write failing unit tests** — `tests/embedded-font-fill.test.ts`:

```ts
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument, MissingGlyphError, FieldTypeError, PdfError } from "../src/index.ts";

const NOTO = new Uint8Array(readFileSync(join(import.meta.dir, "fixtures/fonts/NotoSans-Regular.subset.ttf")));

test("setText rejects a standard-14 PdfFont handle", async () => {
  const doc = PdfDocument.create();
  doc.addPage();
  const fields = doc.createForm();
  fields.addTextField({ name: "n", page: 0, x: 10, y: 10, width: 200, height: 20 });
  const form = doc.getForm();
  const helv = doc.getFont(/* StandardFonts.Helvetica */ "Helvetica" as never);
  expect(() => form.getTextField("n").setText("x", { font: helv })).toThrow(PdfError);
});

test("setText with font on a comb field throws FieldTypeError", async () => {
  const doc = PdfDocument.create();
  doc.addPage();
  doc.createForm().addTextField({ name: "c", page: 0, x: 10, y: 10, width: 200, height: 20, comb: true, maxLength: 4 });
  const font = await doc.embedFont(NOTO);
  expect(() => doc.getForm().getTextField("c").setText("ab", { font })).toThrow(FieldTypeError);
});

test("missing glyph surfaces as MissingGlyphError at save", async () => {
  const doc = PdfDocument.create();
  doc.addPage();
  doc.createForm().addTextField({ name: "n", page: 0, x: 10, y: 10, width: 200, height: 20 });
  const font = await doc.embedFont(NOTO); // Latin-only subset fixture
  doc.getForm().getTextField("n").setText("日本語", { font });
  expect(doc.save()).rejects.toThrow(MissingGlyphError);
});

test("drawText throws MissingGlyphError by default and skips with onMissingGlyph", async () => {
  const doc = PdfDocument.create();
  const page = doc.addPage();
  const font = await doc.embedFont(NOTO);
  page.drawText("日本語", { x: 10, y: 10, size: 12, font });
  expect(doc.save()).rejects.toThrow(MissingGlyphError);

  const doc2 = PdfDocument.create();
  const page2 = doc2.addPage();
  const font2 = await doc2.embedFont(NOTO);
  page2.drawText("日本語", { x: 10, y: 10, size: 12, font: font2, onMissingGlyph: "skip" });
  await doc2.save(); // must not throw
});
```

(Adapt `addTextField` option names and `getFont` invocation to the real builder API — see `src/generate/form-builder.ts:352` and `StandardFonts` in `src/generate/fonts.ts`; the assertions are the contract.)

NOTE: `getForm()` on a created doc materializes and **seals** the builder — if `createForm()` + `getForm()` interplay fights these tests, load a saved intermediate instead (save → load → setText), which is also closer to the flagship use case.

- [ ] **Step 2: Run to verify FAIL** — `bun run build:wasm && bun test tests/embedded-font-fill.test.ts` → type errors / throws missing.

- [ ] **Step 3: Implement** per Interfaces above: errors.ts class + translation helper, fields.ts options + guards + FillOp fontId, page.ts option + wire threading, document.ts plan assembly (fonts section forced when fill uses fontId).

- [ ] **Step 4: Run** — `bun test` (full suite) and `tsc --noEmit`. Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat: setText({ font }) embedded-font fill API, MissingGlyphError, drawText onMissingGlyph"`

---

### Task 6: End-to-end integration tests (CJK, round-trip, subsetting, flatten)

**Files:**
- Create: `tests/fixtures/fonts/NotoSansJP-Regular.subset.ttf` — a CJK-capable subset covering at least 山田太郎日本語 + ASCII. Generate with `pip install fonttools && pyftsubset NotoSansJP-Regular.ttf --text='山田太郎日本語 abcABC' --output-file=tests/fixtures/fonts/NotoSansJP-Regular.subset.ttf` from the Google-Fonts TTF (OFL — note the license in `tests/fixtures/fonts/OFL.txt` if not already present). If the repo already has a CJK fixture, reuse it and skip creation.
- Modify: `tests/embedded-font-fill.test.ts` (extend)

**Interfaces:** none new.

- [ ] **Step 1: Write the tests**

```ts
const NOTO_JP = new Uint8Array(readFileSync(join(import.meta.dir, "fixtures/fonts/NotoSansJP-Regular.subset.ttf")));
const FICHA = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

test("flagship: CJK fill on a loaded standard-14 field, round-trips", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP);
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  const out = await doc.save();
  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("山田太郎");
});

test("CJK fill + flatten in one save", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP);
  const form = doc.getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  form.flatten();
  const out = await doc.save();
  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getFields().length).toBe(0);
  expect(out.length).toBeGreaterThan(0);
});

test("multiline CJK fill wraps and round-trips", async () => {
  const doc = PdfDocument.create();
  doc.addPage();
  doc.createForm().addTextField({ name: "m", page: 0, x: 10, y: 10, width: 80, height: 60, multiline: true });
  const saved = await doc.save();
  const loaded = await PdfDocument.load(saved);
  const font = await loaded.embedFont(NOTO_JP);
  loaded.getForm().getTextField("m").setText("日本語 日本語 日本語", { font });
  const out = await loaded.save();
  expect((await PdfDocument.load(out)).getForm().getField("m")?.value).toBe("日本語 日本語 日本語");
});

test("subset font used only for fill renders all value glyphs (no throw)", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP); // subset: true default; no drawText usage
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  await doc.save(); // MissingGlyphError here would mean fill chars didn't join used_per_font
});

test("font shared by drawText and fill saves without error and embeds once", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP);
  doc.getPage(0).drawText("日本語", { x: 20, y: 20, size: 10, font });
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  const out = await doc.save();
  // crude single-embed check: the font program bytes appear once
  const marker = NOTO_JP.slice(0, 64);
  let count = 0;
  outer: for (let i = 0; i <= out.length - marker.length; i++) {
    for (let j = 0; j < marker.length; j++) if (out[i + j] !== marker[j]) continue outer;
    count++;
  }
  expect(count).toBeLessThanOrEqual(1); // 0 if compressed/subsetted — then drop this assertion, keep the no-throw
});
```

(The single-embed byte-scan is best-effort — FontFile2 streams are FlateDecoded so it may find 0 matches; the authoritative single-build check is the Rust test from Task 1. Keep the no-throw behavior as the TS assertion if the scan is vacuous.)

- [ ] **Step 2: Run** — `bun test tests/embedded-font-fill.test.ts`. Expected: PASS (implementation complete in Tasks 3+5). Any failure here is a real integration bug — debug, don't weaken assertions.

- [ ] **Step 3: Visual acceptance (manual, not CI)** — write one filled+flattened CJK PDF to the scratchpad, open it in Preview/Acrobat, confirm no tofu. Record "visually verified" in the commit message.

- [ ] **Step 4: Full suite + render check** — `bun test && bun run test:render && cargo test --manifest-path crates/core/Cargo.toml`. Expected: all PASS.

- [ ] **Step 5: Commit** — `git commit -am "test: CJK embedded-font fill end-to-end (round-trip, multiline, flatten, subsetting)"`

---

### Task 7: Docs + version bump

**Files:**
- Modify: `README.md` (limitations: remove "embedded/CJK fonts not supported for form-field values" and "missing glyphs silently skipped"; features: add embedded-font fill; document `onMissingGlyph`)
- Modify: `docs/migrating-from-pdf-lib.md` (add mapping: pdf-lib `field.updateAppearances(customFont)` → `field.setText(value, { font })`)
- Modify: `CHANGELOG.md` + `package.json` (1.11.0)
- Test: none (docs)

**Interfaces:** none.

- [ ] **Step 1: Update README** — limitations section (~line 839-880): delete the two closed items; add under a "Behavioral changes in 1.11.0" note: `drawText` and form fill now throw `MissingGlyphError` when an embedded font lacks a glyph; `drawText(text, { onMissingGlyph: 'skip' })` restores the old silent-skip. Features section: embedded-font `setText({ font })` example (the 4-line CJK snippet from the spec). Keep the still-true limitation: comb/choice fields remain standard-14 only.
- [ ] **Step 2: Update migrating-from-pdf-lib.md** — add the `updateAppearances` row; note fill throws (not silently blanks) on missing glyphs, unlike pdf-lib.
- [ ] **Step 3: CHANGELOG 1.11.0** — `### Added` embedded-font form fill; `### Changed (behavioral)` missing-glyph throw with opt-out, framed as a data-loss bug fix. Bump `package.json` version to `1.11.0`.
- [ ] **Step 4: Verify docs claims** — grep README for "silently" and "not yet supported" to catch stale claims: `grep -n "silently\|not yet supported" README.md`. Every remaining hit must still be true.
- [ ] **Step 5: Commit** — `git commit -am "docs: embedded-font form fill, MissingGlyphError behavioral note; bump 1.11.0"`

---

## Self-Review (completed)

- **Spec coverage:** API (Task 5), engine (Task 3), shared single-build subsetting (Tasks 1+3+6), missing-glyph policy incl. drawText flip + opt-out (Tasks 2+5), fill+flatten batched regression (Task 4), UTF-16BE round-trip (Tasks 3+6), dual-boundary guards (Tasks 3+5), byte-identical WinAnsi invariant (existing tests, gated in Tasks 3-4), docs/versioning (Task 7). Visual acceptance (Task 6 Step 3). No gaps found.
- **Placeholder scan:** clean — every code step shows code; the two "adapt to real accessors" notes fix the assertion intent, not the behavior.
- **Type consistency:** `fontId`/`font_id` naming consistent (serde camelCase); `gids_per_line` signature identical across Tasks 2/3; `MissingGlyphError` name consistent across Tasks 2 (Rust prefix contract) and 5 (TS class); `BPF<n>` alias consistent with create path.
