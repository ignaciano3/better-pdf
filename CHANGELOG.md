# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the version is `0.x`, the public API may change between minor releases.

## [Unreleased]

### Added

- `getListBox(name).select(value)` write accessor for single-select list-box
  fields, including the typed `doc.getForm<typeof schema>()` overlay.
- Typed error classes: `PdfError` base plus `UnknownFieldError`,
  `FieldTypeError`, `InvalidOptionError`, and `MissingOnStateError`, all
  exported from the package root and browser entry.

### Changed

- Tooling: TypeDoc API reference (`bun run docs`), a real headless-Chromium
  browser test (`bun run test:browser`) wired into CI, and a `LICENSE` shipped
  with the published WASM package.

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

[Unreleased]: https://github.com/ignaciano3/better-pdf/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ignaciano3/better-pdf/releases/tag/v0.1.0
