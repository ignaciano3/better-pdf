# Multi-Select List Box Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support filling multi-select AcroForm list boxes (`Ch` fields with the Multiselect flag, `Ff` bit 22 = `1 << 21`) by writing `/V` as an array of export values, `/I` as the sorted array of selected indices, and an appearance that highlights every selected row.

**Architecture:** Extend the fill op wire schema with an optional `values: Vec<String>` carried alongside the existing `value: Option<String>`, so single-value fills stay byte-for-byte unchanged. The Rust `resolve` step gains a `ListBoxMulti { values, indices, ap }` apply variant gated on the Multiselect flag; `apply` writes the `/V` string array plus the sorted `/I` array; a new `appearance::listbox_multi_content` builder draws one row per option with a light-blue highlight rectangle behind selected rows. TypeScript adds `PdfListBox.selectMultiple(values)`, a `multiSelect` flag on `FieldInfo`, and a typed `getListBoxMulti` wrapper.

**Tech Stack:** Rust (lopdf), wasm-bindgen, TS API, bun test.

## Global Constraints
- The op-schema change MUST stay backward-compatible with single-value fills: existing `{"name","value"}` ops must serialize and deserialize unchanged (the new `values` key is omitted when absent).
- Reject multi-value fills on non-multiselect fields (a single-select list box, dropdown, or any non-choice field) with a clear error.
- Reject any export value in a multi-value fill that is not a real `/Opt` entry.
- Build the WASM package (`bun run build:wasm`, which emits `pkg-web/`) before running TS tests; `bun test` consumes the built core.
- `source ~/.cargo/env` before any cargo command; on a fresh checkout build `pkg-web` first.
- Rust must pass `cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings`.
- Bump the package + crate version to `0.16.0` (minor) if not already bumped this cycle (currently `0.15.0` in `package.json` and `crates/core/Cargo.toml`).
- Update `README.md` and every doc that says "single-select only" / "single-select in this version" to describe multi-select support.

---

## Task 1 — Op schema + Rust resolve/apply for multi-value `/V` + `/I`

Add the `values` field to the wire op, classify multiselect list boxes, validate every value against `/Opt`, reject multi-value on non-multiselect fields, and write `/V` as a string array and `/I` as the sorted index array.

**Files:**
- Modify `crates/core/src/forms.rs` (expose a `multi_select` flag on `FieldInfo` and a `is_multiselect` helper).
- Modify `crates/core/src/fill.rs` (op struct, `resolve`, `Apply`, `apply`, tests).

**Interfaces (exact signatures):**

```rust
// crates/core/src/fill.rs — extended op (new optional `values` field)
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillOp {
    name: String,
    value: Option<String>,
    values: Option<Vec<String>>,
    image_offset: Option<usize>,
    image_length: Option<usize>,
}
```

```rust
// crates/core/src/fill.rs — new Apply variant
/// Set /V to an array of strings and /I to the sorted array of indices,
/// then draw a multi-row highlight appearance on each widget.
ListBoxMulti {
    values: Vec<String>,
    indices: Vec<i64>,
    options: Vec<String>,
    ap: ApInputs,
},
```

```rust
// crates/core/src/forms.rs — Multiselect flag (Ff bit 22)
pub(crate) fn is_multiselect(ff: i64) -> bool {
    ff & (1 << 21) != 0
}
```

### Steps

