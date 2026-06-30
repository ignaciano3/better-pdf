# Code-Quality Refactor Plan (Rust core + TS layer)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Tier 0 steps use checkbox (`- [ ]`) syntax.

**Goal:** Reduce duplication, shrink god functions/classes, and cut hot-path work — without changing public API behavior or PDF output bytes.

**Scope:** `crates/core/src` (Rust → WASM) and `src` (TS wrapper).

**Tech Stack:** Rust (lopdf 0.41) compiled to WASM (`-Oz`, `panic=abort`, `lto`), TypeScript wrapper, Bun test runner, `cargo test`.

## Global Constraints

- **No behavior or output-byte changes.** These are pure refactors; existing tests must stay green and PDF output must be byte-identical where it is today.
- Hot path is **load → mutate → save**; prefer opt-in/lazy work, measure perf changes against `bench/bench.ts`.
- WASM must be rebuilt (`bun run build:wasm`) after Rust changes before TS tests run against it.
- `cargo test` for Rust, `bun test` for TS.

---

## How this plan was produced

Two parallel analysis agents (Rust core, TS layer) plus direct verification. Key structural facts confirmed:

- `create::create_document_json` is **~1,500 lines** (`create.rs:527-2034`), the single worst god function.
- `draw::apply_draw_ops_json` is **~870 lines** (`draw.rs:885-1752`).
- `create.rs` and `draw.rs` are **two implementations of one content-drawing engine** (duplicate op enums, validation, matrix math); they share only the low-level emit primitives. The string `"opacity must be in 0..1"` is written **14×**.
- `PdfDocumentBase` (`document.ts`, 670 lines) is **two classes (create/load) in one trenchcoat**; ~half its methods open with `if (this.mode …) throw`.
- Text wrapping is implemented **twice** (TS `wrap-text.ts` vs Rust `appearance.rs:78 wrap_lines`) and has **already drifted** (Rust normalizes CRLF, TS does not). The TS copy calls `wasm.measureText` **once per word-candidate** (~N boundary crossings per paragraph).
- `save()` (`document.ts:118-141`) chains up to **6 sequential full-document WASM round-trips**, each re-parsing/re-serializing the whole PDF.
- `wasm.ts` ≈ `wasm-browser.ts` (16 fns ×2, only differ by an init guard); `index.ts` ≈ `index.browser.ts` — **already drifted: browser build is missing `assemble()` and `merge()`**.

---

## Tier 0 — Quick wins (low effort, mechanical, do first)

These unblock the larger splits and immediately reduce line count. No behavior change.

### Task 0a: Rust shared validators
**Files:** `crates/core/src/create.rs`, `crates/core/src/draw.rs` (+ new module or top of a shared file)

Extract `check_page(page, count)`, `check_opacity(o)`, `check_finite(&[f32], names)`, `check_color(c)` — currently duplicated near-verbatim between `create.rs:567-610` and `draw.rs:913-961`, with the validation idioms repeated within `draw.rs:913-1230` (page-range ×11, opacity ×7, color-loop ×7). `"opacity must be in 0..1"` appears 14× total.

- [x] Add the `check_*` helpers (module-scope, `pub(crate)`), with unit tests.
- [x] Replace inline checks in `create_document_json` validation block with calls.
- [x] Replace inline checks in `apply_draw_ops_json` validation block with calls.
- [x] `cargo test`; rebuild WASM; `bun test`.

### Task 0b: Collapse the opacity→ExtGState block (Rust)
**Files:** `crates/core/src/draw.rs`

The 9-line `let gs_key = if let Some(o) = opacity { … extgstate_dict(*o) … }` block is copy-pasted **7×** (`draw.rs:1403,1443,1484,` + Rectangle/Ellipse/Text/Path arms).

- [x] Factor into one closure/helper `alloc_gs(o, &mut counter, &mut extgstates, &mut doc) -> String`.
- [x] Replace all 7 sites. `cargo test`.

### Task 0c: Single `color → [r,g,b]` helper (TS)
**Files:** `src/generate/color.ts` (export), consumers `src/generate/page.ts:208`, `src/generate/form-builder.ts:229`, `src/generate/draw-queue.ts:179`

- [x] Export `colorToTuple(c)` from `color.ts`; replace the 3 copies. `bun test`.

### Task 0d: De-duplicate DA / inheritance helpers (Rust)
**Files:** `crates/core/src/fill.rs`, `crates/core/src/forms.rs`

- [x] `fill.rs:579 da_string` is byte-identical to `forms.rs:284` — delete fill's copy, reuse forms'.
- [x] `fill.rs:564 inherited_str` reinvents the `/Parent` walk already in `forms::inherited` (`forms.rs:356`) — make `forms::inherited` `pub(crate)`, add a string accessor, delete fill's copy.
- [x] `cargo test`.

### Task 0e: `image_xobject_dict()` helper (Rust)
**Files:** `crates/core/src/draw.rs` (+ callers)

5 copies of the image-XObject dict (8 of 9 `dict.set` lines shared): `draw.rs:480,555`, `create.rs:376`, `appearance.rs:874`, `embed.rs:121`.

- [x] Extract `image_xobject_dict(width, height, color_space, filter) -> Dictionary`; replace the 5 sites. `cargo test`.

---

## Tier 1 — Hot-path performance (benchmark levers; schedule after Tier 0)

