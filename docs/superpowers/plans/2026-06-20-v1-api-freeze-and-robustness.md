# V1 API-Freeze + Robustness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lock the public TypeScript API for a stable V1 (breaking cleanup), harden the Rust core against the few remaining input-reachable panics, and expand synthetic PDF-1.5+ (xref-stream/objstm) fixture coverage. Ships as **0.20.0** (the last breaking minor before 1.0.0).

**Architecture:** Three workstreams in one release. (A) Rust panic hardening — convert the 2 fragile bare-index sites in `draw.rs` and the 4 by-construction `.expect()` sites in `create.rs` to `Result` propagation; no behaviour change on valid input. (B) Synthetic fixtures — generate two new PDF-1.5+ variants achievable without qpdf (an incremental update over an xref-stream base; a larger multi-object objstm file) and wire them into the existing fill/flatten round-trip + qpdf-validate loops. (C) API freeze — unify shape draw-options to `fill`/`stroke`/`strokeWidth`, rename ellipse radii to `radiusX`/`radiusY`, export the three missing option types, make `@internal` fields truly private, align `modDate`/`setModificationDate`, and add a semver/stability policy doc with `@public` markers. All public-key renames are TS-only: the wire op field names (in `draw-queue.ts`) and the Rust serde contract stay unchanged.

**Tech Stack:** Rust (lopdf), wasm-bindgen, TypeScript, bun test, pdf-lib (devDep, fixture generation).

## Global Constraints
- Version bump to **0.20.0** in `package.json` and `crates/core/Cargo.toml` (+ `crates/core/Cargo.lock` via `cargo build`); the crate name in `Cargo.lock` is `better-pdf-core` (hyphens). 0.16–0.19 already shipped.
- `source ~/.cargo/env` before any cargo/wasm command.
- Rust must pass `cargo test --manifest-path crates/core/Cargo.toml` and `cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings` (the strict `--all-targets` form — test code included).
- TS must pass `bun test` AND `bun run typecheck` (`tsc --noEmit`). `bun test` strips types, so typecheck is a separate, required gate.
- Build wasm with `bun run build` before running TS tests that round-trip through the core.
- **Public-key renames are TS-only.** Do NOT change wire op field names in `src/generate/draw-queue.ts` or any Rust serde field. Rename only the public option-bag keys in `src/generate/page.ts` interfaces and map them to the unchanged wire fields inside each draw method.
- Every commit ends with the trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do NOT tag 1.0.0 — it still depends on V1-READINESS #2 (docs audit) and #5 (distribution proof), out of scope here. This release prepares the frozen surface only.

---

## Task 1 — Rust panic hardening: draw.rs bare indexes + create.rs expects

Convert the only input-adjacent panic sites the audit found (all currently safe-by-construction, but fragile) to `Result` propagation. No behaviour change for valid input; malformed/edge input gets a clean `Err` instead of a potential abort.

**Files:**
- Modify `crates/core/src/draw.rs` (lines ~1435 and ~1462-1467)
- Modify `crates/core/src/create.rs` (lines ~1167-1169 and ~1682-1683)

**Interfaces:**
- Consumes: existing `apply_draw_ops` error channel (functions already return `Result<_, String>`).
- Produces: no new public symbols; same function signatures, more error paths.

### Steps

- [ ] **1.1 Harden `draw.rs:1435` (current-page index).** Replace:
  ```rust
              sorted_pages.sort_by_key(|(num, _)| *num);
              sorted_pages[*page_idx].1
          };
  ```
  with:
  ```rust
              sorted_pages.sort_by_key(|(num, _)| *num);
              sorted_pages
                  .get(*page_idx)
                  .map(|(_, id)| *id)
                  .ok_or_else(|| format!("page index {page_idx} out of range"))?
          };
  ```