- [ ] **1.1 Failing test: multi-value fill writes `/V` array + sorted `/I` on a multiselect list box.**

  There is no multiselect fixture in the corpus, so build one in-test by setting the Multiselect flag (`1 << 21`) on the existing single-select list box field of `FICHA` and round-tripping it. Add this helper + test to the `tests` module of `crates/core/src/fill.rs`. (`beneficiario.estado_civil` is a `Ch` field with `/Opt` entries `["Soltero","Casado","Divorciado","Viudo"]` in the fixture; confirm the exact option strings by reading the field once with `crate::forms::read_fields_json` if they differ, and adjust the asserted values accordingly.)

  ```rust
  /// Load FICHA, set the Multiselect Ff bit on `field_name`, return new bytes.
  fn with_multiselect(bytes: &[u8], field_name: &str) -> Vec<u8> {
      use lopdf::Document;
      let mut doc = Document::load_mem(bytes).unwrap();
      let (id, _) = find_field(&doc, field_name).unwrap();
      let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
      let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
      d.set("Ff", Object::Integer(ff | (1 << 21)));
      let mut out = Vec::new();
      doc.save_to(&mut out).unwrap();
      out
  }

  /// Read a field's /V and /I directly from the saved document.
  fn reparse_v_i(bytes: &[u8], field_name: &str) -> (Vec<String>, Vec<i64>) {
      let doc = Document::load_mem(bytes).unwrap();
      let (_, field) = find_field(&doc, field_name).unwrap();
      let v: Vec<String> = field
          .get(b"V")
          .unwrap()
          .as_array()
          .unwrap()
          .iter()
          .map(|o| {
              let b = o.as_str().unwrap();
              crate::forms::read_text_string(b)
          })
          .collect();
      let i: Vec<i64> = field
          .get(b"I")
          .unwrap()
          .as_array()
          .unwrap()
          .iter()
          .map(|o| o.as_i64().unwrap())
          .collect();
      (v, i)
  }

  #[test]
  fn multiselect_fill_sets_v_array_and_sorted_i() {
      let base = with_multiselect(FICHA, "beneficiario.estado_civil");
      // Provide values out of /Opt order; expect /I sorted ascending.
      let ops = r#"[{"name":"beneficiario.estado_civil","values":["Viudo","Casado"]}]"#;
      let out = fill_fields_json(&base, ops, &[]).unwrap();
      let (v, i) = reparse_v_i(&out, "beneficiario.estado_civil");
      assert_eq!(v, vec!["Viudo".to_string(), "Casado".to_string()]);
      // "Casado" is index 1, "Viudo" is index 3 in /Opt -> sorted [1, 3].
      assert_eq!(i, vec![1, 3]);
      Document::load_mem(&out).unwrap();
  }
  ```

  If `crate::forms` has no `read_text_string` helper, decode inline instead: `String::from_utf8_lossy(b).trim_start_matches('\u{feff}').to_string()` is too lossy for UTF-16BE, so prefer comparing against the raw PDFDocEncoded/literal form by checking `field.get(b"V")` is an `Object::Array` whose elements are `Object::String` and assert the array length is 2 and `/I == [1, 3]`. Pick whichever assertion is reliable for the fixture; the load-bearing checks are: `/V` is an Array of 2 strings and `/I == [1, 3]`.