- **P1.** ✅ **Done.** Added Rust `apply_all_json` (new `apply.rs`) that loads once, sequences the fill/flatten/draw/metadata/outline mutation cores on one `IncrementalDocument`, and saves once. Each mutator was split into Phase-A (read immutable `doc`) + `*_apply(&mut inc, …)` cores; existing per-op exports kept as thin wrappers. TS `save()` builds a combined plan and calls `wasm.applyAll` on the common path, falling back to the chained pipeline (`saveChained`) only when structural page ops are queued. Verified: cargo 277 / bun 266 green; micro-bench fill+draw+metadata+outline **3.85× faster** (1.26 ms → 0.33 ms, 4 passes → 1).
- **P2.** ✅ **Done.** `maxWidth` now flows through to Rust as a field on the text op; wrapping happens server-side in the single draw pass. New `appearance::wrap_str` (char-based, mirrors `wrap_lines`) + `wrap_standard14` and `fonts::wrap_embedded` (parses the face once) cover both font kinds with one source of truth. Added wrapping to both `draw.rs` (loaded docs) and `create.rs` (created docs) to avoid a regression. Deleted TS `wrap-text.ts` + its per-word `measureStd` plumbing (removed the dead `measureStd` ctor param from `PdfPage` and its 3 call sites). Fixes the CRLF drift (TS only split `\n`) and eliminates the ~N per-word `wasm.measureText` boundary crossings per wrapped paragraph. Verified: cargo 284 (7 new wrap tests) / bun 261 / tsc green; created+loaded maxWidth covered end-to-end.
- **P3.** Convert `format!(…).as_bytes()` (~57 sites, many in per-glyph/per-line/per-op loops) and `fmt_num` (`draw.rs:194`) to `write!(out, …)` — `Vec<u8>: io::Write`. Perf + WASM size. *Effort low-med.*

## Tier 2 — God-function / god-class splits (structural; after Tier 0 helpers exist)

- **P4.** Split `create_document_json` (`create.rs:527-2034`) → `validate_ops`, `build_pages`, `build_fields_and_acroform` (`:1476-1979`), `build_outline`, `build_info`.
- **P5.** Split `apply_draw_ops_json` (`draw.rs:885-1752`) → `validate_ops`, `embed_fonts`, `emit_page_ops`. Real fix: a `content`/`ops` module owning op structs + validators + emit primitives, shared by create & draw.
- **P6.** Decompose `PdfDocumentBase` (`document.ts`): split create/load seam (`CreatedDocument` vs `LoadedDocument`); extract `MetadataState`, `PageStructure` (incl. `buildPageIndexResolver:396`), `ResourceEmbedder`; `save()` becomes a coordinator.
- **P7.** Split `fill::resolve` (`fill.rs:192-459`, 6 independent branches) — one function per branch. Lowest-risk warm-up.

## Tier 3 — Duplication cleanup (medium effort, independently shippable)

- **P8.** `makeBindings(raw, {guard})` factory to collapse `wasm.ts` ≈ `wasm-browser.ts` (~200 lines, 3-place change surface incl. `CoreWasm` interface `document.ts:24`).
- **P9.** Share `load`/`create` body + export barrel between `index.ts`/`index.browser.ts` — **fixes the existing drift: browser is missing `assemble()`/`merge()`.**
- **P10.** `callJson<T>()` / `callBytes()` helper for the ~16 `toPdfError` + `JSON.parse(wasm…)` boilerplate sites; centralize the `PageInfo` wire type.
- **P11.** `page.ts` draw methods: `validatePoints()` + `strokeStyleToWire()/fillStyleToWire()`; pick ONE layer (page vs `draw-queue.ts`) to own optional-field normalization.
- **P12.** `ChoiceField` base with `selectFrom(valid, label, value, {default})` for the 6 duplicated bodies in `fields.ts` (`:374,397,446,469,520,543`).
- **P13.** Smaller Rust dups: DR/Font lookup chain (`fill.rs:586`+`:640`), widget-collection prologue (`fill.rs:594`+`:630`), appearance content-prologue + `quad_offset` (3× in `appearance.rs:222,266,…`).
- **P14.** Reconcile `FormBuilder` schema literal (6×) with `FieldMeta` (`schema.ts:12`) and typegen (`typegen.ts:80`) — `multiSelect`/`required`/`maxLength` mismatch — via one generic `DeclaredField<…>` type.

## Explicitly NOT problems (do not "fix")

- `IncrementalDocument::create_from(data.to_vec(), …)` clones the input at 6 entry points — required by lopdf's incremental API + the `&[u8]` WASM boundary. Add a comment only.
- Two-pass validate-then-build in both Rust giants — correct (fail before mutating).
- PDF date parsing TS-only (`metadata.ts`) — single implementation today; drift *watch* item only.
- `appearance.rs` ↔ `fill.rs` boundary is clean (content-construction vs incremental mutation).

## Recommended sequence

1. **Tier 0** (0a–0e) — mechanical, unblocks splits.
2. **P1 + P2** — hot-path levers; measure against `bench/bench.ts`.
3. **P3** — `write!` conversion.
4. **Tier 3** (P8–P14) — independently shippable.
5. **Tier 2** (P4–P7) — splits last, now that shared helpers exist; start with P7.
