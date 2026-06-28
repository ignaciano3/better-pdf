# Configurable standard-14 font for builder form fields

**Status:** Design approved — ready for implementation planning.
**Date:** 2026-06-28
**Scope:** Tier 2, item 1 (configurable form-field font), standard-14 slice only.

## Problem

Form fields created by the `FormBuilder` always render their value in
Helvetica. `create.rs` hardcodes the font resource name `"Helv"` in every
field's `/DA` string and appearance stream, and builds the AcroForm `/DR` with a
single shared Helvetica font object. The documented limitation reads: *"field
values always render in Helvetica — the font family is fixed and not
configurable."* Size, color, and alignment are already configurable; font
family is the remaining gap.

This spec covers the **standard-14** slice: let a builder field use any of the
12 standard text fonts (Helvetica / Times / Courier × regular / bold / italic /
bold-italic). Embedded TTF/OTF fonts (for CJK / non-Latin) are explicitly out of
scope and tracked as a future slice.

## Background / current state

- `crates/core/src/create.rs`, inside the `if !fields.is_empty()` block:
  - `let helv = doc.add_object(font_dict("Helvetica"));`
  - `let widths = crate::appearance::helvetica_widths();`
  - Every text / choice / comb field passes the literal `"Helv"` (resource
    name), `helv` (object ref), and `widths` into the appearance builders, and
    writes `/DA (/Helv <size> Tf <color>)`.
  - The AcroForm is built with `/DR << /Font << /Helv <helv> >> >>` and
    `/DA (/Helv 0 Tf 0 g)`.
- The appearance engine (`crates/core/src/appearance.rs`) is simple-font /
  WinAnsi based: `text_appearance_content`, `text_appearance_content_multiline`,
  `text_appearance_content_comb`, and `listbox_multi_content` all take a font
  **resource name** plus a `FontWidths` table and emit `(...) Tj` WinAnsi byte
  strings. `standard_14_widths(base_font)` already returns the correct width
  table for any standard-14 base font. **No new appearance code is required.**
- `build_appearance_xobject(content, w, h, font_name, font_ref)` wires the
  XObject `/Resources /Font /<font_name> <font_ref>`.
- The **fill path** (`fill.rs`) already resolves a loaded field's `/DA` font
  against `/DR` (`font_ref`, `resolve_widths` via `standard_14_widths`), so a
  loaded field whose `/DA` already names a standard-14 font renders in that
  font. This feature therefore touches the **builder only**.
- TS exposes a `StandardFonts` enum (`src/generate/fonts.ts`) with exactly the
  12 text fonts; Symbol and ZapfDingbats are intentionally omitted (non-Latin
  encodings incompatible with the WinAnsi text model). `drawText` already
  accepts `font?: StandardFonts | PdfFont`.

## API surface

`TextFieldOptions` and `ChoiceOptions` gain:

```ts
/** Standard-14 font for the field's value. Defaults to Helvetica.
 *  Embedded (PdfFont) fonts are not supported for form fields. */
font?: StandardFonts;
```

- Applies to `addTextField` (including `multiline` and `comb` fields),
  `addDropdown`, and `addListBox`.
- Not added to checkbox, radio, or signature fields (they render a mark or an
  image, not text).
- Default (omitted) = `Helvetica`, preserving today's behavior.

## `/DR` font registry (approach A)

Register only the distinct base fonts actually used by fields, each once, under
a fixed deterministic alias:

| BaseFont               | alias  | BaseFont              | alias  |
| ---------------------- | ------ | --------------------- | ------ |
| Helvetica              | `Helv` | Times-Roman           | `TiRo` |
| Helvetica-Bold         | `HeBo` | Times-Bold            | `TiBo` |
| Helvetica-Oblique      | `HeOb` | Times-Italic          | `TiIt` |
| Helvetica-BoldOblique  | `HeBO` | Times-BoldItalic      | `TiBI` |
| Courier                | `Cour` | Courier-Bold          | `CoBo` |
| Courier-Oblique        | `CoOb` | Courier-BoldOblique   | `CoBO` |

- `Helv` (Helvetica) is **always** registered, because the AcroForm-level `/DA`
  default `(/Helv 0 Tf 0 g)` references it. A form whose fields are all
  Helvetica (or omit `font`) produces **byte-identical** output to today.