- [ ] **1.2 Harden `draw.rs:1462-1467` (goToPage destination index).** The destination is resolved inside a `go_to_page.map(|target| { ... sorted[target].1 })` closure, which cannot use `?`. Replace the `dest_page` binding:
  ```rust
                  let dest_page = go_to_page.map(|target| {
                      let prev = inc.get_prev_documents();
                      let mut sorted: Vec<(u32, ObjectId)> = prev.get_pages().into_iter().collect();
                      sorted.sort_by_key(|(num, _)| *num);
                      sorted[target].1
                  });
  ```
  with a fallible form that propagates:
  ```rust
                  let dest_page = match go_to_page {
                      Some(target) => {
                          let prev = inc.get_prev_documents();
                          let mut sorted: Vec<(u32, ObjectId)> =
                              prev.get_pages().into_iter().collect();
                          sorted.sort_by_key(|(num, _)| *num);
                          Some(
                              sorted
                                  .get(target)
                                  .map(|(_, id)| *id)
                                  .ok_or_else(|| {
                                      format!("link goToPage index {target} out of range")
                                  })?,
                          )
                      }
                      None => None,
                  };
  ```

- [ ] **1.3 Harden the 4 `create.rs` expects.** At `create.rs:1167-1169` and `create.rs:1682-1683`, replace the `.expect("page must exist")` / `.expect("page must be a dict")` pairs. Read each site; convert the `Option` access via `.get(..)`/`.get_mut(..)` and the `.as_dict_mut()` to `?` with a descriptive error, e.g.:
  ```rust
  // was: ...get_mut(&page_id).expect("page must exist").as_dict_mut().expect("page must be a dict")
  let page_obj = doc
      .objects
      .get_mut(&page_id)
      .ok_or_else(|| format!("internal: page object {page_id:?} missing"))?;
  let page_dict = page_obj
      .as_dict_mut()
      .map_err(|e| format!("internal: page object is not a dict: {e}"))?;
  ```
  Keep the surrounding logic identical; only the panic→`?` conversion changes. Confirm both functions already return `Result<_, String>` (they do — they use `?` elsewhere).

- [ ] **1.4 Add a regression test for the goToPage out-of-range path.** In the `#[cfg(test)]` module of `crates/core/src/draw.rs`, add a test that builds/loads a 1-page doc and applies a `link` draw op whose `goToPage` is out of range (e.g. 99), asserting `apply_draw_ops` returns an `Err` containing `"out of range"` rather than panicking. Model it on the existing draw-op tests in that module (find one that constructs an ops JSON + calls the entry point). If constructing the op JSON directly is awkward, instead add the test at the smallest layer that reaches the `dest_page` resolution. RED → confirm it fails (panic) before 1.2, GREEN after. (If you implement 1.2 first, write the test to lock the new behaviour and confirm it passes.)

- [ ] **1.5 Run the full Rust suite + strict clippy.**
  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml
  cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings
  ```
  Both clean.

- [ ] **1.6 Commit.**
  ```
  git add crates/core/src/draw.rs crates/core/src/create.rs
  git commit -m "fix(core): propagate page-index errors instead of panicking

  Convert the two fragile bare-index sites in draw.rs (current page +
  link goToPage destination) and the four by-construction expects in
  create.rs to Result propagation, so a malformed op or pathological
  page tree returns a clean Err instead of risking a wasm abort.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 2 — Synthetic PDF-1.5+ fixtures (incremental-on-xref-stream + larger objstm)

Expand xref-stream/objstm parser coverage beyond the single existing `ficha-objstreams.pdf`. Two new generated fixtures, both achievable without qpdf, wired into the existing round-trip + qpdf-validate tests.

**Files:**
- Modify `scripts/make-fixtures.ts` (add two generators)
- Create `tests/fixtures/generated/ficha-objstreams-big.pdf` (generated, committed)
- Create `tests/fixtures/generated/ficha-objstreams-updated.pdf` (generated, committed)
- Modify `tests/objstreams.test.ts` (read/fill/flatten round-trip on both)
- Modify `tests/qpdf-validate.test.ts` (add both to its fixture list)
- Modify `docs/V1-READINESS.md` (note expanded synthetic coverage + remaining real-producer gap)

