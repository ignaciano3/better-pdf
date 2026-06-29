# Configurable Standard-14 Field Font Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `FormBuilder` text/choice fields render their value in any standard-14 font (not just Helvetica), via a `font?: StandardFonts` option.

**Architecture:** The builder collects the distinct standard-14 fonts used across fields, registers each once in the AcroForm `/DR/Font` under a deterministic short alias (Helvetica stays `/Helv`), and threads the per-field `(alias, font object ref, width table)` into the existing WinAnsi appearance engine and `/DA` string. No new appearance code; embedded fonts are out of scope.

**Tech Stack:** Rust (lopdf) core compiled to WASM, TypeScript wrapper, Bun test runner, `cargo test`.

## Global Constraints

- Standard-14 text fonts only (the 12 in `StandardFonts`); Symbol/ZapfDingbats excluded.
- Helvetica-only / `font`-omitted forms must produce **byte-identical** output to today (no regressions): `/Helv` is always registered first.
- Default font when omitted = `Helvetica`.
- `FieldInfo.fontName` reports the `/DA` resource alias (e.g. `"TiRo"`), unchanged in mechanism from how it reports `"Helv"` today.
- WASM must be rebuilt (`bun run build:wasm`) after Rust changes before TS tests run against it.
- Follow existing builder validation style: throw `RangeError` synchronously.
- Spec: `docs/superpowers/specs/2026-06-28-configurable-field-font-design.md`.

---

### Task 1: Rust core — `/DR` font registry + thread font through text & choice fields

**Files:**
- Modify: `crates/core/src/create.rs` (FieldDef `Text`/`Choice` variants; the `if !fields.is_empty()` block ~1452–1921; add `da_font_alias` helper)
- Test: `crates/core/src/create.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::appearance::standard_14_widths(base) -> Option<FontWidths>`, `font_dict(base) -> Dictionary`, `crate::appearance::text_appearance_content{,_comb}`, `build_appearance_xobject`.
- Produces: `FieldDef::Text` and `FieldDef::Choice` each carry `font: Option<String>`; the AcroForm `/DR/Font` contains one entry per distinct field font keyed by alias; each field's `/DA` and `/AP` use its font's alias.

- [ ] **Step 1: Add the alias helper and its test**

The `tests` module uses `use super::*;`, so the module-private `da_font_alias` is directly reachable from tests.

Add this free function near the other helpers in `create.rs` (e.g. just above the `create_document_json`-side helpers, module scope):

```rust
/// Map a standard-14 base font name to its deterministic AcroForm /DR resource
/// alias. Returns `None` for names that are not standard-14 text fonts.
fn da_font_alias(base: &str) -> Option<&'static str> {
    Some(match base {
        "Helvetica" => "Helv",
        "Helvetica-Bold" => "HeBo",
        "Helvetica-Oblique" => "HeOb",
        "Helvetica-BoldOblique" => "HeBO",
        "Courier" => "Cour",
        "Courier-Bold" => "CoBo",
        "Courier-Oblique" => "CoOb",
        "Courier-BoldOblique" => "CoBO",
        "Times-Roman" => "TiRo",
        "Times-Bold" => "TiBo",
        "Times-Italic" => "TiIt",
        "Times-BoldItalic" => "TiBI",
        _ => return None,
    })
}
```

Add this test in the `tests` module:

```rust
#[test]
fn da_font_alias_maps_all_standard_14() {
    assert_eq!(da_font_alias("Helvetica"), Some("Helv"));
    assert_eq!(da_font_alias("Times-Roman"), Some("TiRo"));
    assert_eq!(da_font_alias("Courier-Bold"), Some("CoBo"));
    assert_eq!(da_font_alias("Times-BoldItalic"), Some("TiBI"));
    assert_eq!(da_font_alias("Symbol"), None);
}
```

- [ ] **Step 2: Run the test — expect PASS (helper) and confirm it compiles**

Run: `cd crates/core && cargo test da_font_alias_maps_all_standard_14`
Expected: PASS. (This step verifies the helper + names; the behavior tests come next.)

- [ ] **Step 3: Add `font` to the `FieldDef::Text` and `FieldDef::Choice` variants**

In `create.rs`, add to the `Text` variant struct fields (alongside `align`):