- Each distinct non-Helvetica font used is added to `/DR/Font` under its alias
  and its object reused across all fields that select it.

## Builder wiring (`create.rs`)

Inside the `if !fields.is_empty()` block:

1. **Pre-pass:** scan the field defs for text / choice fields, collect the set
   of base fonts used (default Helvetica when `font` is `None`). Build a
   registry `HashMap<&'static str /*base*/, (&'static str /*alias*/, ObjectId)>`,
   always seeding Helvetica → (`Helv`, helv_id). For each other base in the set,
   `doc.add_object(font_dict(base))` and insert under its alias.
2. **Per-field:** resolve the field's font to `(alias, font_ref, widths)` where
   `widths = standard_14_widths(base).unwrap()`. Replace the hardcoded
   `"Helv"` / `helv` / `widths` arguments in:
   - `text_appearance_content` / `text_appearance_content_multiline` /
     `text_appearance_content_comb` (text fields),
   - `listbox_multi_content` is not used at create time; choice fields use
     `text_appearance_content` — confirm and use the resolved trio there too,
   - `build_appearance_xobject(..., alias, font_ref)`,
   - the `/DA` string: `/<alias> <size> Tf <color_op>`.
3. The AcroForm `/DR` is built from the registry (always includes `Helv`); the
   AcroForm-level `/DA` stays `(/Helv 0 Tf 0 g)`.

## Wire format + validation

- **TS** (`form-builder.ts`): `WireTextField` and `WireChoice` gain
  `font?: string` (the base font name from the enum value). The builder
  validates synchronously that the value is a member of `StandardFonts`,
  throwing `RangeError` otherwise — matching the existing builder validation
  style (validates before `save()`).
- **Rust** (`create.rs`): `FieldDef::Text` and `FieldDef::Choice` gain
  `#[serde(default)] font: Option<String>`. Defense-in-depth: an unknown base
  font (i.e. `standard_14_index(base).is_none()`) returns `Err`.

## Read-back behavior

`FieldInfo.fontName` continues to report the field's `/DA` font **resource
name** — now the alias (e.g. `"TiRo"`) rather than always `"Helv"`. This matches
how it already surfaces `"Helv"` today. Mapping an alias back to the friendly
base font name is a reader concern and is **out of scope** (future follow-up).

## Backward compatibility

- Fields that omit `font` (or set Helvetica) are unaffected: same `/DR`, same
  `/DA`, same appearance stream → byte-identical output.
- No change to the fill path, the reader, or any non-builder API.

## Testing

**Rust (`create.rs` tests):**
- Text field with `font: "Times-Roman"` → `/DA` contains `/TiRo`,
  `/DR/Font/TiRo` BaseFont = `Times-Roman`, the appearance XObject `/Resources`
  references `TiRo`.
- Dropdown with `font: "Courier-Bold"` → `/CoBo` in `/DA` and `/DR`.
- Two fields with two different non-Helvetica fonts → both aliases present in
  `/DR/Font`, each font object registered once.
- Field omitting `font` → still `/Helv` (guards byte-compat).
- Unknown font name → `create_document_json` returns `Err`.

**TS (`form-generation.test.ts`):**
- Build a text field with `font: StandardFonts.TimesRoman`, reload, assert
  `getField(...).fontName === "TiRo"`.
- `addTextField({ font: "Bogus" as StandardFonts })` → `RangeError`.

## Docs

- `guides/creating-form-fields.mdx`: document the `font` option; update the
  `:::note[Field text is always Helvetica]` callout (it is no longer always
  Helvetica) and the per-field options.
- `reference/limitations.md`: revise the "Form-field text appearance" bullet —
  font family is now configurable among the standard-14 fonts; embedded/CJK
  fonts remain the limitation.
- `README.md`: mirror the limitation/feature wording.
- `CHANGELOG.md` + minor version bump.

## Non-goals (YAGNI)

- Embedded (`PdfFont`) / CJK / non-Latin fonts for form fields — separate future
  slice; the bigger work (a CID-based form-field appearance engine).
- Changing a loaded field's font (no setter on the field wrappers).
- A form-level default font on `createForm()`.
- Mapping the `/DA` alias back to a friendly base-font name in `FieldInfo`.