- [ ] **1.2 Run the test, expect failure.**

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml multiselect_fill_sets_v_array_and_sorted_i
  ```
  Expect a compile error (`FillOp` has no `values`, no `ListBoxMulti` variant) or, once those exist, a panic that `/V` is not an array.

- [ ] **1.3 Implement: add `values` to `FillOp` and `is_multiselect` to forms.rs.**

  In `crates/core/src/fill.rs`, add `values: Option<Vec<String>>` to the `FillOp` struct (between `value` and `image_offset`).

  In `crates/core/src/forms.rs`, add the helper next to `classify`:
  ```rust
  /// True when a choice field carries the Multiselect flag (Ff bit 22).
  pub(crate) fn is_multiselect(ff: i64) -> bool {
      ff & (1 << 21) != 0
  }
  ```

- [ ] **1.4 Implement: `resolve` handles multi-value, validates, rejects bad cases.**

  In `crates/core/src/fill.rs`, rework the value branch of `resolve`. After the image branch, before reading `op.value`, handle a present `op.values`:

  ```rust
  // Multi-value fills are only legal on a multiselect list box.
  if let Some(values) = &op.values {
      if op.value.is_some() {
          return Err(format!(
              "field {} op cannot contain both value and values",
              op.name
          ));
      }
      if kind != "listbox" || !forms::is_multiselect(ff) {
          return Err(format!(
              "field {} does not accept multiple values (not a multi-select list box)",
              op.name
          ));
      }
      let options: Vec<String> = dict
          .get(b"Opt")
          .and_then(|o| o.as_array())
          .map(|a| a.iter().map(forms::opt_export).collect())
          .unwrap_or_default();
      let mut indices = Vec::with_capacity(values.len());
      for v in values {
          match dropdown_index(dict, v) {
              Some(i) => indices.push(i),
              None => {
                  return Err(format!("'{}' is not a valid option for {}", v, op.name));
              }
          }
      }
      indices.sort_unstable();
      return Ok(Resolved {
          field_id,
          apply: Apply::ListBoxMulti {
              values: values.clone(),
              indices,
              options,
              ap: ap_inputs(doc, field_id, dict, &op.name)?,
          },
      });
  }
  ```

  Add the `ListBoxMulti` variant to the `Apply` enum (signature above). Add it to the `touched_appearance` `matches!` in `fill_fields_json` so `NeedAppearances` is cleared:
  ```rust
  Apply::Text { .. }
      | Apply::Dropdown { .. }
      | Apply::ListBoxMulti { .. }
      | Apply::Signature { .. }
  ```

- [ ] **1.5 Implement: `apply` writes `/V` array + sorted `/I` array + appearance.**

  In `crates/core/src/fill.rs`, add an arm to the `match &r.apply` in `apply`:
  ```rust
  Apply::ListBoxMulti {
      values,
      indices,
      options,
      ap,
  } => {
      {
          let d = field_dict_mut(inc, r.field_id)?;
          let v_arr: Vec<Object> = values.iter().map(|s| text_string(s)).collect();
          d.set("V", Object::Array(v_arr));
          let i_arr: Vec<Object> = indices.iter().map(|i| Object::Integer(*i)).collect();
          d.set("I", Object::Array(i_arr));
      }
      draw_listbox_multi_appearances(inc, options, indices, ap)?;
  }
  ```

  Add the appearance writer (mirrors `draw_appearances`, but uses the multi-row content builder added in Task 2; for Task 1 add a temporary stub that calls `text_appearance_content` with an empty string so the crate compiles, then replace it in Task 2). To keep Task 1 self-contained and passing, implement the real loop now but call a placeholder content builder that you will introduce in Task 2 — OR implement Task 2's builder first. Recommended: implement the writer here calling `appearance::listbox_multi_content`, and write the builder in Task 2 before running. If you prefer strict per-task green, add `appearance::listbox_multi_content` as a minimal stub now (returns `/Tx BMC q Q EMC`) and flesh it out in Task 2.

  ```rust
  /// Build and attach a multi-row highlight `/AP/N` on each widget.
  fn draw_listbox_multi_appearances(
      inc: &mut IncrementalDocument,
      options: &[String],
      indices: &[i64],
      ap: &ApInputs,
  ) -> Result<(), String> {
      let encoded: Vec<Vec<u8>> = options.iter().map(|s| appearance::encode_winansi(s)).collect();
      let selected: Vec<bool> = (0..options.len() as i64)
          .map(|i| indices.contains(&i))
          .collect();
      for wb in &ap.widgets {
          let w = wb.rect[2] - wb.rect[0];
          let h = wb.rect[3] - wb.rect[1];
          let content = appearance::listbox_multi_content(
              &encoded,
              &selected,
              ap.da.size,
              w,
              h,
              &ap.da.color,
              &ap.font,
          );
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

  Add the minimal stub to `crates/core/src/appearance.rs` if not yet writing Task 2:
  ```rust
  /// PLACEHOLDER — replaced in Task 2 with real multi-row rendering.
  #[allow(clippy::too_many_arguments)]
  pub fn listbox_multi_content(
      _options: &[Vec<u8>],
      _selected: &[bool],
      _da_size: f32,
      _box_w: f32,
      _box_h: f32,
      _color: &str,
      _font: &str,
  ) -> Vec<u8> {
      b"/Tx BMC q Q EMC".to_vec()
  }
  ```

- [ ] **1.6 Run the test, expect pass.**

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml multiselect_fill_sets_v_array_and_sorted_i
  ```

- [ ] **1.7 Failing test: reject multi-value on a single-select list box.**

  ```rust
  #[test]
  fn rejects_multivalue_on_single_select_listbox() {
      // estado_civil WITHOUT the Multiselect flag set.
      let ops = r#"[{"name":"beneficiario.estado_civil","values":["Casado","Viudo"]}]"#;
      let err = fill_fields_json(FICHA, ops, &[]).unwrap_err();
      assert!(err.contains("does not accept multiple values"), "got: {err}");
  }
  ```

- [ ] **1.8 Failing test: reject an invalid option in a multi-value fill.**

  ```rust
  #[test]
  fn rejects_invalid_option_in_multivalue_fill() {
      let base = with_multiselect(FICHA, "beneficiario.estado_civil");
      let ops = r#"[{"name":"beneficiario.estado_civil","values":["Casado","Nope"]}]"#;
      let err = fill_fields_json(&base, ops, &[]).unwrap_err();
      assert!(err.contains("not a valid option"), "got: {err}");
  }
  ```

- [ ] **1.9 Run both rejection tests, expect pass.** (Both are satisfied by 1.4's validation; if either fails, fix `resolve`.)

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml --   rejects_multivalue_on_single_select_listbox rejects_invalid_option_in_multivalue_fill
  ```

- [ ] **1.10 Run the full Rust suite + clippy, expect green.**

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml
  cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings
  ```

- [ ] **1.11 Commit.**

  ```
  git add crates/core/src/fill.rs crates/core/src/forms.rs
  git commit -m "feat(fill): multi-value /V + /I for multiselect list boxes

  Add an optional values field to the wire op (backward-compatible with
  single value), classify the Multiselect Ff bit, validate every option,
  reject multi-value on non-multiselect fields, and write /V as a string
  array with a sorted /I index array.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 2 — Multi-row highlight appearance builder (Rust)

Replace the placeholder with a real content stream: one text line per option, top-aligned, each selected row backed by a filled light-blue rectangle drawn before its text.

**Files:**
- Modify `crates/core/src/appearance.rs` (real `listbox_multi_content` + unit test).

**Interfaces (exact signature):**

```rust
// crates/core/src/appearance.rs
/// Build the content stream for a multi-select list box appearance: one row per
/// option from top to bottom; each selected row gets a filled highlight
/// rectangle drawn behind its text. `selected[i]` toggles option `i`.
#[allow(clippy::too_many_arguments)]
pub fn listbox_multi_content(
    options: &[Vec<u8>],
    selected: &[bool],
    da_size: f32,
    box_w: f32,
    box_h: f32,
    color: &str,
    font: &str,
) -> Vec<u8>;
```

### Steps

- [ ] **2.1 Failing test: the appearance highlights selected rows and draws every option.**

  Add to the `tests` module of `crates/core/src/appearance.rs`:
  ```rust
  #[test]
  fn listbox_multi_highlights_selected_rows() {
      let options = vec![b"ES".to_vec(), b"EN".to_vec(), b"PT".to_vec()];
      let selected = vec![true, false, true];
      let content = listbox_multi_content(&options, &selected, 0.0, 100.0, 60.0, "0 g", "Helv");
      let s = String::from_utf8_lossy(&content);
      // Marked content + save/restore framing.
      assert!(s.starts_with("/Tx BMC q"));
      assert!(s.trim_end().ends_with("EMC"));
      // Every option is drawn.
      assert!(s.contains("(ES) Tj"), "got: {s}");
      assert!(s.contains("(EN) Tj"), "got: {s}");
      assert!(s.contains("(PT) Tj"), "got: {s}");
      // Two selected rows -> two highlight rectangles filled with the blue rg.
      let blue = "0.60 0.75 0.85 rg";
      assert_eq!(s.matches(blue).count(), 2, "expected 2 highlights in: {s}");
      assert_eq!(s.matches(" re").count(), 2, "expected 2 rectangles in: {s}");
  }
  ```

- [ ] **2.2 Run the test, expect failure** (placeholder returns `/Tx BMC q Q EMC` only).

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml listbox_multi_highlights_selected_rows
  ```

- [ ] **2.3 Implement the real builder.**

  Replace the placeholder `listbox_multi_content` in `crates/core/src/appearance.rs` with:
  ```rust
  /// Build the content stream for a multi-select list box appearance: one row per
  /// option from top to bottom; each selected row gets a filled highlight
  /// rectangle drawn behind its text. `selected[i]` toggles option `i`.
  #[allow(clippy::too_many_arguments)]
  pub fn listbox_multi_content(
      options: &[Vec<u8>],
      selected: &[bool],
      da_size: f32,
      box_w: f32,
      box_h: f32,
      color: &str,
      font: &str,
  ) -> Vec<u8> {
      // Row height: honor a positive DA size, else a sane default capped by box.
      let line = if da_size > 0.0 { da_size } else { MAX_AUTO };
      let row_h = (line + 2.0).max(MIN_AUTO + 2.0);
      let mut out = Vec::new();
      out.extend_from_slice(b"/Tx BMC q ");

      // 1) Highlight rectangles for selected rows (painted first, behind text).
      for (i, &sel) in selected.iter().enumerate() {
          if !sel {
              continue;
          }
          // Top-aligned: row 0 sits just under the top edge.
          let y = box_h - row_h * (i as f32 + 1.0);
          out.extend_from_slice(
              format!(
                  "0.60 0.75 0.85 rg {:.2} {:.2} {:.2} {:.2} re f ",
                  PAD,
                  y,
                  (box_w - 2.0 * PAD).max(0.0),
                  row_h
              )
              .as_bytes(),
          );
      }

      // 2) Text for every option, top to bottom.
      out.extend_from_slice(b"BT ");
      out.extend_from_slice(format!("/{font} {line:.2} Tf {color} ").as_bytes());
      for (i, opt) in options.iter().enumerate() {
          let baseline = box_h - row_h * (i as f32) - line;
          let escaped = escape_pdf_literal(opt);
          out.extend_from_slice(format!("{PAD:.2} {baseline:.2} Td (").as_bytes());
          out.extend_from_slice(&escaped);
          out.extend_from_slice(b") Tj ");
          // Reset text matrix for the next absolute Td (Td is relative).
          out.extend_from_slice(format!("{:.2} {:.2} Td ", -PAD, -(box_h - row_h * (i as f32) - line)).as_bytes());
      }
      out.extend_from_slice(b"ET Q EMC");
      out
  }
  ```

  Note on the `Td` resets: `Td` moves relative to the current line start. To keep the math simple and correct, after each `Tj` we undo the last `Td` so the next iteration's absolute `Td` lands correctly. If this two-step approach reads awkwardly, the equivalent and cleaner form is to emit one `Tm` (text matrix) per row instead:
  ```rust
      for (i, opt) in options.iter().enumerate() {
          let baseline = box_h - row_h * (i as f32) - line;
          let escaped = escape_pdf_literal(opt);
          out.extend_from_slice(format!("1 0 0 1 {PAD:.2} {baseline:.2} Tm (").as_bytes());
          out.extend_from_slice(&escaped);
          out.extend_from_slice(b") Tj ");
      }
  ```
  Prefer the `Tm` form (no relative-offset bookkeeping). Use it and drop the `Td` reset lines. The test in 2.1 only asserts on `(opt) Tj`, the blue `rg`, and ` re` counts, so either form passes; the `Tm` form is the maintainable choice.

- [ ] **2.4 Run the test, expect pass.**

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml listbox_multi_highlights_selected_rows
  ```

- [ ] **2.5 Strengthen the end-to-end Rust test from Task 1 to assert the appearance.**

  Extend `multiselect_fill_sets_v_array_and_sorted_i` (or add a sibling test) to read the widget `/AP/N` stream via the existing `ap_content` helper and assert it highlights the selected rows:
  ```rust
  #[test]
  fn multiselect_fill_generates_highlight_appearance() {
      let base = with_multiselect(FICHA, "beneficiario.estado_civil");
      let ops = r#"[{"name":"beneficiario.estado_civil","values":["Viudo","Casado"]}]"#;
      let out = fill_fields_json(&base, ops, &[]).unwrap();
      let doc = Document::load_mem(&out).unwrap();
      let ap = ap_content(&doc, "beneficiario.estado_civil").expect("AP/N present");
      assert!(ap.contains("0.60 0.75 0.85 rg"), "no highlight: {ap}");
      assert_eq!(ap.matches(" re").count(), 2, "expected 2 highlights: {ap}");
      assert!(ap.contains("(Casado) Tj"), "missing option text: {ap}");
  }
  ```
  Note: `ap_content` lives in the `fill.rs` tests module. If the option text is stored UTF-16/escaped differently in the fixture, relax the `(Casado) Tj` assertion to just `Tj` presence; the load-bearing assertions are the two highlight rectangles and the blue `rg`.

- [ ] **2.6 Run the full Rust suite + clippy, expect green.**

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml
  cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings
  ```

- [ ] **2.7 Commit.**

  ```
  git add crates/core/src/appearance.rs crates/core/src/fill.rs
  git commit -m "feat(appearance): multi-row highlight for multiselect list boxes

  Render one row per option top-to-bottom; draw a light-blue filled
  rectangle behind each selected row before its text.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 3 — TS `selectMultiple` + `multiSelect` flag + schema wrapper + exports (TS test)

Expose the Multiselect flag to TypeScript, add `PdfListBox.selectMultiple(values)` that queues a `values` op, throw when used on a single-select list box, and narrow the typed form.

**Files:**
- Modify `crates/core/src/forms.rs` (add `multi_select` to `FieldInfo` + populate it).
- Modify `src/forms/form.ts` (`FieldInfo.multiSelect`).
- Modify `src/forms/fields.ts` (`FillOp` union gains a `values` variant; `PdfListBox.selectMultiple`).
- Modify `src/forms/schema.ts` (`FieldMeta.multiSelect`; typed `getListBox` stays single-select, document that `selectMultiple` is a runtime-guarded method).
- Modify `src/core/errors.ts` (new `MultiSelectError`).
- Modify `src/index.ts` and `src/index.browser.ts` (export `MultiSelectError`).
- Modify `tests/listbox.test.ts`.

**Interfaces (exact signatures):**

```rust
// crates/core/src/forms.rs — FieldInfo gains a flag
#[serde(rename = "multiSelect")]
pub multi_select: bool,
// populated in describe_field:
multi_select: classify(&ft, ff) == "listbox" && is_multiselect(ff),
```

```ts
// src/forms/fields.ts — op union gains a multi-value variant
export type FillOp =
  | { name: string; value: string }
  | { name: string; values: string[] }
  | { name: string; image: Uint8Array };

// src/forms/fields.ts — new method on PdfListBox
selectMultiple(values: Opt[]): void;
```

```ts
// src/core/errors.ts
export class MultiSelectError extends PdfError {
  constructor(readonly field: string) {
    super(`list box '${field}' is single-select; use select() instead of selectMultiple()`);
  }
}
```

### Steps

- [ ] **3.1 Implement: expose `multiSelect` from Rust** (no separate Rust test; covered by TS round-trip).

  In `crates/core/src/forms.rs`, add the field to `FieldInfo` (after `exported`) and populate it in `describe_field`:
  ```rust
  // struct FieldInfo { ... after `pub exported: bool,` }
  #[serde(rename = "multiSelect")]
  pub multi_select: bool,
  ```
  ```rust
  // in describe_field's FieldInfo { ... } literal:
  multi_select: field_type == "listbox" && is_multiselect(ff),
  ```
  Rebuild WASM so TS picks it up:
  ```
  source ~/.cargo/env
  bun run build:wasm
  ```

- [ ] **3.2 Failing test: `selectMultiple` queues a `values` op; `select` still queues `value`.**

  Update `tests/listbox.test.ts`. Extend `listboxInfo()` to accept a `multiSelect` flag and add tests:
  ```ts
  function listboxInfo(multiSelect = false): FieldInfo {
    return {
      name: "preferencias.idioma",
      type: "listbox",
      value: null,
      states: [],
      options: ["ES", "EN", "PT"],
      readOnly: false,
      required: false,
      exported: true,
      maxLength: null,
      multiSelect,
      widgets: [],
    };
  }

  test("PdfListBox.selectMultiple queues a values op on a multi-select list box", () => {
    const queue = new FillQueue();
    new PdfListBox(listboxInfo(true), queue).selectMultiple(["ES", "PT"]);
    expect(queue.length).toBe(1);
    expect(JSON.parse(queue.toPayload().opsJson)).toEqual([
      { name: "preferencias.idioma", values: ["ES", "PT"] },
    ]);
  });

  test("PdfListBox.selectMultiple rejects an unknown option", () => {
    const lb = new PdfListBox(listboxInfo(true), new FillQueue());
    expect(() => lb.selectMultiple(["ES", "DE"])).toThrow(InvalidOptionError);
  });

  test("PdfListBox.selectMultiple throws on a single-select list box", () => {
    const lb = new PdfListBox(listboxInfo(false), new FillQueue());
    expect(() => lb.selectMultiple(["ES", "PT"])).toThrow(MultiSelectError);
  });
  ```
  Add `MultiSelectError` to the imports at the top of the file:
  ```ts
  import { InvalidOptionError, MultiSelectError } from "../src/core/errors.ts";
  ```

- [ ] **3.3 Run the test, expect failure.**

  ```
  bun test tests/listbox.test.ts
  ```
  Expect failures: `multiSelect` missing on `FieldInfo`, `selectMultiple` undefined, `MultiSelectError` not exported.

- [ ] **3.4 Implement: `FieldInfo.multiSelect`, `FillOp` union, `selectMultiple`, `MultiSelectError`.**

  `src/core/errors.ts` — add the class (signature above), placed after `MissingOnStateError`.

  `src/forms/form.ts` — add to the `FieldInfo` interface (after `maxLength`):
  ```ts
    /** True only for multi-select list boxes (the Multiselect choice flag). */
    multiSelect: boolean;
  ```

  `src/forms/fields.ts` — widen the `FillOp` type union (signature above), import `MultiSelectError`, and add the method to `PdfListBox`:
  ```ts
    /**
     * Select multiple list-box options by their real export values.
     *
     * Only valid for multi-select list boxes (the PDF Multiselect flag). The
     * queued values are written as the field's `/V` array and `/I` index array
     * when `doc.save()` is called.
     *
     * @param values - Export values, each one of `options`.
     * @throws `MultiSelectError` when this list box is single-select.
     * @throws `InvalidOptionError` when any value is not a valid option.
     *
     * @example
     * ```ts
     * form.getListBox("person.languages").selectMultiple(["ES", "EN"]);
     * ```
     */
    selectMultiple(values: Opt[]): void {
      if (!this.info.multiSelect) {
        throw new MultiSelectError(this.info.name);
      }
      if (this.info.options.length) {
        for (const v of values) {
          if (!this.info.options.includes(v)) {
            throw new InvalidOptionError(this.info.name, "listbox", v, this.info.options);
          }
        }
      }
      this.queue.push({ name: this.info.name, values: [...values] });
      this.info.value = values.join(", ");
    }
  ```
  Update the `FillQueue.toPayload` wire map only if needed: `values` ops have no image, so the existing `if (!("image" in op)) return op;` already passes them through unchanged. Confirm by reading `toPayload`.

  Update `PdfListBox`'s class doc comment to drop "single-select in this version".

- [ ] **3.5 Implement: typed schema wrapper + exports.**

  `src/forms/schema.ts` — add `multiSelect: boolean;` to `FieldMeta`, and update the `getListBox` doc comment in `TypedPdfForm` to note `selectMultiple` is runtime-guarded (the typed wrapper does not split single vs multi at the type level since `FieldMeta` does not encode it as a distinct `FieldType`; the runtime guard in `selectMultiple` is the safety net).

  `src/index.ts` and `src/index.browser.ts` — export `MultiSelectError` from `./core/errors.js` (add it to the existing error re-export block; grep for `InvalidOptionError` to find it).

- [ ] **3.6 Run the listbox test, expect pass.**

  ```
  bun test tests/listbox.test.ts
  ```

- [ ] **3.7 Add an end-to-end TS round-trip test (build + reload).**

  Add to `tests/listbox.test.ts` (or a dedicated `tests/listbox-multi-e2e.test.ts`) a test that loads a PDF whose list box has the Multiselect flag set, calls `selectMultiple`, saves, reloads, and asserts both values are present. The corpus has no multiselect fixture, so generate one at test time: load `FICHA`, flip the `Ff` bit on `beneficiario.estado_civil` via the low-level API, save, then drive the public TS API.

  If the TS layer exposes no low-level `Ff` editor, instead build the fixture once in Rust (Task 1's `with_multiselect`) and write it to `tests/fixtures/generated/ficha-multiselect-listbox.pdf` via a small `#[test]`-gated or `xtask`-style emitter, OR construct a minimal multiselect PDF with lopdf. Simplest reliable path: add a `cargo test`-driven fixture emitter that writes `tests/fixtures/generated/ficha-multiselect-listbox.pdf`, then load it from the TS test:
  ```ts
  import { PdfDocument } from "../src/index.ts";

  test("selectMultiple round-trips both values", async () => {
    const bytes = new Uint8Array(
      await Bun.file("tests/fixtures/generated/ficha-multiselect-listbox.pdf").arrayBuffer(),
    );
    const doc = await PdfDocument.load(bytes);
    const form = doc.getForm();
    form.getListBox("beneficiario.estado_civil").selectMultiple(["Casado", "Viudo"]);
    const out = await doc.save();

    const reloaded = await PdfDocument.load(out);
    const field = reloaded.getForm().getField("beneficiario.estado_civil");
    // Multi-value /V is reported by the reader as a single string today; the
    // load-bearing assertion is that both export values survived the round trip.
    expect(field?.value ?? "").toContain("Casado");
    expect(field?.value ?? "").toContain("Viudo");
  });
  ```
  NOTE — BLOCKER RISK: the reader (`describe_field`) calls `value_to_string` on `/V`, which today handles a single string, not an array. If `/V` is an array, `field.value` may be `null` or empty, breaking the round-trip assertion. Mitigation: in this task, also update `value_to_string` (or `describe_field`'s `value` extraction) to render an `Object::Array` of strings as a comma-joined string, and add a Rust test for it. This keeps the reader honest about multi-value list boxes. Do this in step 3.8 before the e2e test passes.

- [ ] **3.8 Implement: reader renders array `/V` as joined string; emit the fixture.**

  In `crates/core/src/forms.rs`, update the `/V` extraction in `describe_field`:
  ```rust
  let value = d.get(b"V").ok().and_then(|o| match o {
      Object::Array(a) => {
          let parts: Vec<String> = a.iter().filter_map(value_to_string).collect();
          if parts.is_empty() { None } else { Some(parts.join(", ")) }
      }
      other => value_to_string(other),
  });
  ```
  Add a Rust test in `forms.rs` building a tiny doc with an array `/V` and asserting the joined string. Add a fixture emitter test (gated, writes only if missing) or a `build.rs`/script that produces `tests/fixtures/generated/ficha-multiselect-listbox.pdf` from `FICHA` with the Multiselect bit set on `beneficiario.estado_civil`. Run it so the file exists before the TS e2e test.

- [ ] **3.9 Build WASM + run all TS tests, expect green.**

  ```
  source ~/.cargo/env
  bun run build:wasm
  cargo test --manifest-path crates/core/Cargo.toml
  cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings
  bun test
  ```

- [ ] **3.10 Commit.**

  ```
  git add crates/core/src/forms.rs src/forms/fields.ts src/forms/form.ts src/forms/schema.ts src/core/errors.ts src/index.ts src/index.browser.ts tests/listbox.test.ts tests/fixtures/generated/ficha-multiselect-listbox.pdf
  git commit -m "feat(ts): PdfListBox.selectMultiple for multi-select list boxes

  Expose the Multiselect flag as FieldInfo.multiSelect, add a
  selectMultiple(values) method that queues a values op, throw
  MultiSelectError on single-select list boxes, render array /V as a
  joined string in the reader, and export MultiSelectError.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 4 — Docs + CHANGELOG + version bump

Document multi-select support, remove the "single-select only" caveats, bump to `0.16.0`, and regenerate API docs if the repo regenerates them.

**Files:**
- Modify `CHANGELOG.md`, `README.md`, `package.json`, `crates/core/Cargo.toml`.
- Modify `docs/site/src/content/docs/reference/api.md`, `docs/site/src/content/docs/api-reference/classes/PdfForm.md`, `docs/site/src/content/docs/guides/filling-forms.md`, `docs/site/src/content/docs/migrating/from-pdf-lib.md`, `docs/migrating-from-pdf-lib.md`, `docs/api/classes/PdfListBox.md`, `docs/api/interfaces/TypedPdfForm.md`, `docs/api/classes/PdfForm.md` (the generated `docs/api/*` files: regenerate via `bun run docs` if available rather than hand-editing).
- Modify `skills/better-pdf/SKILL.md` if it documents list-box behavior (grep first).

### Steps

- [ ] **4.1 Bump version to 0.16.0.**

  Edit `package.json` `"version": "0.16.0"` and `crates/core/Cargo.toml` `version = "0.16.0"`. If `crates/wasm/Cargo.toml` exists with its own version, bump it too.

- [ ] **4.2 Update CHANGELOG.**

  Under a new `## [0.16.0] - 2026-06-20` section (move `## [Unreleased]` above it):
  ```
  ## [0.16.0] - 2026-06-20

  ### Added

  - Multi-select list boxes. `PdfListBox.selectMultiple(values)` fills a choice
    field that has the Multiselect flag set, writing `/V` as an array of export
    values and `/I` as the sorted array of selected indices, and generating an
    appearance that highlights every selected row. `FieldInfo.multiSelect` reports
    whether a list box is multi-select. Calling `selectMultiple` on a single-select
    list box throws `MultiSelectError`.

  ### Changed

  - The fill op wire schema gained an optional `values` array (single-value
    `value` fills are unchanged). The reader renders an array `/V` as a
    comma-joined string.
  ```

- [ ] **4.3 Remove "single-select" caveats.**

  Grep and replace every "single-select only" / "single-select in this version" mention:
  ```
  grep -rn "single-select" README.md docs/ src/ skills/
  ```
  - In `src/forms/fields.ts` and `src/forms/form.ts` doc comments: update `PdfListBox` / `getListBox` to mention `selectMultiple` exists for multi-select list boxes.
  - In `docs/site/.../guides/filling-forms.md`: replace the `// single-select in this version` comment and add a `selectMultiple` example.
  - In `docs/migrating/from-pdf-lib.md` and `docs/migrating-from-pdf-lib.md`: change `(single-select)` to note multi-select is supported via `selectMultiple`.

- [ ] **4.4 Regenerate API docs.**

  ```
  bun run docs
  ```
  If `bun run docs` is not configured or `docs/api/*` is gitignored, skip and rely on the source doc-comment edits. (Check `package.json` scripts for `docs`.)

- [ ] **4.5 Full verification.**

  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml
  cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings
  bun run build
  bun test
  ```

- [ ] **4.6 Commit.**

  ```
  git add -A
  git commit -m "docs: document multi-select list boxes; release 0.16.0

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

- [ ] **4.7 Merge to master** (per repo convention: merge finished branches locally, skip the merge/PR options menu). Do not push or tag unless asked.

---

## Notes on the multiselect fixture (read before Task 1)

The corpus has thin choice coverage and no multi-select list box. Two construction paths, in order of preference:

1. **Flip the flag on an existing field (used by this plan).** Load `FICHA`, set `Ff |= 1 << 21` on the `beneficiario.estado_civil` `Ch` field, and save. This reuses a real field with a real `/Opt` and `/DA`, so the appearance path is exercised end-to-end. The `with_multiselect` helper (step 1.1) does this in-test; step 3.8 emits the same bytes to `tests/fixtures/generated/ficha-multiselect-listbox.pdf` for the TS test.

2. **Build a minimal lopdf `Document`** with one `Ch` field, `Ff = 1 << 21`, `/Opt [(A)(B)(C)]`, a widget `/Rect`, and an AcroForm `/DR` font — only if path 1 proves unreliable (e.g. `estado_civil` turns out to be a dropdown, not a list box, in the fixture; verify its classified type first with `read_fields_json`).
