# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **`splitPages()` parses the source once instead of once per page.** Splitting
  an N-page document previously ran N full parse→assemble passes over the whole
  file; it now runs a single batched pass in the core and repacks the outputs,
  making it roughly 4x faster on a 12-page AcroForm (the gap grows with page
  count). Output bytes are unchanged — verified hash-identical against the
  previous implementation for every page, with and without `objectStreams`.

- **`generateFormTypes` emits schema, not data.** The generated metadata
  objects now contain exactly what the typed-form layer narrows on — `type`,
  `readOnly`, `states`, `options`, `multiSelect` — instead of every readable
  field property. Field values (`value` / `defaultValue`) and read-side flags
  (`required`, `exported`, `maxLength`, `password`, `multiline`, `comb`,
  `editable`, `align`, `tooltip`, `fontName`, `fontSize`, widget `pages`) are no
  longer baked into generated files: generating types from a filled form can no
  longer leak answers into source control, and regeneration diffs only when the
  schema actually changes. Read that data at runtime via `form.getFields()`.
  Hand-written schemas and previously generated modules keep compiling (extra
  properties were never consumed); the compile-time narrowing behavior of
  `doc.getForm<typeof …>()` is unchanged.

## [1.15.0] - 2026-08-18

### Added

- **`flipX` / `flipY` on `page.drawImage()`.** Mirror an embedded image
  horizontally, vertically, or both without pre-processing the pixels. The flip
  happens inside the placement box, so `(x, y)` remains the bottom-left corner
  and the drawn rectangle is unchanged; it composes with `rotate` and the skew
  options, mirroring along the image's own axes.

## [1.14.3] - 2026-08-12

### Fixed

- **`PdfDocument.passwordType()` on cross-reference-stream files.** It returned
  `null` — the same answer as a wrong password — for every password on files
  whose trailer is an xref stream (PDF 1.5+, what Word, Acrobat and
  `qpdf --object-streams=generate` emit), even though
  `PdfDocument.load(bytes, { password })` decrypted those same files. Apps that
  validate a password with `passwordType` before loading therefore rejected
  correct passwords and re-prompted forever. The `/Encrypt` and `/ID` entries
  the check needs are now read from the xref stream's dictionary as well as
  from a classic `trailer`, so both file shapes classify identically. The
  invariant — `passwordType(…) !== null` exactly when `load({ password })`
  succeeds — is now asserted across the whole encryption fixture matrix.

- **Encrypted cross-reference-stream files with a damaged xref were treated as
  plaintext.** The encryption gate that stops the recovery loader from
  "repairing" a still-encrypted document only looked for an `/Encrypt` trailer
  reference *outside* every object span — correct for a classic `trailer`, blind
  to xref-stream files, where that entry lives inside the xref stream object.
  Such a file loaded with no password at all, yielding a document whose strings
  and streams were still ciphertext (and `isEncrypted()` reported `false` for
  it). The gate now also reads `/Type /XRef` dictionaries, and `load_pdf`
  additionally re-checks the raw bytes when a parse yields a document with no
  pages, which is what a reconstructed-but-still-encrypted file looks like.

### Known limitations

- A password whose SASLprep (NFKC) form differs from the bytes typed — an accent
  written as a combining sequence, say — cannot open a file whose producer keyed
  it off those raw bytes (qpdf writes such files). lopdf normalizes before
  deriving the key and exposes no raw-bytes load path; `passwordType` and `load`
  agree in rejecting these, so no caller is misled. Details and the upstream fix
  in `docs/lopdf-saslprep-issue.md`.

## [1.14.2] - 2026-07-30

### Added

- **Typed forms expose `reset()` and `resetField()`**: the reset API shipped on
  `PdfForm` but was never added to `TypedPdfForm`, so `doc.getForm<typeof
  myFormFields>().reset()` failed to compile. Both are now declared, with
  `resetField` narrowed to the schema's field names.
- **`better-pdf-generate-types --password PW`**: the types CLI can now open
  encrypted PDFs. Previously it called `PdfDocument.load` without options, so
  any encrypted file failed with an error pointing at an API the CLI did not
  expose. Pass `--password ''` for owner-locked files.
- **Generated metadata covers every readable field property.** Field reads have
  grown a lot (`defaultValue`, `password`, `multiline`, `comb`, `editable`,
  `align`, `tooltip`, `fontName`, `fontSize`, widget geometry) while the
  generated module still emitted the original subset. All of them are now
  emitted, plus a deduplicated `pages` tuple projected from each field's
  widgets. A test asserts the generated module keeps pace with `FieldInfo`, so
  the next added property cannot silently skip typegen.

## [1.14.1] - 2026-07-09

### Changed

- `form.flatten()` field resolution is now linear in the number of fields
  instead of quadratic: queued names are resolved in a single walk of the
  field tree (plus one orphaned-widget page scan) rather than one full walk
  per name. Flatten-all on a 250-page / 1,000-field form drops from ~217 ms
  to ~51 ms — about 3.4x faster than pdf-lib on the same document, where it
  previously trailed it. No behavioral change: match order, orphaned-widget
  fallback, and error messages are preserved.

## [1.14.0] - 2026-07-09

### Added