```rust
        #[serde(default)]
        font: Option<String>,
```

Add the identical field to the `Choice` variant (alongside its `align`).

- [ ] **Step 4: Write failing behavior tests**

Add to the `tests` module:

```rust
#[test]
fn text_field_uses_requested_standard_14_font() {
    let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":100,"height":20,"font":"Times-Roman"}]"#;
    let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], &[], "[]", f).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let w = get_first_field_dict(&doc);
    let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
    assert!(da.contains("/TiRo"), "DA should reference the Times alias, got: {da}");

    // /DR/Font has TiRo -> a font dict with BaseFont Times-Roman.
    let cat = doc.catalog().unwrap();
    let acro = match cat.get(b"AcroForm").unwrap() {
        Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        Object::Dictionary(d) => d,
        _ => panic!("AcroForm not dict/ref"),
    };
    let dr = acro.get(b"DR").unwrap().as_dict().unwrap();
    let fonts = dr.get(b"Font").unwrap().as_dict().unwrap();
    let tiro = match fonts.get(b"TiRo").unwrap() {
        Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        Object::Dictionary(d) => d,
        _ => panic!("TiRo not dict/ref"),
    };
    let base = tiro.get(b"BaseFont").unwrap().as_name().unwrap();
    assert_eq!(&String::from_utf8_lossy(base), "Times-Roman");
}

#[test]
fn choice_field_uses_requested_font() {
    let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":80,"height":20,"combo":true,"options":["a","b"],"font":"Courier-Bold"}]"#;
    let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], &[], "[]", f).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let w = get_first_field_dict(&doc);
    let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
    assert!(da.contains("/CoBo"), "DA should reference the Courier-Bold alias, got: {da}");
}

#[test]
fn default_font_is_helvetica_alias() {
    let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":100,"height":20}]"#;
    let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], &[], "[]", f).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let w = get_first_field_dict(&doc);
    let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
    assert!(da.contains("/Helv"), "default DA should use Helv, got: {da}");
}

#[test]
fn distinct_fonts_each_registered_once_in_dr() {
    let f = r#"[
        {"type":"text","name":"a","page":0,"x":0,"y":0,"width":100,"height":20,"font":"Times-Roman"},
        {"type":"text","name":"b","page":0,"x":0,"y":40,"width":100,"height":20,"font":"Times-Roman"},
        {"type":"text","name":"c","page":0,"x":0,"y":80,"width":100,"height":20,"font":"Courier"}
    ]"#;
    let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], &[], "[]", f).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let cat = doc.catalog().unwrap();
    let acro = match cat.get(b"AcroForm").unwrap() {
        Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        Object::Dictionary(d) => d,
        _ => panic!("AcroForm not dict/ref"),
    };
    let fonts = acro.get(b"DR").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap();
    assert!(fonts.has(b"Helv"), "Helv always present");
    assert!(fonts.has(b"TiRo"), "TiRo present");
    assert!(fonts.has(b"Cour"), "Cour present");
    // TiRo used twice but registered once: exactly these three entries.
    assert_eq!(fonts.iter().count(), 3, "expected exactly Helv/TiRo/Cour");
}

#[test]
fn unknown_field_font_is_rejected() {
    let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":100,"height":20,"font":"Comic Sans"}]"#;
    let r = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], &[], "[]", f);
    assert!(r.is_err(), "unknown font must be rejected");
}
```

- [ ] **Step 5: Run the tests — verify they FAIL**

Run: `cd crates/core && cargo test text_field_uses_requested_standard_14_font choice_field_uses_requested_font distinct_fonts_each_registered_once_in_dr unknown_field_font_is_rejected`
Expected: compile error (unused `font` field is fine) then FAIL — DA still says `/Helv`, `/DR` only has `Helv`, unknown font not rejected.

- [ ] **Step 6: Build the `/DR` font registry**

In `create.rs`, replace the top of the `if !fields.is_empty()` block. Change:

```rust
    let acro_form_ref = if !fields.is_empty() {
        // Shared Helv font for all field appearances
        let helv = doc.add_object(Object::Dictionary(font_dict("Helvetica")));
        let widths = crate::appearance::helvetica_widths();
```

to:

