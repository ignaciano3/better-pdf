# Embedded Fonts on Builder-Created Text Fields — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a `FormBuilder` plain/multiline text field render its value in an embedded (Type0/CIDFontType2) font from `doc.embedFont()`, so created documents can carry non-Latin (CJK / extended-Latin) form values.

**Architecture:** TS builder threads an embedded font's numeric id onto the text-field wire def (`fontId`). The Rust create path reuses the existing embedded-font engine (`BuiltFont`, `build_embedded_font`) to render a CID-encoded single-line appearance, wires the Type0 object into the field's `/DA`, appearance XObject, and the AcroForm `/DR`, and feeds field-value glyphs into the subset. The loaded-fill path (`fill.rs`) throws on Type0 DA fonts rather than mis-encoding them.

**Tech Stack:** TypeScript (Bun test runner, `bun test`), Rust WASM core (`cargo test` in `crates/core`), `bunx tsc --noEmit` for typecheck.

## Global Constraints

- **No public API break** — widen the existing `font` option to `StandardFonts | PdfFont` on `addTextField` only; no new option name. (Verbatim project rule: don't split the public `PdfDocument` type.)
- **Standard-14 fields unchanged** — the embedded path activates only when a text-field def has `fontId`. Helvetica-only created forms stay byte-identical; existing `form-generation` and `create` tests stay green.
- **Reuse the embedded-font engine** — no new Type0 construction; use `build_embedded_font` / `BuiltFont` / `measure_embedded` from `crates/core/src/fonts/mod.rs`.
- **Single-line appearance only** — matches the standard-14 create path (no build-time multiline wrap). Multiline fields carry the `/Ff` flag but render single-line.
- **CI gates clippy + tests** — run `cargo clippy` cleanliness implicitly (no `unwrap` on untrusted input beyond existing patterns); run `bun test` and `cargo test` per task.
- **Messages (verbatim):**
  - comb/choice + embedded → `embedded fonts are supported on plain and multiline text fields only`
  - re-fill embedded field → `filling embedded-font fields through the form API is not yet supported; set the value at build time via createForm().`

**Pre-authored TDD-red tests already in the working tree (uncommitted):**
- `tests/form-embedded-font.test.ts`
- `crates/core/src/appearance.rs` → `mod embedded_field_appearance_tests`

These are the acceptance criteria. Tasks below reference and commit them.

---

### Task 1: Embedded single-line appearance builder (Rust)

**Files:**
- Modify: `crates/core/src/appearance.rs` (add `text_appearance_content_embedded`; the test module `embedded_field_appearance_tests` already exists at end of file)

**Interfaces:**
- Consumes: `crate::fonts::BuiltFont { gid_for: HashMap<char, u16> }`, `crate::fonts::measure_embedded(font: &[u8], size: f32, text: &str) -> Result<f32, String>`, existing `quad_offset(q, box_w, tw)` and `const PAD`.
- Produces: `pub fn text_appearance_content_embedded(text: &str, size: f32, box_w: f32, box_h: f32, q: i64, color: &str, font: &str, built: &BuiltFont, font_bytes: &[u8]) -> Vec<u8>`.

- [ ] **Step 1: Confirm the failing tests exist and fail to compile**

The tests are already written in `crates/core/src/appearance.rs` (`mod embedded_field_appearance_tests`): `encodes_gids_as_identity_h_hex` and `skips_chars_without_a_glyph`.

Run: `cargo test -p better-pdf-core embedded_field_appearance --no-run`
Expected: FAIL — compile error, `cannot find function text_appearance_content_embedded`.

- [ ] **Step 2: Implement the function**

Add to `crates/core/src/appearance.rs`, next to `text_appearance_content` (after it, before the multiline builder). Mirror the WinAnsi single-line layout exactly, swapping the show string for Identity-H hex:

```rust
/// Single-line appearance content for an embedded (Type0/Identity-H) font.
/// Encodes each char to a 2-byte big-endian GID via `built.gid_for`; chars with
/// no glyph are skipped (matching `drawText`). Horizontal quad offset uses the
/// embedded font's measured advance; vertical baseline matches the WinAnsi
/// single-line builder.
#[allow(clippy::too_many_arguments)]
pub fn text_appearance_content_embedded(
    text: &str,
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    font: &str,
    built: &crate::fonts::BuiltFont,
    font_bytes: &[u8],
) -> Vec<u8> {
    // 2-byte big-endian GID per char with a glyph.
    let mut hex = String::new();
    for ch in text.chars() {
        if let Some(&gid) = built.gid_for.get(&ch) {
            write!(hex, "{gid:04x}").unwrap();
        }
    }
    let tw = crate::fonts::measure_embedded(font_bytes, size, text).unwrap_or(0.0);
    let tx = quad_offset(q, box_w, tw);
    let ty = ((box_h - size) / 2.0 + size * 0.2).max(PAD);
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    write!(out, "/{font} {size:.2} Tf {color} ").unwrap();
    write!(out, "{tx:.2} {ty:.2} Td <").unwrap();
    out.extend_from_slice(hex.as_bytes());
    out.extend_from_slice(b"> Tj ET Q EMC");
    out
}
```

Ensure `use std::fmt::Write;` is in scope in this file (it already is — `write!` is used by `text_appearance_content`).

- [ ] **Step 3: Run the unit tests to verify they pass**

Run: `cargo test -p better-pdf-core embedded_field_appearance`
Expected: PASS — `encodes_gids_as_identity_h_hex`, `skips_chars_without_a_glyph`.

- [ ] **Step 4: Run the full crate tests (no regressions)**

Run: `cargo test -p better-pdf-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/appearance.rs
git commit -m "feat(appearance): embedded (Type0) single-line field appearance builder"
```

---

### Task 2: Builder wire format, widened `font` option, validation (TypeScript)

**Files:**
- Modify: `src/generate/form-builder.ts` (`TextFieldOptions.font`, `WireTextField`, `addTextField`, `addDropdown`, `addListBox`, `applyTextStyle`)
- Modify: `src/forms/schema.ts` (the text-field variant of the `FieldDef` union — add `fontId?`)
- Test: `tests/form-embedded-font.test.ts` (already written)

**Interfaces:**
- Consumes: `PdfFont` and `kFontId` from `src/generate/font.js` (re-exports `kFontId` from `../core/internal.js`); `StandardFonts` from `./fonts.js`.
- Produces: text-field wire def optionally carries `fontId?: number` (mutually exclusive with `font: string`).

- [ ] **Step 1: Verify the relevant tests currently fail**

`tests/form-embedded-font.test.ts` is already written. The comb/dropdown rejection tests and the type-widening are what this task turns green.

Run: `bunx tsc --noEmit`
Expected: FAIL — `tests/form-embedded-font.test.ts` passes a `PdfFont` to `font`, which today is typed `StandardFonts`.

- [ ] **Step 2: Widen the option type and wire field**

In `src/generate/form-builder.ts`, add imports near the top:

```ts
import { PdfFont, kFontId } from "./font.js";
```

Change `TextFieldOptions.font` (both the interface at ~line 51 and any duplicate for choice is NOT changed — only text):

```ts
  /** Font for the field's value: a standard-14 name, or an embedded font from
   *  doc.embedFont(). Embedded fonts are supported on plain and multiline text
   *  fields only. Defaults to Helvetica. */
  font?: StandardFonts | PdfFont;
```

Add `fontId` to the `WireTextField` type (the internal wire def in this file):

```ts
  fontId?: number;
```

- [ ] **Step 3: Resolve the font in `addTextField`**

In `addTextField`, before `applyTextStyle`, resolve the font option into either a standard-14 name or an embedded id, and validate comb:

```ts
    // Resolve font: standard-14 name string, or an embedded PdfFont id.
    let embeddedFontId: number | undefined;
    if (opts.font instanceof PdfFont) {
      const id = opts.font[kFontId];
      if (id !== undefined) {
        embeddedFontId = id;
      } else {
        // A standard-14 PdfFont handle (from doc.getFont()); use its name.
        (opts as TextFieldOptions).font = opts.font.name as StandardFonts;
      }
    }
    if (embeddedFontId !== undefined && opts.comb) {
      throw new PdfError("embedded fonts are supported on plain and multiline text fields only");
    }
```

After building `def` (the `WireTextField`), attach the id and skip the standard-14 `font` path when embedded:

```ts
    if (embeddedFontId !== undefined) {
      def.fontId = embeddedFontId;
    } else {
      applyTextStyle(def, opts, name);
    }
```

Note: when embedded, `applyTextStyle`'s `font` handling is skipped, but `fontSize`/`align`/`textColor` must still apply. Split `applyTextStyle` so size/color/align always run and only the `font` name is gated — simplest: call `applyTextStyle` in both branches but have it ignore `opts.font` when it is a `PdfFont` (it already only reads `opts.font` as a `StandardFonts`; guard it):

```ts
// in applyTextStyle, replace the font block:
  if (opts.font !== undefined && !(opts.font instanceof PdfFont)) {
    // ...existing standard-14 validation + def.font = opts.font
  }
```

Then always call `applyTextStyle(def, opts, name)` and separately set `def.fontId` when embedded. (Pick whichever of these two shapes is cleaner in the actual code; the invariant is: embedded → `def.fontId` set, `def.font` unset; size/color/align always applied.)

Import `PdfError` if not already imported in this file.

- [ ] **Step 4: Reject embedded fonts on choice fields**

In `addDropdown` and `addListBox`, add a guard (their `font` option stays `StandardFonts`, but back it at runtime):

```ts
    if ((opts.font as unknown) instanceof PdfFont && ((opts.font as unknown as PdfFont)[kFontId] !== undefined)) {
      throw new PdfError("embedded fonts are supported on plain and multiline text fields only");
    }
```

- [ ] **Step 5: Add `fontId` to the schema `FieldDef` union**

In `src/forms/schema.ts`, add `fontId?: number;` to the text-field variant of the wire `FieldDef` type so the JSON sent to Rust is typed.

- [ ] **Step 6: Run the tests**

Run: `bunx tsc --noEmit`
Expected: PASS (types compile).

Run: `bun test tests/form-embedded-font.test.ts`
Expected: PARTIAL — `comb field rejects…`, `dropdown rejects…`, and `standard-14 fields are unaffected…` PASS; the render/subset/re-fill tests still FAIL (create path ignores `fontId`, no fill guard yet). This is expected at this task boundary.

Run: `bun test`
Expected: existing suites PASS (no regressions); only the not-yet-implemented embedded-render/re-fill tests in the new file fail.

- [ ] **Step 7: Commit**

```bash
git add src/generate/form-builder.ts src/forms/schema.ts tests/form-embedded-font.test.ts
git commit -m "feat(forms): accept embedded PdfFont on builder text fields (wire + validation)"
```

---

### Task 3: Create-path rendering + field-aware subsetting (Rust)

**Files:**
- Modify: `crates/core/src/create.rs` (the `FieldDef::Text` struct/variant to add `font_id`; the `used_per_font` pre-pass ~600–626; the text-field build block ~1718–1747; the `/DR` registry ~1673–1689)

**Interfaces:**
- Consumes: `text_appearance_content_embedded` (Task 1); `embedded_fonts: HashMap<usize, (ObjectId, BuiltFont)>` (already built in the pre-pass); `build_appearance_xobject`.
- Produces: text fields with `font_id: Some(n)` render a Type0 appearance and wire `/BPF<n>` into `/DA`, the appearance XObject, and `/DR`.

- [ ] **Step 1: Verify the render tests fail**

Run: `bun test tests/form-embedded-font.test.ts`
Expected: the three render/subset tests (`plain text field renders…`, `multiline text field…`, `subsetting is field-aware…`) FAIL — the field value does not render in the embedded font / the doc may error.

- [ ] **Step 2: Add `font_id` to the `FieldDef::Text` variant**

In `crates/core/src/create.rs`, find the `FieldDef` enum's `Text { … }` variant (fields include `font`, `align`, `font_size`, …). Add:

```rust
        #[serde(default, rename = "fontId")]
        font_id: Option<usize>,
```

Add `font_id,` to the destructuring in the `FieldDef::Text { … } =>` match arm (~1696–1718).

- [ ] **Step 3: Make subsetting field-aware in the pre-pass**

In the `used_per_font` block (~600–611), after the loop over `ops`, add a loop over `fields` that adds text-field value/default glyphs to the font they use:

```rust
        for field in fields {
            if let FieldDef::Text {
                font_id: Some(i),
                value,
                default_value,
                ..
            } = field
            {
                let set = used_per_font.entry(*i).or_default();
                if let Some(v) = value {
                    set.extend(v.chars());
                }
                if let Some(dv) = default_value {
                    set.extend(dv.chars());
                }
            }
        }
```

Because a field-only font now appears in `used_per_font`, the existing build loop (`ids = used_per_font.keys()`, ~613–626) builds it — so a font used only by a field is embedded. No further change needed to the build loop.

- [ ] **Step 4: Render the embedded appearance in the text-field build**

In the `FieldDef::Text` build block (~1719–1747), branch on `font_id` before the standard-14 path. Replace the appearance construction so that when `font_id` is set it uses the embedded builder and the Type0 object:

```rust
                let op = color_op(*text_color);
                let size = font_size.unwrap_or(12.0);
                let q = quadding(align);
                let val_str = value.clone().unwrap_or_default();

                let (content, font_alias_str, ap_font_ref) = if let Some(fid) = font_id {
                    let (type0_id, built) = &embedded_fonts[fid];
                    let alias = format!("BPF{fid}");
                    let fd = &font_descs[*fid];
                    let fbytes = &fonts[fd.offset..fd.offset + fd.length];
                    let content = crate::appearance::text_appearance_content_embedded(
                        &val_str, size, *width, *height, q, &op, &alias, built, fbytes,
                    );
                    (content, alias, *type0_id)
                } else {
                    let base_font = font.as_deref().unwrap_or("Helvetica");
                    let (font_alias, font_ref) = font_registry[base_font];
                    let widths = crate::appearance::standard_14_widths(base_font).unwrap();
                    let val_bytes = crate::appearance::encode_winansi(&val_str);
                    let content = if *comb {
                        crate::appearance::text_appearance_content_comb(
                            &val_bytes, size, *width, *height, max_length.unwrap_or(0), &op, font_alias, &widths,
                        )
                    } else {
                        crate::appearance::text_appearance_content(
                            &val_bytes, size, *width, *height, q, &op, font_alias, &widths,
                        )
                    };
                    (content, font_alias.to_string(), font_ref)
                };
                let ap_stream = crate::appearance::build_appearance_xobject(
                    content, *width, *height, &font_alias_str, ap_font_ref,
                );
                let ap_id = doc.add_object(Object::Stream(ap_stream));
```

Note `build_appearance_xobject`'s `font_name` param is `&str` — passing `&font_alias_str` (a `String`) works. If its signature is `&'static str`, change it to `&str` (it only formats the name into `/Resources`); update the standard-14 call site accordingly.

- [ ] **Step 5: Write the field's `/DA` with the embedded alias**

Where the field dict sets `/DA` (search the `FieldDef::Text` block for `set("DA"` / the `/DA` string built from `font_alias`), use `font_alias_str` for both the standard-14 and embedded cases so embedded fields get `(/BPF<n> <size> Tf <color>)`. Concretely the `/DA` literal is built from the alias + size + color; ensure it reads the unified `font_alias_str` computed above.

- [ ] **Step 6: Register embedded fonts in `/DR`**

After the standard-14 `/DR` font registry is built (~1689, after the `for base in &needed` loop), add every embedded font used by a text field:

```rust
    for field in fields {
        if let FieldDef::Text { font_id: Some(i), .. } = field {
            let alias = format!("BPF{i}");
            if !dr_fonts.has(alias.as_bytes()) {
                let (type0_id, _) = &embedded_fonts[i];
                dr_fonts.set(alias.as_bytes().to_vec(), Object::Reference(*type0_id));
            }
        }
    }
```

(Use whatever `Dictionary` "contains key" / "set" API `dr_fonts` exposes; `has`/`set` mirror the existing calls in this file.)

- [ ] **Step 7: Run the render tests**

Run: `bun test tests/form-embedded-font.test.ts`
Expected: render + multiline + subsetting tests PASS; re-fill test still FAILS (guard is Task 4).

- [ ] **Step 8: Run full crate + regression suites**

Run: `cargo test -p better-pdf-core`
Expected: PASS.

Run: `bun test`
Expected: PASS except the one re-fill test (Task 4). Existing `form-generation`/`create` tests green (standard-14 unchanged).

- [ ] **Step 9: Commit**

```bash
git add crates/core/src/create.rs
git commit -m "feat(create): render embedded-font text fields; field-aware subsetting"
```

---

### Task 4: Re-fill guard (Rust fill path)

**Files:**
- Modify: `crates/core/src/fill.rs` (`text_field_appearance_inputs` ~584; helpers `dr_font_dict` ~682)
- Test: add a Rust unit test in `crates/core/src/fill.rs`

**Interfaces:**
- Consumes: the resolved `/DR/Font/<name>` dictionary for a field's DA font.
- Produces: `text_field_appearance_inputs` returns `Err` when the DA font's `/Subtype` is `/Type0`.

- [ ] **Step 1: Write the failing unit test**

Add to the `#[cfg(test)]` module in `crates/core/src/fill.rs` (build a minimal doc whose field DA font is Type0, then attempt a fill):

```rust
    #[test]
    fn rejects_filling_a_type0_da_font_field() {
        // A created doc with a text field bound to an embedded (Type0) font.
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, FONT.len());
        let fields = r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20,"value":"A","fontId":0}]"#;
        let ops = r#"[{"op":"addPage","width":300,"height":300}]"#;
        let doc = crate::create::create_document_json(ops, &[], FONT, &fonts_json, fields).unwrap();

        // Attempt to re-fill the field through the fill path.
        let ops_json = r#"{"fill":[{"name":"n","kind":"text","value":"B"}]}"#;
        // fill_fields_json takes (data, ops_json, images); adjust to the real signature.
        let err = crate::fill::fill_fields_json(&doc, r#"[{"name":"n","kind":"text","value":"B"}]"#, &[])
            .unwrap_err();
        assert!(err.contains("not yet supported"), "got: {err}");
    }
```

Adjust the `fill_fields_json` call to its actual signature/ops shape (check the existing fill tests in this file for the exact JSON format). The essential assertion: filling a Type0-DA field returns an `Err` containing `not yet supported`.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p better-pdf-core rejects_filling_a_type0`
Expected: FAIL — currently the fill path draws WinAnsi bytes (no error, or a different error).

- [ ] **Step 3: Add the guard**

In `text_field_appearance_inputs` (`fill.rs` ~584), after resolving the DA font's `/DR` dictionary (the `dr_font_dict` helper, ~682, or inline where `font_ref`/`resolve_widths` are computed), inspect `/Subtype`:

```rust
    // Reject Type0 (embedded/composite) DA fonts: the WinAnsi engine below would
    // mis-encode them. Filling embedded-font fields is a future slice.
    if let Some(dict) = dr_font_dict(doc, acro, &da.font) {
        if matches!(dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()), Some(b"Type0")) {
            return Err(format!(
                "filling embedded-font fields through the form API is not yet supported; set the value at build time via createForm(). (field '{name}')"
            ));
        }
    }
```

(Match the exact `dr_font_dict` return type — it returns the font dictionary for a DA font name. If it returns a reference, resolve it to a `&Dictionary` first, mirroring how `resolve_widths` reads it.)

- [ ] **Step 4: Run the unit test + the TS re-fill test**

Run: `cargo test -p better-pdf-core rejects_filling_a_type0`
Expected: PASS.

Run: `bun test tests/form-embedded-font.test.ts`
Expected: ALL tests in the file PASS (including `re-filling an embedded-font field via the form API throws`).

- [ ] **Step 5: Full suites (no regressions)**

Run: `cargo test -p better-pdf-core`
Expected: PASS.

Run: `bun test`
Expected: PASS (whole suite).

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/fill.rs
git commit -m "feat(fill): reject filling Type0-DA-font fields (embedded-font fill is a future slice)"
```

---

### Task 5: Docs + final verification

**Files:**
- Modify: `docs/site/src/content/docs/reference/limitations.md` (the form-field font bullet, ~lines 63–68)

**Interfaces:**
- Consumes: the shipped behavior from Tasks 1–4. No production code.

- [ ] **Step 1: Update the limitations bullet**

In `docs/site/src/content/docs/reference/limitations.md`, replace the form-field-value font bullet (the one stating "Embedded / non-Latin (CJK) fonts are not supported for form-field values — only the standard-14 WinAnsi fonts") with:

```markdown
- **Form-field text appearance:** field values render in a **standard-14 font**
  — selectable per field via the builder `font` option (Helvetica / Times /
  Courier families), with `fontSize`, `textColor`, and `align` also
  configurable (and `checkStyle` for the selected mark of checkboxes and
  radios). **Embedded fonts (CJK / non-Latin) are supported on builder-created
  plain and multiline text fields** via `addTextField({ font: doc.embedFont(bytes) })`;
  the value is CID-encoded into a Type0 appearance and the glyphs are subset.
  - **Caveat — build-time only.** Embedded-font field values must be set through
    the builder (`value` / `defaultValue`); re-filling an embedded-font field
    through the form API (`getForm().getTextField(...).setText(...)`) throws
    `filling embedded-font fields through the form API is not yet supported`.
  - **Caveat — comb and choice fields.** Comb text fields, dropdowns, and list
    boxes accept standard-14 fonts only; passing an embedded font throws
    `embedded fonts are supported on plain and multiline text fields only`.
```

- [ ] **Step 2: Full verification**

Run: `bun test`
Expected: PASS (whole suite, including `tests/form-embedded-font.test.ts`).

Run: `cargo test -p better-pdf-core`
Expected: PASS.

Run: `bunx tsc --noEmit`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add docs/site/src/content/docs/reference/limitations.md
git commit -m "docs: embedded fonts supported on builder-created text fields"
```

---

## Self-Review Notes

- **Spec coverage:** API widening + wire format (Task 2); embedded appearance builder (Task 1); create-path wiring `/DA`+XObject+`/DR` (Task 3); field-aware subsetting + build-all-referenced-fonts (Task 3); re-fill guard (Task 4); comb/choice rejection (Task 2); docs (Task 5). Single-line-only decision reflected (no multiline builder). Non-goals (choice/comb embedded, loaded-fill, loaded-doc field creation) intentionally have no tasks.
- **Type consistency:** `text_appearance_content_embedded(text, size, box_w, box_h, q, color, font, built: &BuiltFont, font_bytes)` used identically in Task 1 (impl + tests) and Task 3 (call site). Wire field `fontId` (TS) ↔ `font_id` (Rust, `rename = "fontId"`) consistent across Tasks 2–4. Alias `BPF<n>` consistent across create-path DA/XObject/DR (Task 3) and the drawText convention.
- **Placeholder scan:** none — every code/test step is concrete. Two steps note "match the actual signature" for `build_appearance_xobject`'s `&str`/`&'static str` and `dr_font_dict`'s return type / `fill_fields_json`'s ops JSON; these are deliberate "confirm against the real code" instructions, not placeholders, since the surrounding code is visible to the implementer.
```
