# Stability and Versioning Policy

## Semantic Versioning from 1.0.0

`@ignaciano3/better-pdf` follows [Semantic Versioning](https://semver.org) from the 1.0.0 release.

- **Patch** releases fix bugs without changing the public API.
- **Minor** releases add backwards-compatible functionality.
- **Major** releases may contain breaking changes to the public API.

Breaking changes will only be made in major releases and will be called out explicitly in the CHANGELOG.

## Public API Surface

The **public API** is everything exported from the documented package entry points:

| Import path | Entry point |
|---|---|
| `@ignaciano3/better-pdf` | `./dist/index.js` / `./dist/index.browser.js` (env-resolved) |
| `@ignaciano3/better-pdf/browser` | `./dist/index.browser.js` |
| `@ignaciano3/better-pdf/forms` | `./dist/forms/index.js` |
| `@ignaciano3/better-pdf/generate` | `./dist/generate/index.js` |
| `@ignaciano3/better-pdf/typegen` | `./dist/forms/typegen.js` |

Only symbols exported from those barrel files are covered by this policy. Specifically excluded:

- **Deep imports** into internal modules (e.g. `@ignaciano3/better-pdf/core/document`) — these are implementation details and may change or be removed at any time without notice.
- **Symbols tagged `@internal`** — these appear in type definitions for implementation reuse but are not part of the supported API.

## Deprecation Policy

When a public API is scheduled for removal:

1. It is marked `@deprecated` in JSDoc with a migration note pointing to the replacement.
2. It is kept for at least one minor release after the deprecation is introduced.
3. It is removed in the next major release.

This gives users a clear migration window and at least one release version to update before anything disappears.

## Non-Goals

The following are deliberately out of scope and will not be added in any version:

- **XFA forms** — Adobe's XML-based form format, deprecated by Adobe and removed from the ISO PDF standard. AcroForms are the supported form model.
- **Creating encrypted PDFs / re-encrypting** — reading and editing encrypted PDFs is supported via `PdfDocument.load(bytes, { password })` (RC4 / AES-128 / AES-256; use `""` for owner-locked files), and saving an edited encrypted PDF produces decrypted output. This library never encrypts its output; encrypt or re-encrypt with an external tool (e.g. `qpdf --encrypt`).
- **Lenient recovery of malformed / off-spec PDFs** — the parser is strict by design. Files that violate the PDF specification are rejected rather than silently misread.

See also the [README Non-Goals section](../README.md#non-goals) and [V1 Readiness notes](V1-READINESS.md).