```rust
    let acro_form_ref = if !fields.is_empty() {
        // Collect the distinct standard-14 fonts used by text/choice fields
        // (Helvetica is always present, since the form-level /DA references it).
        // Validate each up front so an unknown font fails before any object is
        // written. Register each font once in /DR/Font under its alias.
        let mut needed: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        needed.insert("Helvetica");
        for field in &fields {
            let base = match field {
                FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } => {
                    font.as_deref().unwrap_or("Helvetica")
                }
                _ => continue,
            };
            if da_font_alias(base).is_none() {
                return Err(format!("unknown field font: {base}"));
            }
            needed.insert(base);
        }

        // alias -> font object id, and base-font -> (alias, object id). Helvetica
        // is added first so Helvetica-only forms stay byte-identical to before.
        let mut dr_fonts = Dictionary::new();
        let mut font_registry: std::collections::HashMap<&str, (&'static str, lopdf::ObjectId)> =
            std::collections::HashMap::new();
        let helv = doc.add_object(Object::Dictionary(font_dict("Helvetica")));
        dr_fonts.set("Helv", Object::Reference(helv));
        font_registry.insert("Helvetica", ("Helv", helv));
        for base in &needed {
            if *base == "Helvetica" {
                continue;
            }
            let alias = da_font_alias(base).unwrap();
            let fid = doc.add_object(Object::Dictionary(font_dict(base)));
            dr_fonts.set(alias, Object::Reference(fid));
            font_registry.insert(base, (alias, fid));
        }
```

(`helv` and `font_registry` are now the source of per-field font resolution. The old `widths` binding is removed — each field resolves its own widths below.)

- [ ] **Step 7: Resolve and use the per-field font in the `Text` arm**

In the `FieldDef::Text { … }` arm, destructure the new `font` field (add `font,` to the pattern), then immediately after `let q = quadding(align);` insert:

```rust
                    let base_font = font.as_deref().unwrap_or("Helvetica");
                    let (font_alias, font_ref) = font_registry[base_font];
                    let widths = crate::appearance::standard_14_widths(base_font).unwrap();
```

Replace the two `"Helv"` literals and `&widths` in the `text_appearance_content_comb` / `text_appearance_content` calls with `font_alias` and `&widths` (the `widths` now refers to the per-field binding), and the `build_appearance_xobject(content, *width, *height, "Helv", helv)` call with `build_appearance_xobject(content, *width, *height, font_alias, font_ref)`. Replace the `/DA` line:

```rust
                    field_dict.set("DA", Object::string_literal(format!("/{font_alias} {size} Tf {op}")));
```

- [ ] **Step 8: Resolve and use the per-field font in the `Choice` arm**

In the `FieldDef::Choice { … }` arm, add `font,` to the pattern. After `let q = quadding(align);` insert the same three `base_font` / `(font_alias, font_ref)` / `widths` lines as Step 7. Replace the `"Helv"` literal in its `text_appearance_content` call with `font_alias` and its `&widths` argument with `&widths` (per-field), the `build_appearance_xobject(..., "Helv", helv)` with `(..., font_alias, font_ref)`, and the `/DA` line with the `format!("/{font_alias} {size} Tf {op}")` form.

- [ ] **Step 9: Build the AcroForm `/DR` from the registry**

Replace the AcroForm `/DR` construction:

```rust
            "DR" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "Helv" => Object::Reference(helv)
                })
            }),
```

with:

```rust
            "DR" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dr_fonts)
            }),
```

(The form-level `"DA" => "/Helv 0 Tf 0 g"` line stays unchanged.)

- [ ] **Step 10: Run the new tests + full crate suite — expect PASS**

Run: `cd crates/core && cargo test`
Expected: all pass, including the five new behavior tests and the existing create/DA tests (`default_da_uses_size_12`, etc.). 0 warnings.

- [ ] **Step 11: Commit**

