# Milestone 17 — Typed Errors, getListBox, API Docs + Browser Test

**Status:** ✅ Implemented and merged.

**Goal:** Improve consumer ergonomics (typed errors, a list-box write accessor)
and shore up the release with generated API docs and a real browser test.

## What shipped

- **Typed error classes** (`src/errors.ts`) — `PdfError` base plus
  `UnknownFieldError`, `FieldTypeError`, `InvalidOptionError`, and
  `MissingOnStateError`, exported from both entry points. Each carries structured
  fields (e.g. `FieldTypeError.actual`/`.expected`). Messages were preserved so
  existing regex-based tests still pass. `PdfError` sets `this.name` from
  `new.target.name` so the concrete subclass name survives minification.
- **`getListBox(name).select(value)`** — a `PdfListBox<Opt>` write accessor for
  single-select list boxes, plus a narrowed `TypedPdfForm.getListBox`. The Rust
  fill engine already handled `listbox` identically to `dropdown`
  (`fill.rs`), so this was a pure TypeScript addition with no core change.
  List boxes are now writable (no longer read-only).
- **TypeDoc** (`bun run docs` → `docs/api`, gitignored) via
  `typedoc-plugin-markdown`; exported `FieldMeta` so the schema types resolve.
- **Real headless-browser test** (`scripts/browser-real-test.ts`,
  `bun run test:browser`) — serves the `--target web` build, loads it in headless
  Chromium via Playwright, runs load → read → fill → save in the page, and asserts
  fields were read and a valid `%PDF-` came back. Wired into CI.
- Added `crates/core/LICENSE` so the published WASM package carries it.

## Files

- Create `src/errors.ts`, `scripts/browser-real-test.ts`, `typedoc.json`,
  `tests/errors.test.ts`, `tests/listbox.test.ts`, `crates/core/LICENSE`.
- Modify `src/fields.ts`, `src/form.ts`, `src/schema.ts`, `src/index.ts`,
  `src/index.browser.ts`, `tests/types/typed-form.types.ts`, `.github/workflows/ci.yml`,
  `tsconfig.json` (include `scripts`), `package.json`, `README.md`, `CHANGELOG.md`,
  `skills/better-pdf/SKILL.md`.

## Deferred

- Multi-select list boxes / dropdowns remain single-select (documented limit).
