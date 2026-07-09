# qpdf-inspired test suite — scaffold & follow-up findings

Behavioral tests inspired by [QPDF](https://github.com/qpdf/qpdf), the reference
implementation for PDF *structure*. Scaffolded 2026-07-09 in
`tests/qpdf-ported.test.ts`. Complements the pypdf suite (forms / recovery) and
the pdf-lib suite (generation) by targeting structure: xref reconstruction,
incremental updates, object/xref streams, encryption R2–R6, linearization.

## Layout

- `tests/qpdf-ported.test.ts` — the tests, in three tiers (below).
- `tests/scripts/gen-qpdf-fixtures.ts` — generates the fixture PDFs via the qpdf
  CLI (encryption matrix, object streams, linearized). Run on a machine with
  qpdf installed; the fixture-dependent tiers skip until it has been run.
- `tests/fixtures/qpdf/` — generated fixtures + `LICENSE.qpdf` (provenance).
- `tests/qpdf-validate.test.ts` — pre-existing; uses qpdf in the *oracle*
  direction (re-parses better-pdf output, asserts severity is no worse).

## Tiers

### Tier 1 — Structure & recovery (inline byte fixtures) — GREEN
Hand-authored malformed/edge-case PDFs exercising the shapes QPDF's qtest suite
covers. No external files; runs everywhere. All 10 pass today:
- baseline sanity
- incremental update: appended redefinition (via `/Prev`) of a page wins
- multi-section `/Prev` chain: newest definition wins
- indirect stream `/Length` resolved
- missing xref table reconstructed by object scan
- bogus (all-zero) xref offsets reconstructed
- duplicate object number: last definition wins on reconstruct
- understated trailer `/Size` still resolves all objects
- leading junk before `%PDF-` tolerated
- comments in the body tolerated

### Tier 2 — Encryption matrix (qpdf `--encrypt`) — SKIPS until generated
R2 RC4-40, R3 RC4-128, R4 AES-128, R6 AES-256, across empty / user / owner
passwords, plus `isEncrypted` and `passwordType` (owner/user/wrong) against a
distinct-passwords file (`user=foo`, `owner=bar`). Cross-producer coverage:
qpdf is the encryptor, better-pdf the reader.

### Tier 3 — Object/xref streams & linearization (qpdf-produced) — SKIPS until generated
PDF-1.5+ compressed object streams + xref streams, and a linearized layout;
each must load and round-trip through `save()`.

## Populating Tiers 2–3

**CI does this automatically:** `.github/workflows/ci.yml` installs qpdf, then
runs `gen-qpdf-fixtures.ts` before `bun test`, so Tiers 2–3 run on every CI
build. The generated PDFs are gitignored (`tests/fixtures/qpdf/.gitignore`) —
CI regenerates them each run, so they can't drift across qpdf versions.

**Locally**, to run the tiers instead of skipping them:

```
brew install qpdf        # or: apt/dnf install qpdf
bun run tests/scripts/gen-qpdf-fixtures.ts
bun test tests/qpdf-ported.test.ts
```

Without qpdf, Tiers 2–3 skip (Tier 1 still runs). The generator exits with a
clear message if qpdf is absent.

## Findings

_None yet — Tiers 2–3 have not been run against generated fixtures. Record any
gaps QPDF-produced files surface here, in the numbered style of
`docs/pypdf-findings.md` (Was / Root cause / Fix / Tests)._

## Candidate future tiers (QPDF areas not yet touched)
- **Free-object / generation-number** handling (reused object numbers across
  updates, non-zero generations).
- **Recovery of damaged object streams** (truncated/!corrupt ObjStm).
- **Cross-reference stream edge cases** (hybrid-reference files with both a
  classic table and an xref stream — the `/XRefStm` pointer).
- **Page-tree inheritance** (MediaBox/Resources inherited from an intermediate
  `/Pages` node) — QPDF normalizes these.
