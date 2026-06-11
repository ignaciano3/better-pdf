# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the version is `0.x`, the public API may change between minor releases.

## [Unreleased]

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

[Unreleased]: https://github.com/ignaciano3/better-pdf/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/ignaciano3/better-pdf/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ignaciano3/better-pdf/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ignaciano3/better-pdf/releases/tag/v0.1.0
