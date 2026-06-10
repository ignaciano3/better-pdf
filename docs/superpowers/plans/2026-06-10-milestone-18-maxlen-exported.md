# Milestone 18 — maxLength + exported Field Metadata

**Status:** ✅ Implemented and merged.

**Goal:** Close the two in-scope field-operation gaps found by auditing pdf-lib's
field API against ours.

## Background — pdf-lib parity audit

Compared pdf-lib's `cjs/api/form/*` against our API. Most unimplemented
operations are **out of scope** for a fill/flatten library: field and widget
creation (`create*`, `addToPage`, `removeField`), field-flag mutators
(`enable/disable*`), font-size/alignment writes, image text-fields, and XFA. We
already honor `/Q` text alignment (left/center/right) in the appearance engine.

Two gaps were genuinely in scope and cheap:

1. text `/MaxLen` (pdf-lib `getMaxLength`) — not read.
2. the `NoExport` flag (pdf-lib `isExported`) — not exposed.

## What shipped

- **`FieldInfo.maxLength: number | null`** — reads a text field's inheritable
  `/MaxLen` (gated to the `text` type; `null` otherwise).
- **`FieldInfo.exported: boolean`** — `!(Ff & 4)` (the `NoExport` flag).
- **`PdfTextField.setText`** throws the new **`MaxLengthExceededError`**
  (`field`, `maxLength`, `actualLength`) when a value exceeds `/MaxLen`.
- The type generator emits both `maxLength` and `exported`.

## Deferred (documented limitations, not gaps to fix for v1)

- Multi-line text wrapping, comb fields, and multi-select choice fields.

## Files

- Modify `crates/core/src/forms.rs` (read `/MaxLen` + `exported`, Rust test),
  `src/form.ts`, `src/errors.ts`, `src/fields.ts`, `src/index.ts`,
  `src/index.browser.ts`, `src/typegen.ts`, `tests/typegen.test.ts`,
  `tests/listbox.test.ts`, `README.md`, `CHANGELOG.md`, `skills/better-pdf/SKILL.md`.
- Add `tests/text-maxlen.test.ts`.

## Verification

- Rust tests + clippy, `bun test` (35), `bun run typecheck`, `bun run build:js`.