```bash
git add crates/core/src/create.rs
git commit -m "feat(create): render form fields in any standard-14 font

Add an optional per-field font (FieldDef::Text/Choice). The builder
collects the distinct standard-14 fonts used, registers each once in the
AcroForm /DR under a deterministic alias (Helvetica stays /Helv), and
threads the alias + width table into the /DA and appearance stream.
Helvetica-only forms are unchanged.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: TS builder option + validation + wire threading

**Files:**
- Modify: `src/generate/form-builder.ts` (`TextFieldOptions`, `ChoiceOptions`, `WireTextField`, `WireChoice`, `applyTextStyle`, import `StandardFonts`)
- Test: `tests/form-generation.test.ts`

**Interfaces:**
- Consumes: `StandardFonts` enum from `./fonts.js`; the Rust `font: Option<String>` wire field from Task 1.
- Produces: `addTextField`/`addDropdown`/`addListBox` accept `font?: StandardFonts`; invalid values throw `RangeError`; the value flows to the wire as `font: <enum string>`.

- [ ] **Step 1: Rebuild the WASM so TS tests exercise Task 1**

Run: `bun run build:wasm`
Expected: `✨ Done` and `pkg ready`.

- [ ] **Step 2: Write failing TS tests**

Add to `tests/form-generation.test.ts` (it already imports `PdfDocument, PageSizes, PdfError` and defines `buildAndReload`). Add `StandardFonts` to the import from `../src/index.ts`:

```ts
describe("form-generation: field font", () => {
  test("text field font round-trips as the DA alias", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("t", {
        page: 0,
        x: 50,
        y: 700,
        width: 200,
        height: 20,
        font: StandardFonts.TimesRoman,
      });
    });
    expect(reloaded.getForm().getField("t")?.fontName).toBe("TiRo");
  });

  test("dropdown font round-trips as the DA alias", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addDropdown("d", {
        page: 0,
        x: 50,
        y: 650,
        width: 200,
        height: 20,
        options: ["a", "b"] as const,
        font: StandardFonts.CourierBold,
      });
    });
    expect(reloaded.getForm().getField("d")?.fontName).toBe("CoBo");
  });

  test("omitting font defaults to Helvetica (Helv alias)", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("t", {
        page: 0, x: 50, y: 700, width: 200, height: 20,
      });
    });
    expect(reloaded.getForm().getField("t")?.fontName).toBe("Helv");
  });

  test("an unknown font string throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const fb = doc.createForm();
    expect(() =>
      fb.addTextField("t", {
        page: 0, x: 10, y: 10, width: 100, height: 20,
        font: "Comic Sans" as StandardFonts,
      }),
    ).toThrow(RangeError);
  });
});
```

- [ ] **Step 3: Run the tests — verify they FAIL**

Run: `bun test tests/form-generation.test.ts`
Expected: FAIL — `font` is not yet a known option (no validation, not on the wire); `fontName` is `"Helv"` for the Times/Courier cases.

- [ ] **Step 4: Add the `font` option to `TextFieldOptions` and `ChoiceOptions`**

Add an import near the top of `form-builder.ts`:

```ts
import { StandardFonts } from "./fonts.js";
```

Add to `TextFieldOptions` (after `fontSize?`):

```ts
  /** Standard-14 font for the field's value. Defaults to Helvetica. Embedded
   * (PdfFont) fonts are not supported for form fields. */
  font?: StandardFonts;
```

Add the identical property to `ChoiceOptions`.

- [ ] **Step 5: Add `font` to the wire types**

Add `font?: string;` to `interface WireTextField` (after `fontSize?`) and to `interface WireChoice`.

- [ ] **Step 6: Validate and thread the font in `applyTextStyle`**

Replace `applyTextStyle` with a version that also validates + copies `font`:

```ts
function applyTextStyle(
  def: { align?: FieldAlign; fontSize?: number; font?: string },
  opts: { align?: FieldAlign; fontSize?: number; font?: StandardFonts },
  label: string,
): void {
  if (opts.align !== undefined) def.align = opts.align;
  if (opts.fontSize !== undefined) {
    assertPositive(opts.fontSize, `${label}.fontSize`);
    def.fontSize = opts.fontSize;
  }
  if (opts.font !== undefined) {
    if (!Object.values(StandardFonts).includes(opts.font)) {
      throw new RangeError(`${label}.font is not a standard-14 font: ${String(opts.font)}`);
    }
    def.font = opts.font;
  }
}
```

(`addTextField` calls `applyTextStyle(def, opts, name)` at `form-builder.ts:340` and the choice path at `:483`, so no per-method change is needed — the new `font` handling flows through automatically.)

- [ ] **Step 7: Run the tests — expect PASS**

Run: `bun test tests/form-generation.test.ts`
Expected: all PASS.

- [ ] **Step 8: Typecheck + full suite**

Run: `bun run typecheck && bun test`
Expected: typecheck clean; full suite green (0 fail).

- [ ] **Step 9: Commit**

```bash
git add src/generate/form-builder.ts tests/form-generation.test.ts
git commit -m "feat(forms): add standard-14 font option to builder text/choice fields