**Interfaces:**
- Consumes: `PDFDocument` from pdf-lib (already imported in make-fixtures.ts); the built `better-pdf` core for the incremental fixture.
- Produces: two committed fixture PDFs + tests referencing them by path.

### Steps

- [ ] **2.1 Generate `ficha-objstreams-big.pdf` (larger objstm).** In `scripts/make-fixtures.ts`, add a generator block (mirror the existing `ficha-objstreams.pdf` block's style): load `FICHA`, then `copyPages` the source page into the doc several times (e.g. embed/append ~8 copies so the object count is much larger than the single-form file), then `doc.save({ useObjectStreams: true, updateFieldAppearances: false })` and write to `tests/fixtures/generated/ficha-objstreams-big.pdf`. Use pdf-lib's `copyPages(src, [0])` + `addPage` API. Document in a comment that this stresses object-stream decoding at higher object counts. Run `bun run scripts/make-fixtures.ts` (or the documented `bun run fixtures:generate`) and confirm the file exists and `grep -a "/ObjStm" tests/fixtures/generated/ficha-objstreams-big.pdf` matches.

- [ ] **2.2 Generate `ficha-objstreams-updated.pdf` (incremental update over a 1.5 base).** This fixture must be produced by **our own** core's incremental save so it has a base xref-stream plus an appended update section with `/Prev`. Add a generator that: builds the wasm package first if needed, loads `tests/fixtures/generated/ficha-objstreams.pdf` through the public `PdfDocument.load`, fills one text field (a known field from `FICHA`, e.g. `beneficiario.apellidos_nombres`), `await doc.save()`, and writes the bytes to `tests/fixtures/generated/ficha-objstreams-updated.pdf`. If wiring our own ESM build into `make-fixtures.ts` is awkward, instead create this fixture from a small dedicated script `scripts/make-objstream-update-fixture.ts` that imports from `../src/index.ts` and document the two-step generation in a comment. Run it (after `bun run build`) and confirm the file exists and contains at least two `startxref` occurrences (`grep -ac "startxref" tests/fixtures/generated/ficha-objstreams-updated.pdf` ≥ 2 — base + update).

- [ ] **2.3 Add round-trip tests in `tests/objstreams.test.ts`.** For each new fixture, add a test mirroring the existing "fills and reloads an xref-stream PDF" test: load, read a field value, fill a field, save, reload, assert the new value round-trips; and a flatten test asserting the field is gone after flatten+reload. Use `beneficiario.apellidos_nombres` (text) as the target. Build wasm first.
  ```
  source ~/.cargo/env && bun run build
  bun test tests/objstreams.test.ts
  ```
  Expect PASS (these exercise our parser on the richer layouts).

- [ ] **2.4 Add both fixtures to `tests/qpdf-validate.test.ts`.** Append the two new fixture paths to the fixture list that test iterates (the `dir`/`rel` list near the top). The test auto-skips when qpdf is absent (it is locally), so this only adds coverage where qpdf is installed (CI). Confirm the test file still type-checks and runs (skips locally):
  ```
  bun test tests/qpdf-validate.test.ts
  ```

- [ ] **2.5 Update `docs/V1-READINESS.md`.** In the "Xref-stream / objstm on a real corpus" bullet (#4), note that synthetic coverage now includes a larger-objstm file and an incremental-update-over-xref-stream file, and that genuine multi-producer (Word/LibreOffice/Chrome/Acrobat), linearized, and hybrid-reference files remain a **post-1.0** gap requiring real input files or qpdf-in-fixture-gen. Keep it one short paragraph.

- [ ] **2.6 Run the full TS suite + typecheck.**
  ```
  source ~/.cargo/env && bun run build
  bun test
  bun run typecheck
  ```
  All green.

- [ ] **2.7 Commit.**
  ```
  git add scripts/ tests/fixtures/generated/ficha-objstreams-big.pdf tests/fixtures/generated/ficha-objstreams-updated.pdf tests/objstreams.test.ts tests/qpdf-validate.test.ts docs/V1-READINESS.md
  git commit -m "test(fixtures): add larger-objstm and incremental-over-xref-stream PDFs

  Expand synthetic PDF-1.5+ coverage with a higher-object-count
  object-stream file and an incremental update appended over an
  xref-stream base (our own incremental save). Both wired into the
  fill/flatten round-trip and qpdf-validate loops. Real multi-producer /
  linearized / hybrid coverage remains a documented post-1.0 gap.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 3 — API: unify shape draw-options to `fill` / `stroke` / `strokeWidth` (BREAKING, TS-only)

Replace the three inconsistent shape-option conventions with one. **Public keys only** — the wire op fields in `draw-queue.ts` and the Rust serde contract are unchanged; each draw method maps the new public key to the existing wire field.

Rename map (public option keys):

| Method | Old public key(s) | New public key(s) | Unchanged wire field |
|---|---|---|---|
| `drawRectangle` | `color`, `borderColor`, `borderWidth` | `fill`, `stroke`, `strokeWidth` | `color`, `borderColor`, `borderWidth` |
| `drawEllipse` | `color`, `borderColor`, `borderWidth` | `fill`, `stroke`, `strokeWidth` | `color`, `borderColor`, `borderWidth` |
| `drawLine` | `color`, `thickness` | `stroke`, `strokeWidth` | `color`, `thickness` |
| `drawSvgPath` | `fill`, `stroke`, `strokeWidth` | (unchanged) | (unchanged) |
| `drawPolygon` | `fill`, `stroke`, `strokeWidth` | (unchanged) | (unchanged) |

Note: `drawRectangle`/`drawEllipse` interior color is currently `color` → becomes `fill`; their outline `borderColor`/`borderWidth` → `stroke`/`strokeWidth`. `drawLine` has no interior; its `color` is a stroke → becomes `stroke`, and `thickness` → `strokeWidth`.

**Files:**
- Modify `src/generate/page.ts` (option interfaces `DrawRectangleOptions`, `DrawEllipseOptions`, `DrawLineOptions`; the destructuring + wire-mapping in `drawRectangle`/`drawEllipse`/`drawLine`; the `validateBorderWidth` call labels and JSDoc)
- Modify any test that uses the old keys: `tests/` (grep `borderColor|borderWidth|thickness` in tests)
- Modify docs that show the old keys (README, docs site) — handled in Task 7's doc sweep, but update inline JSDoc here.

**Interfaces:**
- Consumes: `Color` type, `tuple()` helper, existing wire op builders (unchanged).
- Produces: renamed public option interfaces. Other tasks/tests must use `fill`/`stroke`/`strokeWidth`.

### Steps

- [ ] **3.1 Write/adjust failing tests first.** In the relevant test file(s) for shape drawing (grep `drawRectangle\|drawEllipse\|drawLine` under `tests/`), change the option keys to the new names (`fill`/`stroke`/`strokeWidth`) in a representative test for each of the three methods, and assert the produced wire op still carries the correct values (the tests that inspect `toPayload()`/ops JSON should still see wire `borderColor`/`thickness` etc.). Run them and confirm they FAIL against the current code (old keys still expected).
  ```
  bun test tests/<shape-test-file>.test.ts
  ```

- [ ] **3.2 Rename the public interfaces in `page.ts`.** Update `DrawRectangleOptions` and `DrawEllipseOptions`: `color?` → `fill?`, `borderColor?` → `stroke?`, `borderWidth?` → `strokeWidth?`. Update `DrawLineOptions`: `color?` → `stroke?`, `thickness?` → `strokeWidth?`. Update each interface's JSDoc to describe fill vs stroke.

- [ ] **3.3 Update the draw-method bodies in `page.ts`.** In `drawRectangle` (~line 341), `drawEllipse` (~line 418), and `drawLine` (~line 311): destructure the new public keys, then map to the UNCHANGED wire fields when building the op. Examples:
  - `drawRectangle`/`drawEllipse`: `const { ..., fill, stroke, strokeWidth, ... } = options;` then emit `...(fill !== undefined ? { color: tuple(fill) } : {})`, `...(stroke !== undefined ? { borderColor: tuple(stroke) } : {})`, `...(strokeWidth !== undefined ? { borderWidth: strokeWidth } : {})`.
  - `drawLine`: `const { start, end, stroke, strokeWidth, opacity } = options;` then emit wire `...(stroke !== undefined ? { color: tuple(stroke) } : {})`, `...(strokeWidth !== undefined ? { thickness: strokeWidth } : {})`.
  - Update `validateBorderWidth(strokeWidth)` / `validateBorderWidth(strokeWidth, "strokeWidth")` call labels so error messages reference the new key.

- [ ] **3.4 Run the shape tests, expect PASS.**
  ```
  source ~/.cargo/env && bun run build
  bun test tests/<shape-test-file>.test.ts
  ```

- [ ] **3.5 Sweep remaining old-key usages.** `grep -rn "borderColor\|borderWidth\|thickness" src/ tests/` — every remaining hit must be either a wire field in `draw-queue.ts` (KEEP) or a Rust-bound name (KEEP), not a public option. Fix any test/example still using old public keys. Run full `bun test` + `bun run typecheck`.

- [ ] **3.6 Commit.**
  ```
  git add src/generate/page.ts tests/
  git commit -m "feat(api)!: unify shape draw-options to fill/stroke/strokeWidth

  BREAKING: drawRectangle/drawEllipse now take { fill, stroke, strokeWidth }
  (was color/borderColor/borderWidth); drawLine takes { stroke, strokeWidth }
  (was color/thickness). drawSvgPath/drawPolygon already used this. Wire
  format and Rust core are unchanged; only the public option keys moved.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 4 — API: rename ellipse radii + export missing option types (BREAKING + additive, TS-only)

**Files:**
- Modify `src/generate/page.ts` (`DrawEllipseOptions.xScale`/`yScale` → `radiusX`/`radiusY`; `drawEllipse` body)
- Modify `src/index.ts` and `src/index.browser.ts` (export `DrawLinkOptions`, `DrawSvgPathOptions`, `DrawPolygonOptions`)
- Modify `src/generate/index.ts` (export the same three if it re-exports option types)
- Modify tests using `xScale`/`yScale`

**Interfaces:**
- Produces: `DrawEllipseOptions` with `radiusX`/`radiusY`; the three option types reachable from root barrels.

### Steps

- [ ] **4.1 Failing test for the radii rename.** In the ellipse test, change `xScale`/`yScale` to `radiusX`/`radiusY`; run, expect FAIL.

- [ ] **4.2 Rename in `page.ts`.** `DrawEllipseOptions`: `xScale` → `radiusX`, `yScale` → `radiusY` (keep them required). In `drawEllipse` (~418): destructure `radiusX`/`radiusY`, update the `<= 0` guards and error messages (`radiusX must be > 0`), and map to the UNCHANGED wire op fields `xScale`/`yScale` (`{ xScale: radiusX, yScale: radiusY }`). Update the method JSDoc ("horizontal radius `radiusX`").

- [ ] **4.3 Run ellipse test, expect PASS.** `bun run build && bun test tests/<ellipse-test>.test.ts`.

- [ ] **4.4 Export the three missing option types.** Add `DrawLinkOptions`, `DrawSvgPathOptions`, `DrawPolygonOptions` to the `export type { ... }` block (from `./generate/page.js`) in BOTH `src/index.ts` and `src/index.browser.ts`, alongside the existing `DrawTextOptions` etc. If `src/generate/index.ts` re-exports option types, add them there too.

- [ ] **4.5 Typecheck + full suite.**
  ```
  source ~/.cargo/env && bun run build
  bun run typecheck
  bun test
  ```
  Green. Confirm the three types are importable from the root: add a one-line type-only import assertion to `tests/types/typed-form.types.ts` or a small new `tests/types/draw-options.types.ts` that imports all three from `../../src/index.ts` (compile-time check).

- [ ] **4.6 Commit.**
  ```
  git add src/generate/page.ts src/index.ts src/index.browser.ts src/generate/index.ts tests/
  git commit -m "feat(api)!: rename ellipse radii to radiusX/radiusY; export Draw*Options

  BREAKING: drawEllipse takes { radiusX, radiusY } (was xScale/yScale) —
  they are radii in points, not scale factors. Also export the previously
  omitted DrawLinkOptions / DrawSvgPathOptions / DrawPolygonOptions from
  both entry points. Wire format unchanged.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 5 — API: make `@internal` fields truly private + align metadata naming (BREAKING where observable)

Stop leaking implementation fields on the public type surface, and align the metadata getter/setter naming.

**Files:**
- Modify `src/generate/font.ts` (`PdfFont._fontId`, `_bytes` → private; the consumer in `page.ts drawText` needs access)
- Modify `src/generate/image.ts` / wherever `PdfImage.bytes` and `EmbeddedPdfPage.bytes` live → private
- Modify `src/forms/form.ts` (`PdfForm.queue`, `flattenQueue` → private)
- Modify `src/core/metadata.ts` + the `DocumentMetadata` type (`modDate` → `modificationDate`)
- Modify any internal consumers + tests

**Interfaces:**
- Produces: `DocumentMetadata.modificationDate`; internal fields no longer in the public type surface.

### Steps

- [ ] **5.1 Survey access patterns first (read-only).** `grep -rn "\._fontId\|\._bytes\|\.bytes\b\|\.queue\b\|\.flattenQueue\|modDate" src/`. Identify each cross-class reader. For fields read by another class in the same package (e.g. `page.ts` reading `font._fontId`), the clean fix is a module-private `Symbol`-keyed property OR keeping the field but marking it `private` and exposing a package-internal accessor. Choose the lowest-churn approach that removes the field from the PUBLIC `.d.ts` surface: TypeScript `private`/`#private` hides it from consumers; if a sibling class must read it, use an `/** @internal */` accessor method or a shared module symbol. Document the chosen pattern in the task report.

- [ ] **5.2 `modDate` → `modificationDate` (failing test first).** Update the metadata round-trip test to expect `metadata.modificationDate`; run, expect FAIL. Then rename the property in the `DocumentMetadata` interface and in `getMetadata`'s wire-mapping (`src/core/metadata.ts` / `document.ts`). The setter `setModificationDate` already matches; only the returned interface field changes. Run, expect PASS.

- [ ] **5.3 Make the internal fields private.** Apply the chosen pattern from 5.1: convert `_fontId`/`_bytes`/`bytes`/`queue`/`flattenQueue` to `private`/`#private` (or symbol-keyed), updating the few in-package readers to use the internal accessor. The PUBLIC `PdfFont`/`PdfImage`/`EmbeddedPdfPage`/`PdfForm` classes must no longer expose these in their type surface.

- [ ] **5.4 Add a `PdfFont.name` JSDoc caveat.** `PdfFont.name` is meaningless for embedded fonts (set to a Helvetica placeholder). Add a JSDoc note: for embedded fonts this returns a placeholder and should not be relied on. (No rename — documentation only.)

- [ ] **5.5 Full verification.**
  ```
  source ~/.cargo/env && bun run build
  bun run typecheck
  bun test
  ```
  All green. Typecheck is the load-bearing gate here (private-field leakage and the `modDate` rename surface only in `tsc`).

- [ ] **5.6 Commit.**
  ```
  git add src/ tests/
  git commit -m "feat(api)!: hide internal fields; rename DocumentMetadata.modDate

  BREAKING: DocumentMetadata.modDate is now modificationDate (matches
  setModificationDate). PdfFont/_PdfImage/EmbeddedPdfPage/PdfForm internal
  fields (_fontId, _bytes, bytes, queue, flattenQueue) are no longer part
  of the public type surface. PdfFont.name documented as a placeholder for
  embedded fonts.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 6 — Stability policy doc + `@public` markers + stale-comment cleanup

Make the V1 commitment explicit. No behaviour change.

**Files:**
- Create `docs/STABILITY.md` (semver + deprecation policy)
- Modify `src/index.ts` / `src/index.browser.ts` (top-of-file `@public` banner or per-export tags — see step)
- Modify `src/generate/fonts.ts` (remove the stale "Revisit in M24 if requested" comment on `StandardFonts`)
- Modify `README.md` (link to STABILITY.md; brief stability statement) — minimal here, full doc sweep is Task 7

**Interfaces:** none (docs + annotations).

### Steps

- [ ] **6.1 Write `docs/STABILITY.md`.** Content: (a) from 1.0.0 the package follows semver — breaking changes to the documented public API only in major versions; (b) the public API = everything exported from the package root (`@ignaciano3/better-pdf`) and the documented subpath entries; deep imports and `@internal` symbols are NOT covered; (c) deprecation policy — deprecated APIs are marked `@deprecated` with a migration note and kept for at least one minor before removal in the next major; (d) what is explicitly out of scope (the documented Non-Goals: XFA, encryption, lenient malformed-PDF recovery). Keep it ~1 page.

- [ ] **6.2 Mark the public surface.** Add a top-of-file `/** @public */`-style banner comment to `src/index.ts` and `src/index.browser.ts` stating that all exports from this barrel constitute the stable public API as of 1.0.0 (and `@internal` symbols are excluded). If the repo prefers per-export tags, tag the barrel instead of every symbol — the goal is one discoverable marker, not 60 annotations.

- [ ] **6.3 Remove the stale milestone comment.** In `src/generate/fonts.ts`, delete/replace the `StandardFonts` "Revisit in M24 if requested" comment (Symbol/ZapfDingbats omission). Replace with a neutral note that those two non-Latin standard-14 faces are intentionally not exposed, or a GitHub-issue reference — no milestone jargon.

- [ ] **6.4 Verify build + typecheck (no behaviour change).**
  ```
  source ~/.cargo/env && bun run build
  bun run typecheck
  bun test
  ```
  Green.

- [ ] **6.5 Commit.**
  ```
  git add docs/STABILITY.md src/index.ts src/index.browser.ts src/generate/fonts.ts README.md
  git commit -m "docs(api): add STABILITY.md semver policy; mark public surface

  Document the post-1.0 semver + deprecation policy and what the public
  API surface is. Mark the root barrels as @public. Remove the stale
  'Revisit in M24' StandardFonts comment.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 7 — Docs sweep + migration guide + CHANGELOG + version 0.20.0

Update every doc that shows the renamed options, add a migration guide for the breaking changes, and bump the version.

**Files:**
- Modify `README.md`, `docs/site/src/content/docs/**` (any page showing `color`/`borderColor`/`borderWidth`/`thickness`/`xScale`/`yScale` for shapes, or `modDate`)
- Modify `docs/migrating/from-pdf-lib.md` and `docs/migrating-from-pdf-lib.md` (note better-pdf uses `fill`/`stroke`/`strokeWidth`, mapping from pdf-lib's `color`/`borderColor`/`thickness`)
- Create a `## Migration: 0.19 → 0.20` section (in CHANGELOG or a `docs/migrating/0.19-to-0.20.md`)
- Modify `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`, `crates/core/Cargo.lock`
- Regenerate API docs via `bun run docs` (typedoc) only if it runs clean and the output isn't gitignored

### Steps

- [ ] **7.1 Doc sweep for renamed keys.** `grep -rn "borderColor\|borderWidth\|thickness\|xScale\|yScale\|modDate" README.md docs/` (excluding generated/gitignored `docs/api/*`). Update every shape-drawing example to `fill`/`stroke`/`strokeWidth`, every ellipse example to `radiusX`/`radiusY`, and metadata reads to `modificationDate`. Verify no public-facing doc still teaches an old key.

- [ ] **7.2 Migration guide 0.19→0.20.** Write a concise migration section listing each breaking change with a before/after: shape options table, ellipse radii, `DocumentMetadata.modDate` → `modificationDate`, and the now-private internal fields (note: only affects code that reached into `@internal` fields). Place it where the repo keeps migration docs (check `docs/migrating/`); link it from CHANGELOG.

- [ ] **7.3 Version bump to 0.20.0.** `package.json` `"version": "0.20.0"`, `crates/core/Cargo.toml` `version = "0.20.0"`, then `source ~/.cargo/env && cargo build --manifest-path crates/core/Cargo.toml` to sync `Cargo.lock` (verify `better-pdf-core` → 0.20.0). Bump `crates/wasm/Cargo.toml` too if it exists.

- [ ] **7.4 CHANGELOG.** Insert a new `## [0.20.0] - 2026-06-20` section between `## [Unreleased]` (kept empty on top) and `## [0.19.0]`. Sections: **Changed (BREAKING)** — shape draw-options unified to fill/stroke/strokeWidth; drawEllipse radiusX/radiusY; DocumentMetadata.modDate→modificationDate; internal fields hidden. **Added** — DrawLinkOptions/DrawSvgPathOptions/DrawPolygonOptions exported; STABILITY.md policy; larger-objstm + incremental-over-xref-stream fixtures. **Fixed** — page-index panics converted to clean errors (draw.rs/create.rs). Link the migration guide. Do NOT modify older sections.

- [ ] **7.5 Regenerate API docs (optional).** Check `git check-ignore docs/api/classes/PdfPage.md`. If `docs/api/*` is gitignored (it is per prior cycles), skip `bun run docs` and rely on source JSDoc. If not gitignored and `bun run docs` runs clean, regenerate.

- [ ] **7.6 Final full verification — run all, confirm pass.**
  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml
  cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings
  bun run build
  bun run typecheck
  bun test
  ```

- [ ] **7.7 Commit.**
  ```
  git add -A
  git commit -m "docs: shape-option migration guide; release 0.20.0 (API freeze prep)

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

- [ ] **7.8 Merge to master** (repo convention: merge finished branches locally, skip the merge/PR menu). Do not push or tag — the user pushes manually, and 1.0.0 awaits the docs audit (#2) and distribution proof (#5).

---

## Done criteria

- No input-reachable bare-index/`expect` panic remains in `draw.rs`/`create.rs` page-index resolution; a goToPage out-of-range op returns a clean `Err`.
- Two new synthetic PDF-1.5+ fixtures exist and pass fill/flatten round-trip; remaining real-producer gap documented in V1-READINESS.
- Public shape draw-options are uniformly `fill`/`stroke`/`strokeWidth`; ellipse uses `radiusX`/`radiusY`; the three Draw*Options types export from both barrels; wire format + Rust unchanged (no Rust serde edits).
- Internal fields are off the public type surface; `DocumentMetadata.modificationDate` replaces `modDate`.
- `docs/STABILITY.md` documents the semver/deprecation policy; root barrels marked `@public`; stale M24 comment gone.
- Migration guide covers every breaking change; CHANGELOG has a 0.20.0 section; `package.json`/`Cargo.toml`/`Cargo.lock` at 0.20.0.
- `cargo test` + `cargo clippy --all-targets -D warnings` + `bun test` + `bun run typecheck` + `bun run build` all green.

## Self-review notes (for the executor)
- Every public-key rename keeps the wire op name — if you find yourself editing `draw-queue.ts` field names or a Rust serde `rename`, STOP: that breaks the contract; map in `page.ts` instead.
- `bun test` passing is NOT sufficient — Bun strips types. The breaking renames and private-field changes only fail under `bun run typecheck`. Run it every task that touches TS types.
- Use `cargo clippy --all-targets` (not the bare `-- -D warnings`) — test-code dead helpers fail only under `--all-targets`.
