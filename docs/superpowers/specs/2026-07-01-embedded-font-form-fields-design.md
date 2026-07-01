# Embedded (CJK/Unicode) fonts on builder-created text fields — Design

**Date:** 2026-07-01
**Status:** Design — pending user approval
**Scope:** Tier 2 follow-up to `configurable-field-font` (2026-06-28). Embedded-font slice, builder-created plain + multiline text fields only.

## Summary

Let a `FormBuilder` text field render its value in an **embedded** TTF/OTF font
(`doc.embedFont(bytes)`), not just the 12 standard-14 WinAnsi fonts. This makes
non-Latin form values — CJK, and accented/extended Latin outside WinAnsi —
render correctly on created documents. It closes the #2 pdf-lib parity gap
(`form.updateFieldAppearances(customFont)`), for the create path.

The standard-14 slice already added a `font?: StandardFonts` option, a `/DR`
font registry, and reuses the WinAnsi appearance engine. This slice adds the
**Type0/CIDFontType2** counterpart: a CID-encoded appearance path, field-aware
subsetting, and a guard against the not-yet-supported fill path.

## Goals

- `addTextField(name, { font: <embedded PdfFont> })` on **plain and multiline**
  text fields renders the field value in that embedded font.
- Reuse the existing embedded-font engine (`crates/core/src/fonts/mod.rs`,
  `BuiltFont`, `build_embedded_font`) — no new font embedding, no new Type0
  object construction.
- `subset: true` (the `embedFont` default) "just works" for field values — no
  blank/`.notdef` glyphs.
- Standard-14 fields are behaviorally and byte-for-byte unchanged.
- No public API break: widen the existing `font` option's type only.

## Non-goals (this slice)

- **Comb, dropdown, and list-box fields with embedded fonts.** Rejected with a
  clear error; they keep standard-14 support. (Comb needs per-cell CID
  centering; choice fields add selection highlighting — deferred.)
- **Filling embedded-font fields through the form API** (`setText` etc.). This
  is the loaded-fill path (`fill.rs`), whose WinAnsi engine would mis-encode a
  Type0 font. Guarded with a throw; it is the seam for a future "embedded-font
  fill" slice (scope B). Setting the value at **build time** is the supported
  path.
- **Adding embedded-font fields to loaded documents** — out of scope, tracked
  separately with [[getform-created-docs-architecture]]'s "fields on loaded
  docs" follow-up.

## Background / current state

- **Embedded-font engine** (`crates/core/src/fonts/mod.rs`): `build_embedded_font`
  takes `EmbeddedFontInput { data, subset, used_chars: BTreeSet<char> }` and
  returns `(type0_object_id, BuiltFont { gid_for: HashMap<char, u16> })`. For a
  Type0 CIDFontType2 with Identity-H, CID = GID and text is shown as 2-byte
  big-endian GIDs. `measure_embedded(font, size, text)` and
  `wrap_embedded(font, size, avail_w, text)` measure/wrap by glyph advance.
- **Create path** (`crates/core/src/create.rs`): gathers `used_per_font` from
  `drawText` ops (~lines 600–609) and calls `build_embedded_font` (~624)
  **before** the field-build block (~1651+). Fields currently build their
  appearance with `text_appearance_content*` (WinAnsi) using
  `standard_14_widths` and a standard-14 `/DR` alias (`Helv`, `TiRo`, …). The
  `/DR` registry (~1651–1690) registers only standard-14 base fonts;
  Helvetica is always present because the AcroForm-level `/DA` references it.
- **Appearance engine** (`crates/core/src/appearance.rs`): WinAnsi builders
  `text_appearance_content` / `..._multiline` / `..._comb` take a `&FontWidths`
  and emit `(...) Tj`. `wrap_str(text, avail_w, measure)` already wraps a
  Unicode `&str` with a measure closure; `quad_offset(q, box_w, tw)` and
  `PAD` handle alignment.
- **Fill path** (`crates/core/src/fill.rs`): `text_field_appearance_inputs`
  (~584) resolves the DA font via `/DR/Font/<name>` (`font_ref`, `dr_font_dict`)
  and `resolve_widths`, then draws with the WinAnsi engine.
