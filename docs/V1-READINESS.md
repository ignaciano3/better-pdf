# better-pdf — V1 Readiness Assessment

Status as of 0.15.0. What separates "feature-rich" from "stable V1".

Library is well past original scope (now creates PDFs, draws, merges — far beyond
"load → fill → save"). Quality is high: TDD, fuzz target, qpdf validation, real
fixture corpus. But "stable V1" needs more than features. Gaps ranked below.

---

## 1. Broken V1 requirement: multiline text-field fill ⚠️

`PLAN.md` mandates *"Set text on Text Fields **on Text Area Fields**"*.

- Multiline flag (`<< 12`) is handled in `create.rs:1227` (new fields) but **not**
  in `fill.rs` / `appearance.rs` when filling existing fields.
- README/docs confirm: *"Text fields are single-line; multi-line wrapping is not
  yet generated."*
- `drawText({ maxWidth })` word-wrap shipped in 0.14.0 — wire that same `wrapText`
  logic into the text-field appearance path.

This is a stated V1 requirement, not optional. **Do first.**

## 2. Documentation drift — fix before tagging

- README previously claimed merged forms aren't interactive; 0.15.0 reconstructs
  the AcroForm. **(Fixed — README Limitations corrected.)**
- Audit remaining README/docs Limitations vs current reality before tagging V1.

## 3. The actual meaning of V1: freeze the API

Still `0.x`; CHANGELOG says *"public API may change between minor releases."*
V1 = commitment. Before tagging:

- Lock the public TS surface. Decide what is final.
- Write a semver / deprecation policy.
- Audit naming consistency across the now-large API (`drawText` / `drawImage` /
  `drawSvgPath` / `fill*` / page ops).

## 4. Real-world robustness (biggest "stable" risk)

- **Encryption** — out of scope per PLAN; fine for the OSFATUN corpus. But a
  *general pdf-lib replacement* hits encrypted files constantly (even empty-owner-
  password). Minimum: detect + throw a clean `EncryptedPdfError`, document it.
  pdf-lib reads these.
- **Xref-stream / objstm on a real corpus** — synthetic coverage now includes a
  larger-objstm file (`ficha-objstreams-big.pdf`, ~419 KB, 8 ObjStm streams) and
  an incremental-update-over-xref-stream file (`ficha-objstreams-updated.pdf`,
  produced by our own incremental save with a `/Prev` pointer, 2 `startxref`
  occurrences), both wired into the fill/flatten round-trip and qpdf-validate
  loops. Genuine multi-producer files (Word, LibreOffice, Chrome, Acrobat),
  linearized PDFs, and hybrid-reference files remain a **post-1.0** gap — they
  require real input files or qpdf-based fixture generation.
- **WASM panic safety** — a panic in wasm aborts the instance, unrecoverable.
  Every `.unwrap()` reachable from input is a landmine (many in `fill.rs`).
  Confirm all input-reachable paths return `Result`, not panic. Run sustained
  fuzzing, not just the existing target.

## 5. Distribution proof ✓ DONE (0.21.0)

The `./wasm` export subpath is in `package.json`. Runtime examples ship in
`examples/runtimes/` with per-runtime READMEs. A per-runtime guide lives at
`docs/site/src/content/docs/guides/runtimes.md`. The README Limitations section
now names the exact init pattern instead of hedging.

**Verified end-to-end** (create + draw + save, valid `%PDF-` output confirmed in
this environment):
- Node v24.16.0 — via `pack-smoke.ts` installing from the packed tarball
- Bun v1.3.14 — via `pack-smoke.ts` installing from the packed tarball
- Browser — existing Playwright test
- Vite v5.4.21 — `npm run build` completed
- webpack v5.107.2 — `npm run build` completed
- Next.js v15.5.19 — `npm run build` completed

**Config provided** (runnable example + config shipped; toolchain absent here):
- Deno — `npm:` specifier example in `examples/runtimes/deno/`
- Cloudflare Workers — wrangler wasm-module binding example in
  `examples/runtimes/cloudflare-workers/`

## 6. Smaller — document-or-fix

- CMYK images rejected (common in print PDFs).
- SVG arcs (`A`/`a`) throw.
- Nested page trees rejected (rare but real — Adobe emits balanced trees).
- Multi-select listboxes unsupported.
- Publish benchmark numbers vs pdf-lib (`bench/` exists — show the win).

---

## Minimum for an honest V1

- #1 (requirement gap)
- #2 (docs)
- #3 (API freeze)
- panic-safety audit from #4

#4-encryption and #5 separate "V1 for my corpus" from "V1 as a pdf-lib
replacement". Pick which V1 is being claimed.

---

## Limitations vs Non-Goals (terminology — done)

- **Limitations** = future gaps, intend to close. Lives in README + docs site.
- **Non-Goals** = deliberately unsupported, not planned (legacy / rare / better
  served elsewhere). Currently: **XFA forms**, **lenient malformed-PDF recovery**.
