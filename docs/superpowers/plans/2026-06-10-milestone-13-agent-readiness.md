# Milestone 13 — Agent Readiness

**Status:** ✅ Implemented and merged.

**Goal:** Make better-pdf easy and safe for AI agents to drive, and harden the package for tree-shaking.

## What shipped

- **Agent skill** `skills/better-pdf/SKILL.md` — procedural knowledge for the
  load → inspect → generate types → fill/flatten/sign → save workflow, plus the
  non-obvious rules (use a field's *real* export values, never assume `Yes`/`On`;
  visual signatures are not cryptographic; `save()` is an incremental update).
  Shipped in the npm `files` list and installable via [skills.sh](https://www.skills.sh).
- **Tree-shakeability guard** — enabled `noUncheckedIndexedAccess` (the codebase
  was already clean under it) and added a test asserting `src/typegen.ts` imports
  are all `import type` (so the `better-pdf/typegen` subpath never pulls in the
  WASM core). `package.json` already declares `"sideEffects": false`.

## Files

- Create `skills/better-pdf/SKILL.md`.
- Modify `package.json` (`files` includes the skill), `tsconfig.json`
  (`noUncheckedIndexedAccess`), `tests/typegen.test.ts` (tree-shaking guard).

## Decisions

- The strongest agent-readiness feature is the M12 typed workflow: generate a
  types module and `doc.getForm<typeof myFormFields>()` turns hallucinated field
  names and invalid values into compile errors.

## Verification

- `bun test` (tree-shaking guard passes), `bun run typecheck` under
  `noUncheckedIndexedAccess`, and a manual check that `dist/typegen.js` has zero
  runtime imports.