- **TS**: `embedFont()` returns a `PdfFont` carrying `[kFontId]` (the numeric
  index into the registered-fonts blob). `drawText` resolves the same id to
  resource `BPF<n>`. Field builders currently take `font?: StandardFonts` and
  thread it as a string via `applyTextStyle` → `def.font`.

## API

Widen the existing option on `addTextField` only:

```ts
/** Font for the field's value: a standard-14 name, or an embedded font from
 *  doc.embedFont(). Embedded fonts are supported on plain and multiline text
 *  fields only. Defaults to Helvetica. */
font?: StandardFonts | PdfFont;
```

- `addDropdown` / `addListBox` keep `font?: StandardFonts` (type-level
  rejection of `PdfFont`), backed by a runtime guard for untyped callers.
- Checkbox / radio / signature are unchanged (no `font`).
- Omitting `font` = Helvetica, exactly as today.

## Wire format & builder resolution (TypeScript)

The internal `WireTextField` (`form-builder.ts`) and the `FieldDef` union
(`schema.ts`) gain one optional field:

```ts
fontId?: number;   // embedded-font index; mutually exclusive with `font`
```

`addTextField` resolves `font`:

| `font` value | Wire result |
| --- | --- |
| `StandardFonts` string | `def.font = <name>` (existing path) |
| `PdfFont` **with** `kFontId` (embedded) | `def.fontId = font[kFontId]`; `def.font` unset |
| `PdfFont` **without** `kFontId` (a `getFont()` standard-14 handle) | `def.font = font.name` |

Validation, thrown synchronously at `addTextField`:

- Embedded font (`fontId` resolved) **+ `comb: true`** →
  `throw new PdfError("embedded fonts are supported on plain and multiline text fields only")`.
  Multiline is allowed.
- `addDropdown` / `addListBox` given a `PdfFont` with `kFontId` → same message.

`form-builder.ts` imports `PdfFont` and `kFontId`. The id comes straight off
the handle; the builder never touches the draw queue. `def.fontId = n`
references the same fonts blob `createDocument` receives, whose Type0 object is
resource `BPF<n>` — the exact convention `drawText` uses, so a field and a
`drawText` sharing one `PdfFont` share one embedded object (no double-embed).

## Rust create path

**New appearance functions** (`appearance.rs`), CID counterparts of the WinAnsi
builders:

```rust
pub fn text_appearance_content_embedded(
    text: &str, size: f32, box_w: f32, box_h: f32, q: i64,
    color: &str, font: &str, built: &BuiltFont, font_bytes: &[u8],
) -> Vec<u8>;

pub fn text_appearance_content_multiline_embedded(
    lines: &[&str], size: f32, box_w: f32, box_h: f32, q: i64,
    color: &str, font: &str, built: &BuiltFont, font_bytes: &[u8],
) -> Vec<u8>;
```

- Encode each char via `built.gid_for` to a 2-byte big-endian GID; emit a hex
  show string `<....> Tj` (Identity-H). Chars absent from `gid_for` are skipped
  (matching `drawText`).
- Single line: horizontal offset from `quad_offset(q, box_w, measure_embedded(font_bytes, size, text))`;
  vertical baseline identical to the WinAnsi single-line formula.
- Multiline: `lines` are pre-wrapped by the caller (via
  `wrap_str(value, avail_w, |s| measure_embedded(font_bytes, size, s))`); emit
  one show string per line, top-aligned, stepping by the same leading the
  WinAnsi multiline path uses.
- Hex is lowercase (`{:04x}`); the font op is `/{font} {size:.2} Tf {color}`,
  matching the WinAnsi format byte-for-byte except for the show string.

**Field wiring** (`create.rs`). For a text-field def with `font_id: Some(n)`:

- Resolve the already-built `BuiltFont` and its Type0 object id for font `n`.
- Appearance XObject `/Resources /Font << /BPF<n> <type0_ref> >>`, content from
  the embedded builder above (single or multiline by the field's `multiline`
  flag).
