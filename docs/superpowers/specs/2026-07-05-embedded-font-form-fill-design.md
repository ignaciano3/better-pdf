# Embedded-font form fill (CJK/Unicode field values)

**Date:** 2026-07-05
**Status:** Approved design, pending implementation plan
**Ships as:** 1.11.0 (contains a documented behavioral change to `drawText`)

## Problem

Form-field values can only render in standard-14 WinAnsi fonts. Filling a field whose
`/DA` references a Type0 font throws at save, and there is no way to fill any field
with CJK or other non-WinAnsi text. This is the single most user-visible gap versus
pdf-lib (`field.updateAppearances(customFont)`), and the seam was deliberately left
when builder-created embedded-font fields shipped (2026-07-01): `fill.rs` guards the
Type0 case with "filling embedded-font fields through the form API is not yet
supported".

## Scope

- **In:** `setText` / `setDefaultText` with an explicit embedded font on **plain and
  multiline text fields of any origin** — loaded from a PDF, builder-created
  standard-14, or builder-created embedded-font.
- **Out:** comb fields, dropdowns, listboxes (stay rejected, matching the create-path
  guard); auto-reusing a field's existing Type0 font (subsetted originals commonly
  lack the needed glyphs); WinAnsi missing-char policy changes.

## API (TypeScript)

```ts
const font = await doc.embedFont(notoSansJP);              // existing API
form.getTextField("name").setText("山田太郎", { font });   // new options bag
form.getTextField("name").setDefaultText("...", { font }); // symmetric
```

- `setText(value: string, opts?: { font?: PdfFont })` is the only signature change.
  No `setFont`, no separate `updateAppearances` — one atomic call.
- The passed font **overrides** the field's `/DA` font.
- `{ font }` on comb/dropdown/listbox → `FieldTypeError` at call time in TS, and
  rejected again in Rust validation (the wire JSON is a trust boundary — same dual
  guard as the create path).
- `setText` **without** a font on a Type0-DA field still throws, but the message
  becomes actionable: "pass `{ font }` with an embedded font".
- Standard-14 fills with no `font` option are byte-for-byte untouched (the existing
  `/V` WinAnsi encoding invariant remains tested).

## Rust engine (`fill.rs` + wire format)

The fill op gains an optional `fontId`; font bytes travel in the existing
concatenated-blob channel (same as draw ops — no new transport). At apply time, when
`fontId` is present:

1. Resolve/build the `BuiltFont` via the existing `build_embedded_font` engine.
   Fonts build **once per save**, shared across draw ops, create-path fields, and
   fills. Fill values' characters (value + default value) are added to
   `used_per_font` **before** the build loop, so `subset: true` never yields blank
   glyphs.
2. Render the appearance with `text_appearance_content_embedded` (Identity-H,
   CID=GID, hex `Tj`). Multiline wraps via `measure_embedded` with the same rules as
   the WinAnsi fill engine (word wrap; no mid-word breaking — that documented
   limitation carries over).
3. Write `/V` and `/DV` with `lopdf::text_string` (UTF-16BE) so CJK round-trips
   through `read_fields`.
4. Rewrite the field `/DA` to the Type0 resource (`BPF<n>` convention) and merge the
   font object into AcroForm `/DR` and the widget appearance `/Resources`.

**Apply-time resolution rule:** the appearance is generated inside the batched
pipeline after the value mutation; flatten re-resolves widget appearances at apply
time, so `setText({font})` + `flatten()` in one `save()` stamps correctly. This gets
an explicit regression test (it is the shape of the 1.10.1 flatten bug).

The existing Type0 guard in `fill.rs` is deleted, replaced by the two behaviors
above. All output lands as appended objects; incremental save is unaffected.

## Missing-glyph policy

- New exported error `MissingGlyphError` carrying the field name (or
  "drawText on page N"), the font name, and the deduped offending characters with
  code points (e.g. `"㐀" (U+3400)`).
- **Field fill:** any value character absent from the font's cmap → `save()` throws
  `MissingGlyphError`. Checked in Rust at apply time before any bytes are written; a
  throwing save produces no partial output. Rationale: a form value that prints
  blanks is silent data corruption.
- **`drawText`:** flips from silent-skip to **throw by default** with the same error.
  Opt-out: `drawText(text, { onMissingGlyph: 'skip' })`. The option is a string
  union so a future `'replace'` could slot in (not built now).
- Whitespace/control characters that legitimately lack glyphs (space handled by the
  engine, `\n` in multiline) are excluded from the check — only renderable
  characters trigger the error.
- Standard-14 / WinAnsi behavior is unchanged in this slice.
- **Versioning:** 1.11.0 with a prominent CHANGELOG behavioral-change entry framing
  the `drawText` flip as a data-loss bug fix, plus the opt-out. The README
  "missing glyphs silently skipped" limitation is rewritten.

## Testing

- CJK fill on a loaded standard-14 field (flagship case); on a builder-created
  embedded-font field (removes the old throw); plain + multiline wrap.
- Round-trip: fill → save → load → `read_fields` returns the exact value.
- Batched `setText({font})` + `flatten()` in one save (1.10.1-shaped regression).
- Subsetting: fill-only font renders all glyphs; a font shared between `drawText`
  and fill builds once.
- `MissingGlyphError` from fill and from `drawText`; `onMissingGlyph: 'skip'`.
- Standard-14 `/V` byte-identical invariant still green.
- Rejections: `{font}` on comb/dropdown/listbox at both TS and Rust boundaries.
- Acceptance (not CI): fixture PDFs opened in a real viewer — appearance streams are
  where "tests pass but Acrobat shows tofu" hides.

## Docs

README features + limitations (remove "embedded-font fill unsupported", add
missing-glyph note), `docs/migrating-from-pdf-lib.md` gains
`updateAppearances(font)` → `setText(value, { font })`, CHANGELOG 1.11.0 callout.