- **`PdfDocument.isEncrypted(bytes)`**: report whether a PDF is encrypted
  without decrypting it or needing a password, so callers can decide whether to
  pass `{ password }` to `load`.
- **`PdfDocument.passwordType(bytes, password)`**: classify how a password
  authorizes an encrypted PDF — `"owner"` (full access), `"user"` (restricted),
  or `null` when it authenticates neither role or the file is not an encrypted
  classic-`trailer` PDF (xref-stream encrypted files return `null`).
- **Hierarchical (dotted) field names** are now resolved: a parent field with
  terminal kids is expanded into fully-qualified `parent.child` names in
  `getFields()`, and the qualified children are fillable and round-trip.
  Flattening prunes the nested tree, dropping emptied parents.
- **Orphaned widget fields** — Widget annotations present on a page but never
  linked into `/AcroForm/Fields` (some producers emit these) — are now surfaced
  by `getFields()` and are fillable by name.
- Test suite: added `tests/pypdf-ported.test.ts` (behavioral tests ported from
  [pypdf](https://github.com/py-pdf/pypdf)) plus its fixture corpus under
  `tests/fixtures/pypdf/`.

### Fixed

- Filling a text field whose `/DA` names a standard-14 font that is absent from
  the AcroForm `/DR` no longer throws; the font is synthesized into the
  generated appearance's `/Resources/Font` (affects real government forms such
  as IRS f1040).
- A corrupt catalog whose `/Pages` reference points at the wrong object (e.g.
  the Info dictionary) is now recovered to the real page tree instead of loading
  with zero pages.
- V4 (AES-128) encrypted PDFs whose `/Encrypt` dictionary omits the top-level
  `/Length` now decrypt (the spec fixes V4 at 128-bit; a workaround for the
  underlying decryptor — see [J-F-Liu/lopdf#523](https://github.com/J-F-Liu/lopdf/issues/523)).

### Changed

- Updated the `lopdf` dependency from 0.41 to 0.43.

## [1.13.0] - 2026-07-08

### Added

- **Recovery loader**: PDFs with broken or missing xref tables/trailers, junk
  before the `%PDF` header, invalid `/Root` references, or missing
  `endstream`/`endobj` keywords are now repaired on load instead of failing
  (ported pdf-lib robustness corpus).
- Test suite: added `tests/pdf-lib-ported.test.ts` (65 behavioral tests ported
  from pdf-lib) plus its fixture corpus under `tests/fixtures/pdf-lib/`.

### Fixed

- Field names stored as UTF-16BE text strings (FE FF BOM) are decoded
  correctly (affects lookup, fill, and flatten by name).
- Indirect references in `/V`, `/DV`, and `/Opt` are dereferenced when
  reading and filling fields.
- Radio groups with `/Opt` report and accept the option label instead
  of the raw index on-state; TS API options via `PdfRadioGroup.select()`
  now surface `/Opt` labels.

## [1.12.1] - 2026-07-08

### Fixed

- Field detection now resolves a widget's page when the page's `/Annots` is an
  indirect reference to the array (macOS Quartz writes it this way) instead of
  reporting the field with no widgets.
- Fields whose `/Fields` entry is a duplicated widget dict present on no page
  (another Quartz pattern) now recover their widgets from the page `/Annots`,
  matched by fully-qualified field name — the same merge rule Acrobat applies.
- Flatten's annot removal also follows an indirect `/Annots` reference.

## [1.12.0] - 2026-07-08

### Added

- `doc.attach(bytes, name, options)` — embed file attachments (`/EmbeddedFiles`) on
  created and loaded documents; queued and written at `save()`.
- `doc.getAttachments()` — read every attachment's metadata and bytes.
- `afRelationship` option writes `/AFRelationship` and the catalog `/AF` array
  (ZUGFeRD/Factur-X structure).
- New errors: `DuplicateAttachmentError`, `AttachmentNotFoundError` (reserved).

## [1.11.0] - 2026-07-06

### Added

- **Embedded-font form fill.** `field.setText(value, { font })` and
  `.setDefaultText(value, { font })` now accept a font from
  `doc.embedFont(bytes)`, so text-field values can carry any Unicode script
  (CJK included) — not just the standard-14 WinAnsi fonts. Works on plain and
  multiline text fields, on both loaded and builder-created documents. Values
  are written `/V`/`/DV` as UTF-16BE and round-trip through load/read.
  Embedded fonts used across `drawText` and form fill in the same `save()`
  are built once and shared, and subsetting automatically includes glyphs
  used by fill values. **Still unsupported:** comb, dropdown, and listbox
  fields reject an embedded font (`FieldTypeError`) — they remain
  standard-14 only. A `setText({ font })` call also cannot be combined with
  `insertPage`/`removePage`/`movePage` in the same `save()`; call `save()`
  separately before or after the page-structure change.

### Changed (behavioral)

- **Missing glyphs now throw instead of being silently dropped.** A
  silently-skipped glyph in `drawText()` or embedded-font form fill was data
  loss dressed up as success — text visibly went missing from rendered
  output with no signal to the caller. Both now throw `MissingGlyphError`
  when the font has no glyph for a character. `drawText()` gains an opt-out:
  `page.drawText(text, { font, onMissingGlyph: "skip" })` restores the old
  silent-skip behavior; there is no skip opt-out for form fill.

## [1.10.1] - 2026-07-05

### Fixed

- **Fill + flatten of the same field in one `save()` lost the field's text**
  (regression in 1.8.1's batched save; affects 1.8.1–1.10.0). Flatten resolved
  the widget's appearance against the pre-fill document, so the appearance the
  fill generated in the same save was never stamped into the page content —
  the field was removed but its value became invisible. Flatten now resolves
  the appearance at apply time, seeing the state fills earlier in the same
  save produced. Chained `fill → save → flatten → save` was never affected.

## [1.10.0] - 2026-07-03

### Added

- **Stream compression on save.** `save()` now deflate-compresses the content,
  appearance, and font streams `better-pdf` generates, producing substantially
  smaller PDFs. Controlled by a new option: `doc.save({ compress })`, defaulting
  to `true`. Pass `doc.save({ compress: false })` for plaintext output.
  - New `SaveOptions` type (`{ compress?: boolean }`), exported from the package
    entry points. Additive and backward-compatible — `save()` with no argument
    behaves as before, only smaller.
  - Streams that already carry a `/Filter` (embedded images, embedded font
    programs) are left untouched, so there is no double-compression.
  - Benchmarks (load → mutate → save): on the full-document create path, output
    shrank to ~12% of its uncompressed size (5-page text document) for a ~4%
    (~0.02 ms) time cost per save. On incremental (loaded-document) saves the
    time cost is within noise, since only the newly appended section is
    compressed.

  Caveats:
  1. Raw PDF byte output changes. Consumers that regex or snapshot the raw bytes
     of saved PDFs should pass `{ compress: false }` or update their fixtures.
  2. Incremental saves remain append-only, so compression only affects the newly
     appended section — the original revision's bytes are preserved, so existing
     digital signatures on that revision stay valid.

- **Object streams (opt-in structural compression).** On full-document saves you
  can now pack non-stream objects into PDF object streams + cross-reference
  streams for smaller output: `doc.save({ objectStreams: true })` (created
  documents) and `PdfDocument.merge` / `assemble` / `copyPages` / `splitPages`
  via a new `ManipulateOptions` (`{ objectStreams?: boolean }`). New
  `SaveOptions.objectStreams`, default `false`.
  - Applies only to full-document paths (create/merge/assemble/copyPages/splitPages).
    Incremental (loaded-document) saves ignore the flag and remain append-only.
  - Object streams require and enable cross-reference streams and raise the
    output to PDF 1.5+. The result is **not** PDF/A-1 conformant; leave the option
    off (the default) if you need PDF/A-1 or maximum consumer compatibility.

## [1.9.0] - 2026-07-02

### Added

- **`createForm()` now works on documents opened with `PdfDocument.load()`.**
  Add new AcroForm fields (text, checkbox, radio, dropdown, list box, signature)
  to an existing PDF, then read/fill them via `getForm()` in the same session.
  Fields are injected on the first `getForm()`/`save()`; add all fields before
  calling `getForm()`. A field name that collides with an existing field is
  rejected. Filling an embedded-font field created this way is not yet supported.
- **`getForm()` now works on documents created with `PdfDocument.create()`.**
  After adding fields with the form builder, call `getForm()` in the same
  session to read, fill, and flatten them — no save-and-reload round-trip. The
  first `getForm()` call materializes the document and seals it: adding more
  fields, pages, or drawings afterward throws `FormSealedError`.

## [1.8.1] - 2026-06-30

### Fixed

- **Browser build now exposes `PdfDocument.assemble()` and `PdfDocument.merge()`.**
  Both static methods existed on the Node entry but were missing from the browser
  entry. The two entry points now share one implementation, so the browser build
  has the same page-assembly API as Node.
- **`maxWidth` text wrapping normalizes CR and CRLF line breaks.** Drawing text
  with `maxWidth` now treats `\r\n` and `\r` as line breaks (matching `\n`), so
  created documents wrap multi-line text consistently regardless of newline
  style.

### Changed

- **Saves that combine operations are faster.** A loaded document that queues
  more than one kind of change (for example field fills plus drawing plus
  metadata) is now written in a single parse → mutate → serialize pass instead of
  one full round-trip per operation. Output bytes are unchanged.

## [1.8.0] - 2026-06-29

### Added

- **Toggle `multiline` / `comb` / `password` on a loaded text field.**
  `PdfTextField` gains `setMultiline(value)`, `setComb(value, maxLen)` /
  `setComb(false)`, and `setPassword(value)`. Unlike the other flag setters,
  these regenerate the field's appearance stream from its current value:
  multiline wraps and top-aligns, comb draws fixed-pitch per-character cells
  (writing the cell count to `/MaxLen`), and password renders an empty
  appearance so the value never leaks into the stream (the `/V` is preserved).
  Enabling `comb` requires a `maxLen`; the `setComb` overload enforces this at
  the type level, and the engine rejects a comb toggle that would leave the
  field with no `/MaxLen`. These flags apply to text fields only.

## [1.7.0] - 2026-06-28

### Added

- **Read & modify encrypted PDFs.** `PdfDocument.load(bytes, { password })`
  decrypts RC4 / AES-128 / AES-256 encrypted PDFs (use `""` for owner-locked
  files), so they can be read, filled, and flattened. Decryption is opt-in —
  bare `load(bytes)` is unchanged. Modifying an encrypted PDF produces a
  decrypted output. A wrong password throws the new `IncorrectPasswordError`; an
  encrypted file loaded without a password throws `EncryptedPdfError`. Producing
  encrypted output is still unsupported.

## [1.6.0] - 2026-06-28

### Added

- **Configurable standard-14 field font.** `addTextField`, `addDropdown`, and
  `addListBox` accept `font?: StandardFonts` to render the field value in any of
  the 12 standard text fonts (Helvetica / Times / Courier families). Each
  distinct font is registered once in the AcroForm `/DR`. Defaults to Helvetica;
  embedded/CJK fonts remain unsupported for form fields.

## [1.5.0] - 2026-06-28

### Added

- **Create multi-select list boxes.** `addListBox(name, { multiSelect: true, … })`
  sets the choice Multiselect flag, so the generated field reports
  `FieldInfo.multiSelect === true` and accepts `listBox.selectMultiple(values)`.
  `addDropdown` rejects `multiSelect`, since combo boxes are never multi-select.
- **Mutate field flags and widget visibility on loaded fields.** Every field
  wrapper gains setters that change a field's flags rather than its value:
  `setReadOnly`, `setRequired`, and `setExported` toggle the field `/Ff`
  ReadOnly / Required / NoExport bits; `hide`, `show`, `setPrintable`, and
  `setNoView` toggle the `/F` Hidden / Print / NoView bits on each of the
  field's widgets. Changes are applied to the in-memory `FieldInfo` immediately
  and written on `doc.save()`. The shared logic lives on a new exported
  `PdfField` base class; the change carries through as a `FieldFlagChanges`
  payload. The appearance-affecting flags `multiline` / `comb` / `password`
  remain creation-only.

## [1.4.0] - 2026-06-28

### Added

- **More field metadata on read.** `FieldInfo` now exposes `multiline`, `comb`,
  `password` (text-field `/Ff` flags), `editable` (combo box Edit flag), `align`
  (from the widget `/Q` quadding), `tooltip` (the `/TU` descriptive name, or
  `null`), `defaultValue` (the `/DV` reset value, or `null`), and `fontName` /
  `fontSize` (the effective `/DA` font resource name and size for variable-text
  fields, else `null`).
- **Widget visibility flags on read.** Each `FieldInfo.widgets` entry now carries
  `hidden`, `print`, and `noView`, decoded from the annotation `/F` flags.
- **Writable default value (`/DV`).** The field default/reset value — what a
  viewer's "reset form" restores, independent of the current value — can be set
  on all value-bearing field types. New fields take builder options
  `defaultValue` (text), `defaultChecked` (checkbox), and `defaultSelected`
  (radio/dropdown/listbox); existing fields take the setters `setDefaultText`,
  `setDefaultChecked`, and `setDefaultSelected`. Choice/radio defaults validate
  against the field's options; text defaults validate against `maxLength`.
- **Form reset.** `form.resetField(name)` and `form.reset()` restore fields to
  their default value (`/DV`), or clear them when there is none — the equivalent
  of a PDF viewer's "reset form". `reset()` skips signature and push-button
  fields.
- **Writable password text fields.** `addTextField` accepts `password: true` to
  set the `/Ff` Password flag, so viewers mask the displayed value. This changes
  display only (not encryption) and is independent of the field's value.

### Fixed

- **Generated form fields now print.** Created field widgets set the annotation
  `/F` Print flag (bit 3), so fields added via the builder appear in printed
  output instead of being omitted by viewers.

## [1.3.0] - 2026-06-26

### Added

- **Image and embedded-page transforms.** `drawImage` and `drawPage` accept
  `rotate` (degrees, counter-clockwise about the placement point) and
  `xSkew` / `ySkew` (degrees), applied via the content-stream CTM.
- **Dashed strokes.** All stroked shapes — `drawLine`, `drawRectangle`,
  `drawEllipse`, `drawSvgPath`, and `drawPolygon` — accept `dash` (alternating
  on/off segment lengths in points) and `dashPhase`.

## [1.2.0] - 2026-06-26

### Added

- **Field text alignment and size.** Text and choice fields accept `align`
  (`"left"` | `"center"` | `"right"`, mapped to the widget `/Q` quadding) and
  `fontSize` (points, default 12, applied to the `/DA` string and appearance).
- **Checkbox and radio mark styles.** `addCheckBox` and `addRadioGroup` accept
  `checkStyle` — `"check"`, `"cross"`, `"circle"`, `"square"`, `"diamond"`, or
  `"star"` — drawn as vector paths in the selected appearance. Defaults are
  unchanged (checkbox = check, radio = filled circle).
- **Comb text fields.** `addTextField` accepts `comb: true` to render a single
  line split into `maxLength` equal cells, one character per cell (e.g. SSN or
  date boxes). Sets the `/Ff` Comb flag; requires `maxLength` and is
  incompatible with `multiline`.
- **Image and embedded-page opacity.** `drawImage` and `drawPage` accept
  `opacity` (0–1), applied as an ExtGState (`/ca`, `/CA`). Composes with
  per-pixel PNG soft masks.

## [1.1.0] - 2026-06-25

### Added

- **Field text color.** All generated form fields accept an optional `textColor`
  (a `Color`) on their options, controlling the color of the field's
  text/value via the widget `/DA` string. Defaults to black. Applies to text
  fields and choice fields (dropdowns/list boxes).
- **Editable combo boxes.** `addDropdown` accepts `editable: true` to set the
  combo box Edit flag, letting users type a custom value not in `options`.
  `addListBox` ignores `editable` (list boxes are never combo boxes).

## [1.0.0] - 2026-06-23

First stable release. The public API is frozen and the package now follows
Semantic Versioning — breaking changes will only land in a future major.

### Changed

- **Stable 1.0.0 — API frozen.** The TS surface settled in 0.20.0 (see the
  0.19 → 0.20 migration guide) is now committed. No code behavior changes from
  0.21.0; this release marks the stability commitment.

### Docs

- Fixed a stale "merged/assembled form fields are not interactive" claim that
  survived in the README §(g) and the Generating guide — merged AcroForm fields
  have stayed interactive since 0.15.0.
- Added an **Examples** section to the docs site (overview indexing the
  `examples/runtimes/` starters, plus fill-and-flatten / invoice / merge-PDFs
  recipes) and surfaced the previously orphaned **Runtime setup** guide in the
  sidebar.

## [0.21.0] - 2026-06-20

### Added

- `./wasm` export subpath — resolves to the raw `.wasm` binary; gives bundlers
  (Vite, webpack, Next.js) and edge runtimes (Cloudflare Workers) a stable asset
  handle to pass to `initializeWasm()`.
- Runtime examples in `examples/runtimes/`: Node.js, Bun, Deno, Vite, webpack
  5, Next.js 15, Cloudflare Workers — each with a README, working code, and
  honest status (Verified or Config provided).
- Per-runtime guide at `docs/site/src/content/docs/guides/runtimes.md` with
  one section per runtime, exact init snippet, and a summary matrix.
- Runtime support matrix in README (replaces the vague bundler-requirements
  hedge); links to `examples/runtimes/` and the new guide.

### Fixed

- `sideEffects` in `package.json` now lists `./dist/core/wasm.js` and
  `./dist/core/wasm-browser.js` — bundlers no longer tree-shake the wasm init
  side effect.

## [0.20.0] - 2026-06-20

See [Migration guide: 0.19 → 0.20](docs/site/src/content/docs/migrating/0.19-to-0.20.md) for
before/after examples of every breaking change.

### Changed (BREAKING)

- **Shape draw-options unified.** `drawRectangle` and `drawEllipse` options
  `color`/`borderColor`/`borderWidth` are renamed to `fill`/`stroke`/`strokeWidth`.
  `drawLine` options `color`/`thickness` are renamed to `stroke`/`strokeWidth`. These
  now match the already-stable `drawSvgPath`/`drawPolygon` option names.
- **`drawEllipse` radii renamed.** `xScale`/`yScale` → `radiusX`/`radiusY`.
- **`DocumentMetadata.modDate` → `modificationDate`.** The field read by
  `doc.getMetadata()` is now `modificationDate`.
- **Internal fields removed from the public type surface.** `PdfImage.bytes`,
  `EmbeddedPdfPage.bytes`, `PdfForm.queue`/`flattenQueue`, and the `_fontId`/`_bytes`
  accessors are no longer part of the public TypeScript types. These were always
  marked `@internal` and never documented.

### Added

- `DrawLinkOptions`, `DrawSvgPathOptions`, and `DrawPolygonOptions` are now exported
  from both the Node (`@ignaciano3/better-pdf`) and browser entry points.
- `docs/STABILITY.md` — semver and deprecation policy for the library.
- `fixtures:generate:update` npm script for the incremental-update fixture generator
  (`scripts/make-objstream-update-fixture.ts`), making it discoverable alongside
  `fixtures:generate`.
- Two new synthetic PDF-1.5+ test fixtures: a larger multi-object objstm file and an
  incremental-update over an xref-stream base. Both pass the fill/flatten round-trip
  and qpdf-validate loops.

### Fixed

- Page-index resolution in `draw.rs` and `create.rs` converted from bare array index
  (panic on out-of-range) to clean `Err` propagation. A `goToPage` link to a
  non-existent page now returns a typed error instead of crashing.

## [0.19.0] - 2026-06-20

### Added

- `page.drawSvgPath()` now supports SVG elliptical-arc commands `A`/`a`. Arcs are
  converted to cubic-bézier segments in TypeScript (SVG 1.1 Appendix F.6.5/F.6.6),
  including out-of-range-radii correction, ≤90° sweep splitting, packed-flag
  parsing, and the spec degenerate cases (zero radius → line, zero-length → no-op).

## [0.18.0] - 2026-06-20

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

## [0.17.0] - 2026-06-20

### Added

- Encrypted PDFs are now detected on load (an `/Encrypt` trailer entry) and rejected with a new typed `EncryptedPdfError` (a `PdfError`), exported from both the Node and browser entry points. Encryption remains unsupported; this turns a confusing downstream failure into an explicit, catchable error.

## [0.16.0] - 2026-06-20

### Added

- Filling a form text field that carries the Multiline flag (AcroForm `Tx` field, Ff bit 13) now generates a wrapped, top-aligned multi-line `/AP/N` appearance. Hard `\n` breaks are preserved, each paragraph is greedily word-wrapped to the field width, per-line quadding (left/center/right) is honored, and a word wider than the field overflows onto its own line. Single-line text fields are unchanged.

## [0.15.0] - 2026-06-19

### Added

- `PdfDocument.merge` / `assemble` / `copyPages` now rebuild a working `/AcroForm` when assembled pages carry form widgets, so fields stay interactive (fillable) in the output. The rebuilt form merges each source's `/DR` fonts and `/DA` and sets `/NeedAppearances true`. Field names that collide across source documents are renamed with a per-source prefix (`d0_`, `d1_`, …).

### Notes

- A page selected more than once shares its field objects (linked, not renamed). `/XFA` data is not carried into the merged output.

## [0.14.0] - 2026-06-19

### Added

- `page.drawText` now accepts a `maxWidth` option that word-wraps text to fit the given width in points. Explicit `\n` remain hard breaks; a word wider than `maxWidth` overflows onto its own line. Works for both standard-14 and embedded fonts.

## [0.13.1] - 2026-06-19

### Fixed

- Draw operations now follow their page across structural changes. A `PdfPage` handle (from `addPage`, `getPage`, or `getPages`) carries a stable identity; its final index is resolved at `save()` time. Previously, drawing on an appended page and then calling `insertPage`/`removePage`/`movePage`, or drawing on a loaded page and then moving it, could silently stamp content onto the wrong page. Drawing on a page that is removed before `save()` now throws instead of mis-targeting.

### Added

- `insertPage`, `removePage`, and `movePage` validate their indices eagerly and throw `PageOutOfRangeError` for out-of-range or non-integer arguments, instead of failing later in the core.

## [0.13.0] - 2026-06-19

### Added

- Page insertion on loaded documents: `doc.addPage(size?)` now works on loaded PDFs (appends a blank, drawable page); `doc.insertPage(index, size?)`, `doc.removePage(index)`, `doc.movePage(from, to)`. Incremental — existing forms and content are preserved. Appended pages are drawable in the same save; inserted/removed/moved pages are reflected after save + reload.

### Fixed

- Non-ASCII document metadata: `setTitle`/`setAuthor`/etc. now correctly encode non-Latin text (Japanese, accented Latin, etc.) as UTF-16BE, ensuring round-trip fidelity.
- Palette (indexed-color) PNG embedding: color-type-3 PNGs with `tRNS` transparency now embed correctly (transparency stored as a soft mask, same as RGBA PNGs).

## [0.12.0] - 2026-06-19

### Added

- Text rotation & opacity (`drawText({rotate, opacity})`) and document outlines/bookmarks (`doc.setOutline()`), on loaded and created PDFs.

## [0.11.0] - 2026-06-18

### Added

- Vector paths: `page.drawSvgPath()` (SVG path data) and `page.drawPolygon()` with fill/stroke/opacity, on loaded and created PDFs. SVG arcs (A/a) not yet supported.

## [0.10.0] - 2026-06-18

### Added

- Link annotations: `page.drawLink()` adds clickable external-URI and internal page-jump links on loaded and created PDFs.

## [0.9.0] - 2026-06-18

### Added

- Embed PDF pages: `doc.embedPdfPage(src, pageIndex)` + `page.drawPage(embedded, {x, y, width?, height?})` stamp a page from another PDF as a Form XObject, on loaded and created PDFs. `width`/`height` default to the source page's intrinsic MediaBox size. Interactive form fields and annotations on the embedded page are flattened to static visual appearance only.

## [0.8.0] - 2026-06-18

### Added

- PNG transparency: the alpha channel of RGBA and gray+alpha PNGs is preserved
  as a soft mask (`/SMask`) on embedded images. `embedPng` + `drawImage` just
  work — no API change. Opaque RGB/grayscale PNGs are unaffected.

## [0.7.0] - 2026-06-18

### Added

- Page rotate/resize: `page.setRotation()`, `page.setSize()`, `page.setMediaBox()`
  on loaded and created PDFs.
- `page.setRotation(degrees)` — rotate a page by any multiple of 90; value is
  normalised to 0 / 90 / 180 / 270. Non-multiples of 90 throw `InvalidRotationError`.
- `page.setSize(width, height)` — resize a page (convenience wrapper around
  `setMediaBox(0, 0, width, height)`).
- `page.setMediaBox(x0, y0, x1, y1)` — set the PDF `/MediaBox` directly; useful
  when the page has a non-zero origin.
- All three methods work on pages from both `doc.getPage(i)` (loaded documents)
  and `doc.addPage(...)` (created documents).

## [0.6.0] - 2026-06-17

### Added

- Page operations: merge multiple PDFs, extract/copy/reorder pages, and split
  into single-page PDFs — `PdfDocument.merge` / `PdfDocument.assemble`,
  `doc.copyPages` / `doc.splitPages`.
- `PdfDocument.merge(docs: Uint8Array[]): Promise<Uint8Array>` — combine an
  array of PDFs into one document (all pages, in order).
- `PdfDocument.assemble(docs, selections): Promise<Uint8Array>` — build a new
  PDF from an explicit ordered selection of `{docIndex, pageIndex}` entries
  across multiple source documents; supports reorder, cross-doc copy, and
  page removal by omission.
- `doc.copyPages(indices: number[]): Promise<Uint8Array>` — extract the given
  pages (0-based) from a loaded document into a new PDF.
- `doc.splitPages(): Promise<Uint8Array[]>` — split a loaded document into one
  single-page PDF per page.
- Inherited page attributes (MediaBox, Resources, Rotate, CropBox) are resolved
  so extracted pages stand alone.

### Notes

- Form fields on assembled or merged pages keep their **visual appearance** (the
  field appearance stream is baked onto the page) but are **not interactive** —
  no AcroForm dictionary is reconstructed in the output. Flatten fields before
  merging to produce a flat, printable result.
- In-place page rotation/resize and blank-page insertion are not yet available
  (rotation/resize is planned for M29).

## [0.5.0] - 2026-06-17

### Added

- Document metadata: read/write the PDF Info dictionary (Title, Author, Subject, Keywords,
  Creator, Producer, CreationDate, ModDate) on both loaded and created PDFs via
  `doc.setTitle()` / `doc.setAuthor()` / `doc.setSubject()` / `doc.setKeywords(arr)` /
  `doc.setCreator()` / `doc.setProducer()` / `doc.setCreationDate(d)` /
  `doc.setModificationDate(d)`, and `await doc.getMetadata()` → `DocumentMetadata`.
- On a **loaded** PDF, metadata setters write an incremental update; Info-dict keys that are
  not set are preserved from the original document.
- Dates round-trip: setters accept a JS `Date`; `getMetadata()` returns `Date` objects.

### Notes

- XMP metadata streams are not written or modified (only the Info dictionary is updated).

## [0.4.0] - 2026-06-17

### Added

- `doc.embedFont(bytes, { subset? })` — embed any TTF or OTF font and get back a
  `PdfFont` for use with `drawText`. Fonts are embedded as Type0/CIDFontType2
  composite fonts with a ToUnicode CMap, enabling full Unicode text (including CJK
  and accented Latin) that is selectable and searchable in PDF viewers.
- `{ subset: boolean }` option (default `true`) — strips the embedded font to only
  the glyphs actually used in the document, keeping output file sizes small.
- `widthOfTextAtSize` works on embedded fonts, enabling layout calculations.
- Both `PdfDocument.create()` and `PdfDocument.load()` paths support `embedFont`;
  embedded fonts can be used on both created and existing pages.

### Known caveats

- The subsetter supports TrueType (`glyf`) outlines. OpenType-CFF (`.otf` files
  whose outlines are CFF rather than `glyf`) may fail to subset — pass
  `{ subset: false }` for those fonts.
- Characters with no glyph in the embedded font are silently skipped.

## [0.3.0] - 2026-06-13

### Added

- `doc.createForm()` — a chainable `FormBuilder` for adding AcroForm fields to a
  document created with `PdfDocument.create()` (throws on loaded documents).
- Field creation methods: `addTextField`, `addCheckBox`, `addRadioGroup`,
  `addDropdown`, `addListBox`, and `addSignatureField`, covering all six field
  types: text, checkbox, radio, dropdown, listbox, and signature.
- Per-field options: `value`, `readOnly`, `required`, `tooltip`, `maxLength`,
  and `multiline` (text fields), plus `checked`/`onValue` (checkboxes) and
  `selected` (radio/choice fields).
- Per-field `border` (`{ color, width? }`) and `background` (a `Color`)
  appearance options, using `rgb`/`grayscale` color helpers.
- Typed field-name accumulation: each `add*` call refines the builder's schema,
  so `getFieldNames()` returns the declared names typed. Generated forms are
  normal fillable AcroForms — after `save()` and reload via `PdfDocument.load`
  they can be filled with `getForm()` and flattened by this library.

## [0.2.0] - 2026-06-13

### Added

- Page access on loaded documents: `doc.getPageCount()`, `doc.getPages()`, and
  `doc.getPage(i)` returning a `PdfPage`; throws `PageOutOfRangeError` for
  out-of-range indices.
- `drawText` on both loaded and created pages, with `x`, `y`, `size`, `font`,
  `color`, and `lineHeight` options.
- `PdfDocument.create()` — create a new empty document without a source PDF.
- `doc.addPage(size)` — append a page given a `[width, height]` tuple;
  `PageSizes` constants (`A3`, `A4`, `A5`, `Letter`, `Legal`, `Tabloid`)
  provide the standard sizes.
- `doc.embedJpg(bytes)` / `doc.embedPng(bytes)` — embed images into the
  document, returning a `PdfImage` with `.width`, `.height`, and `.scale(f)`.
- `page.drawImage(img, options)` — draw an embedded image on a page with
  `x`, `y`, `width`, and `height`.
- `page.drawLine(options)` — draw a line segment with `start`, `end`,
  `thickness`, `color`, and `opacity`.
- `page.drawRectangle(options)` — draw a filled and/or bordered rectangle with
  `x`, `y`, `width`, `height`, `color`, `borderColor`, `borderWidth`, and
  `opacity`.
- `page.drawEllipse(options)` — draw a filled and/or bordered ellipse; `(x, y)`
  is the center and `xScale`/`yScale` are the x and y radii.
- `rgb(r, g, b)` and `grayscale(v)` color helpers.
- `StandardFonts` enum: `Helvetica`, `HelveticaBold`, `HelveticaOblique`,
  `HelveticaBoldOblique`, `Courier`, `CourierBold`, `CourierOblique`,
  `CourierBoldOblique`, `TimesRoman`, `TimesBold`, `TimesItalic`,
  `TimesBoldItalic`.
- `doc.getFont(StandardFonts.X)` — returns a `PdfFont` with
  `font.widthOfTextAtSize(text, size)` for layout calculations.
- `./forms` and `./generate` subpath exports for tree-shaking-friendly imports.
- `PageOutOfRangeError` and `InvalidImageError` added to the `PdfError` family.

## [0.1.2] - 2026-06-11

### Changed

- Expanded TypeScript API documentation with pdf-lib-style JSDoc examples,
  parameters, return values, and error notes for document, form, field, and type
  generation APIs.
- Aligned the Rust/WASM core package version with the npm package version.

### Fixed

- Filled text values with accented and other non-ASCII characters are now stored
  as proper PDF text strings and decode correctly when fields are read back.

## [0.1.1] - 2026-06-11

### Added

- `FieldInfo.maxLength` (text field `/MaxLen`) and `FieldInfo.exported`
  (false when the `NoExport` flag is set); both also emitted by the type
  generator. `setText()` now throws `MaxLengthExceededError` past `/MaxLen`.
- `getListBox(name).select(value)` write accessor for single-select list-box
  fields, including the typed `doc.getForm<typeof schema>()` overlay.
- Typed error classes: `PdfError` base plus `UnknownFieldError`,
  `FieldTypeError`, `InvalidOptionError`, `MaxLengthExceededError`, and
  `MissingOnStateError`, all exported from the package root and browser entry.

### Changed

- Package renamed to `@ignaciano3/better-pdf` (the unscoped npm name is taken).
- Ships a single WASM binary (web target); Node loads it synchronously from disk.
- Signature images cross the JS↔WASM boundary as binary, not JSON number arrays.
- `FieldInfo.value` now reflects queued mutations immediately; `save()` always
  starts from the originally loaded bytes.
- Tooling: TypeDoc API reference (`bun run docs`), a real headless-Chromium
  browser test (`bun run test:browser`) wired into CI, and a `LICENSE` shipped
  with the published WASM package.

### Added

- `PdfCoreError`: core (WASM) failures from `save()` are part of the `PdfError` family.
- Standard-14 font metrics with Arial/Times New Roman/Courier New aliases,
  `/Widths`-array fallback, and full WinAnsi text encoding (€, smart quotes, …).
- XFA-backed forms are detected and rejected on fill/flatten with a clear error.
- CMYK JPEG signature images are rejected instead of being mislabeled RGB.
- Validation: `qpdf --check` in CI, a pdf.js render regression check, fuzz
  targets for the PDF/image/DA parsers, and xref-stream/object-stream fixtures.

## [0.1.0]

First public pre-release. Fill and flatten AcroForm fields in existing PDFs,
from both the browser and server runtimes, via a Rust core compiled to
WebAssembly with a fully-typed TypeScript API.

### Added

- Load a PDF and read AcroForm fields: fully-qualified `name`, `type`, `value`,
  `states`, `options`, `readOnly`, `required`, and per-widget `widgets`
  (0-based `page` index and `rect` in PDF points).
- Typed mutation accessors: `getTextField`, `getCheckBox`, `getRadioGroup`,
  `getDropdown`, and `getSignature`, using a field's real export values.
- Self-generated appearance streams so filled and flattened fields render
  without relying on `/NeedAppearances`.
- Flatten one field (`flattenField`) or all fields (`flatten`).
- Visual-only signature images from JPEG and supported PNG inputs.
- Append-only incremental saves (`save()`); a no-op save is a byte-exact
  round trip.
- Form type generator (`generateFormTypes` and the
  `better-pdf-generate-types` CLI) plus a type-only narrowed
  `doc.getForm<typeof schema>()` that turns unknown field names, wrong-type
  access, and invalid option/state values into compile errors at zero runtime
  cost.
- Browser entry (`better-pdf/browser`) backed by the `--target web` WASM build.
- Agent skill shipped in the package for AI-driven usage.
- `pdf-lib` comparison benchmark harness (`bun run bench`).

### Known limitations

- Existing PDFs only; no PDF creation, encryption, or malformed-PDF recovery.
- No cryptographic/PAdES signing — signatures are appearances only.
- Text fields are single-line; multi-line wrapping is not generated.
- PNG alpha is dropped rather than preserved as a soft mask.

[Unreleased]: https://github.com/ignaciano3/better-pdf/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/ignaciano3/better-pdf/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/ignaciano3/better-pdf/compare/v0.21.0...v1.0.0
[0.6.0]: https://github.com/ignaciano3/better-pdf/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/ignaciano3/better-pdf/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/ignaciano3/better-pdf/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/ignaciano3/better-pdf/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ignaciano3/better-pdf/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/ignaciano3/better-pdf/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ignaciano3/better-pdf/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ignaciano3/better-pdf/releases/tag/v0.1.0
