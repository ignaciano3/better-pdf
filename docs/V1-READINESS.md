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
- **Xref-stream / objstm on a real corpus** — only *generated* test variants exist.
  Any Office/Chrome-print PDF uses these. Get real PDF 1.5+ files into fixtures
  before claiming broad support.
- **WASM panic safety** — a panic in wasm aborts the instance, unrecoverable.
  Every `.unwrap()` reachable from input is a landmine (many in `fill.rs`).
  Confirm all input-reachable paths return `Result`, not panic. Run sustained
  fuzzing, not just the existing target.

## 5. Distribution proof

README hedges: *"expects a modern bundler/runtime that can serve the `.wasm`."*
That hedge = support tickets. Before V1, ship verified working examples:
Vite, webpack, Next.js, Deno, Bun, Node, Cloudflare Workers. "Runs everywhere"
was a core requirement — prove it.

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