- Field `/DA (/BPF<n> <size> Tf <color>)`.
- Extend the `/DR` registry to add `/Font /BPF<n> → <type0_ref>` for each
  embedded font used by a field (alongside the standard-14 aliases). The
  AcroForm-level default `/DA (/Helv 0 Tf 0 g)` stays Helvetica; per-field
  `/DA` overrides it.

Standard-14 fields (`font_id` absent) take the unchanged WinAnsi path.

## Subsetting (field-aware glyph collection)

In the `used_per_font` collection loop (`create.rs`, before `build_embedded_font`):

- Additionally walk the text-field defs; for each with `font_id: Some(n)`, add
  the chars of its `value` **and** `defaultValue` into `used_per_font[n]`.
- Ensure a `BuiltFont` is built for **every** embedded font referenced by a
  draw op **or** a field — extend the build loop to iterate the full
  registered-fonts table, so a font used only by a field is still built (its
  `used_chars` coming purely from field values).

The same `BuiltFont.gid_for` drives both the subset and the appearance
encoding, so they cannot diverge. Inherited caveat: OpenType-CFF may fail to
subset; escape hatch is `embedFont(bytes, { subset: false })`, which then works
for fields too.

## Re-fill guard (Rust fill path)

In `text_field_appearance_inputs` (`fill.rs`, ~584), after resolving the DA
font's `/DR/Font/<name>` dictionary, inspect its `/Subtype`. If it is `/Type0`,
return an error instead of drawing through the WinAnsi engine:

```
filling embedded-font fields through the form API is not yet supported; set the
value at build time via createForm(). (field '<name>')
```

- One check covers every WinAnsi text/choice appearance redraw (`setText`,
  `setMultiline`/`setComb`/`setPassword`).
- Keys on `/Subtype /Type0`, not on the `BPF<n>` name, so it also strictly
  rejects any loaded PDF with a Type0 DA font — consistent with the project's
  reject-rather-than-mishandle stance.
- Non-appearance reads (`getField`, `.value`) are unaffected.
- Surfaces as a rejected `save()` (the fill op runs at save time).

## Errors

| Case | Where | Message |
| --- | --- | --- |
| Embedded font + `comb` | builder (TS) | `embedded fonts are supported on plain and multiline text fields only` |
| Embedded font on dropdown/listbox | builder (TS) | same |
| Re-fill embedded-font field via form API | `fill.rs` | `filling embedded-font fields through the form API is not yet supported; …` |
| Char with no glyph | appearance (Rust) | silently skipped (matches `drawText`) |
| OTF-CFF subset failure | `embedFont` | existing error; escape hatch `{ subset: false }` |

## Testing

TDD-red tests already authored (become this plan's acceptance criteria):

- **`tests/form-embedded-font.test.ts`** — plain + multiline render/round-trip;
  field-only-font subsetting; comb + dropdown rejection; re-fill rejects at
  `save()`; standard-14 unaffected.
- **`crates/core/src/appearance.rs`** (`embedded_field_appearance_tests`) —
  Identity-H GID hex encoding; no-glyph skipping; one show operator per line.

Additional coverage to add during implementation:

- Rust unit test: `fill.rs` guard returns `Err` for a field whose `/DR` DA font
  is `/Type0` (construct a minimal in-memory doc).
- Regression: Helvetica-only created forms stay byte-identical (existing
  `form-generation` tests remain green).

Fixtures: reuse `tests/fixtures/fonts/NotoSans-Regular.subset.ttf` — the Type0
encoding path is font-agnostic; a non-WinAnsi glyph exercises it without a large
CJK font.

## Docs

Update `docs/site/src/content/docs/reference/limitations.md`: the form-field
font bullet changes from "Embedded / non-Latin (CJK) fonts are not supported for
form-field values" to: embedded fonts are supported on **builder-created plain +
multiline text fields**, noting the comb/choice exclusion and the "set value at
build time; form-API re-fill not yet supported" boundary.

## Future work

- Embedded-font **fill** of loaded/materialized fields (scope B) — replaces the
  `fill.rs` guard with a real Type0 fill.
- Embedded fonts on comb and choice fields.