addTextField/addDropdown/addListBox accept font?: StandardFonts, validated
against the standard-14 set and threaded to the wire. Defaults to Helvetica.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Docs + changelog + version bump

**Files:**
- Modify: `docs/site/src/content/docs/guides/creating-form-fields.mdx`
- Modify: `docs/site/src/content/docs/reference/limitations.md`
- Modify: `README.md`
- Modify: `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`, `crates/core/Cargo.lock`

**Interfaces:** none (docs only).

- [ ] **Step 1: Document the `font` option in `creating-form-fields.mdx`**

In the per-field options list, add a bullet (under Text and note it applies to choice fields too):

```md
- **Font** — `font` selects the field value's typeface from the standard-14
  fonts (`StandardFonts` enum: Helvetica / Times / Courier × regular / bold /
  italic / bold-italic). Defaults to Helvetica. Applies to text, dropdown, and
  list-box fields. Embedded (`PdfFont`) fonts are not supported for form fields.
```

Replace the `:::note[Field text is always Helvetica]` callout body so it no longer claims the family is fixed:

```md
:::note[Field text fonts]
Field values render in one of the standard-14 fonts (`font`, default Helvetica).
Size (`fontSize`), color (`textColor`), and alignment (`align`) are also
configurable. Embedded/CJK fonts are not supported for form-field values.
:::
```

- [ ] **Step 2: Revise the `limitations.md` appearance bullet**

Replace the `**Form-field text appearance:**` bullet body with:

```md
- **Form-field text appearance:** field values render in a **standard-14 font**
  — selectable per field via the builder `font` option (Helvetica / Times /
  Courier families), with `fontSize`, `textColor`, and `align` also
  configurable. **Embedded / non-Latin (CJK) fonts are not supported for
  form-field values** — only the standard-14 WinAnsi fonts.
```

- [ ] **Step 3: Mirror the wording in `README.md`**

Find the README limitations bullet about form-field font (search `Helvetica`) and update it to match the limitations.md wording above (standard-14 selectable; embedded/CJK not supported).

- [ ] **Step 4: Add the CHANGELOG entry under `[Unreleased]`**

```md
### Added

- **Configurable standard-14 field font.** `addTextField`, `addDropdown`, and
  `addListBox` accept `font?: StandardFonts` to render the field value in any of
  the 12 standard text fonts (Helvetica / Times / Courier families). Each
  distinct font is registered once in the AcroForm `/DR`. Defaults to Helvetica;
  embedded/CJK fonts remain unsupported for form fields.
```

- [ ] **Step 5: Cut the release**

Insert `## [1.6.0] - 2026-06-28` between `## [Unreleased]` and the new `### Added`. Bump `package.json` `"version"` to `1.6.0`, `crates/core/Cargo.toml` `version` to `1.6.0`, then run `cd crates/core && cargo build` to refresh `Cargo.lock`.

- [ ] **Step 6: Verify build + suite, then commit**

Run: `bun run build:wasm && bun run typecheck && bun test && (cd crates/core && cargo test)`
Expected: all green.

```bash
git add docs README.md CHANGELOG.md package.json crates/core/Cargo.toml crates/core/Cargo.lock
git commit -m "docs(forms): document configurable field font; release 1.6.0

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Notes for the implementer

- The `font_registry` borrows `&str` from `fields`; both the registry-building loop and the field-building loop take immutable borrows of `fields`, which is fine.
- `standard_14_widths("Helvetica")` returns the same table as `helvetica_widths()`, so the Helvetica path stays byte-identical.
- `get_first_field_dict`, `Document::catalog`, and `create_document_json` already exist and are used by neighboring tests in `create.rs` — follow those patterns.
- Choice fields at create time use `text_appearance_content` (not `listbox_multi_content`, which is fill-only).
