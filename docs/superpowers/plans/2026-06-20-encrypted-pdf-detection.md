# Encrypted-PDF Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect encrypted PDFs the moment their bytes are parsed and fail fast with a typed, catchable `EncryptedPdfError` instead of a confusing downstream failure.

**Architecture:** Introduce one shared Rust helper that wraps `Document::load_mem`, checks the parsed trailer for an `/Encrypt` entry, and returns a stable, machine-detectable error string (`ENCRYPTED: ...`) when present. Route every production `Document::load_mem` call site through this helper so detection happens in exactly one place. On the TypeScript side, add `EncryptedPdfError extends PdfError`, map the `ENCRYPTED:` prefix to it inside the single boundary wrapper `toPdfError` (the function referenced at `src/core/errors.ts:96`), and export the new class from both public barrels.

**Tech Stack:** Rust (lopdf), wasm-bindgen, TS error layer, bun test.

## Global Constraints
- Detection lives in ONE shared Rust helper (`load_pdf`) used by every production load path; no per-feature duplication.
- The Rust error string uses the stable prefix `ENCRYPTED:` exactly: `"ENCRYPTED: this PDF is encrypted; encrypted PDFs are not supported"`.
- A new `EncryptedPdfError` is added to `src/core/errors.ts` and exported from both `src/index.ts` and `src/index.browser.ts`.
- Build the wasm package (`bun run build`, which builds `pkg-web`) before running any TS test; the TS layer only sees encryption through the wasm core.
- Always `source ~/.cargo/env` before any `cargo`/wasm build in this environment.
- Bump the version to `0.16.0` (this cycle has not been bumped yet — current is `0.15.0` in both `package.json` and `crates/core/Cargo.toml`) and keep the two in sync.
- Update `README.md`, `docs/site/src/content/docs/reference/limitations.md`, and `CHANGELOG.md` to note that encrypted PDFs now fail with a typed `EncryptedPdfError`.

---

## Background (read before starting)

Current load layout (verified):

- Every production parse uses the exact idiom `Document::load_mem(data).map_err(|e| e.to_string())?` (or `.map_err(|e| e.to_string())?` on a slice / `src_bytes`). Production call sites:
  - `crates/core/src/pages.rs:48`
  - `crates/core/src/outline.rs:131`
  - `crates/core/src/metadata.rs:84`
  - `crates/core/src/metadata.rs:117`
  - `crates/core/src/embed.rs:70` (variable is `src_bytes`)
  - `crates/core/src/pageops.rs:263` (slice `&docs_blob[d.offset..end]`)
  - `crates/core/src/fill.rs:21`
  - `crates/core/src/forms.rs:36`
  - `crates/core/src/pagetree.rs:45`
  - `crates/core/src/flatten.rs:27`
  - `crates/core/src/draw.rs:787`
  - All other `Document::load_mem` hits are inside `#[cfg(test)]` modules and must NOT be changed.
- There is no `create::` load path (it builds a fresh `Document`), so `create.rs` needs no change.
- The TS `PdfDocument.load` does NOT call into wasm; it just stores bytes. Encryption first reaches the core on the first read (`getForm()` → `readFields`, `getPage()`/page ops → `readPages`, `getMetadata()` → `readMetadata`) or on `save()`. Every one of those TS call sites already wraps thrown errors with `toPdfError(e)` (see `src/core/document.ts` and `src/index.ts`). Therefore mapping the `ENCRYPTED:` prefix inside `toPdfError` is the single chokepoint that covers all entry points.
- TS error pattern to match: subclasses extend `PdfError`; `PdfError` sets `this.name = new.target.name`. The barrel exports list errors explicitly in both `src/index.ts` and `src/index.browser.ts`.

---

## Task 1 — Rust shared detection helper + wire into all load paths

**Files:**
- `crates/core/src/lib.rs` (add a small `doc_io` module and declare it)
- `crates/core/src/doc_io.rs` (new file: the shared helper + its unit test)
- `crates/core/src/pages.rs`
- `crates/core/src/outline.rs`
- `crates/core/src/metadata.rs`
- `crates/core/src/embed.rs`
- `crates/core/src/pageops.rs`
- `crates/core/src/fill.rs`
- `crates/core/src/forms.rs`
- `crates/core/src/pagetree.rs`
- `crates/core/src/flatten.rs`
- `crates/core/src/draw.rs`

**Interfaces (exact):**
```rust
// crates/core/src/doc_io.rs
use lopdf::Document;

/// Stable, machine-detectable prefix the TS boundary maps to `EncryptedPdfError`.
pub const ENCRYPTED_PREFIX: &str = "ENCRYPTED:";

/// Parse PDF bytes into a `Document`, failing fast on encrypted files.
///
/// Encryption is not supported. If the parsed trailer carries an `/Encrypt`
/// entry, this returns an `Err` whose message starts with [`ENCRYPTED_PREFIX`]
/// so the TS layer can raise a typed `EncryptedPdfError`.
pub fn load_pdf(data: &[u8]) -> Result<Document, String> {
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    if doc.trailer.has(b"Encrypt") {
        return Err(format!(
            "{ENCRYPTED_PREFIX} this PDF is encrypted; encrypted PDFs are not supported"
        ));
    }
    Ok(doc)
}
```

Call sites change from:
```rust
let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
```
to:
```rust
let doc = crate::doc_io::load_pdf(data)?;
```
(Adjust the argument per site: `src_bytes` in `embed.rs`, `&docs_blob[d.offset..end]` in `pageops.rs`. The helper already returns `Result<_, String>`, so the trailing `.map_err(|e| e.to_string())?` is dropped at every site.)

### Steps

- [ ] **1.1 Write the failing Rust unit test (real code).** Create `crates/core/src/doc_io.rs` containing ONLY the test module first, so the build fails on the missing `load_pdf`/`ENCRYPTED_PREFIX` symbols:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use lopdf::{Dictionary, Document, Object};

      /// Build a minimal valid PDF whose trailer references an `/Encrypt` dict,
      /// without performing real encryption (detection only checks the key).
      fn encrypted_pdf_bytes() -> Vec<u8> {
          let mut doc = Document::with_version("1.5");
          let pages_id = doc.new_object_id();
          let page_id = doc.add_object(lopdf::dictionary! {
              "Type" => "Page",
              "Parent" => pages_id,
              "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
          });
          doc.objects.insert(
              pages_id,
              Object::Dictionary(lopdf::dictionary! {
                  "Type" => "Pages",
                  "Kids" => vec![page_id.into()],
                  "Count" => 1,
              }),
          );
          let catalog_id = doc.add_object(lopdf::dictionary! {
              "Type" => "Catalog",
              "Pages" => pages_id,
          });
          // A dummy /Encrypt dictionary referenced from the trailer.
          let mut enc = Dictionary::new();
          enc.set("Filter", Object::Name(b"Standard".to_vec()));
          enc.set("V", 1);
          enc.set("R", 2);
          let enc_id = doc.add_object(Object::Dictionary(enc));
          doc.trailer.set("Root", catalog_id);
          doc.trailer.set("Encrypt", Object::Reference(enc_id));
          let mut out = Vec::new();
          doc.save_to(&mut out).unwrap();
          out
      }

      fn plain_pdf_bytes() -> Vec<u8> {
          let mut doc = Document::with_version("1.5");
          let pages_id = doc.new_object_id();
          let page_id = doc.add_object(lopdf::dictionary! {
              "Type" => "Page",
              "Parent" => pages_id,
              "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
          });
          doc.objects.insert(
              pages_id,
              Object::Dictionary(lopdf::dictionary! {
                  "Type" => "Pages",
                  "Kids" => vec![page_id.into()],
                  "Count" => 1,
              }),
          );
          let catalog_id = doc.add_object(lopdf::dictionary! {
              "Type" => "Catalog",
              "Pages" => pages_id,
          });
          doc.trailer.set("Root", catalog_id);
          let mut out = Vec::new();
          doc.save_to(&mut out).unwrap();
          out
      }

      #[test]
      fn load_pdf_rejects_encrypted_trailer() {
          let bytes = encrypted_pdf_bytes();
          let err = load_pdf(&bytes).expect_err("encrypted PDF must be rejected");
          assert!(
              err.starts_with(ENCRYPTED_PREFIX),
              "error must start with ENCRYPTED prefix, got: {err}"
          );
      }

      #[test]
      fn load_pdf_accepts_plain_pdf() {
          let bytes = plain_pdf_bytes();
          assert!(load_pdf(&bytes).is_ok(), "plain PDF must load");
      }
  }
  ```
  NOTE on the fixture: this uses **fixture approach 1** (generate in Rust at test time via `lopdf`). No committed binary is needed because detection only inspects the `/Encrypt` trailer key. Verify the `dictionary!` macro is in scope — it is exported by `lopdf` and already used across the crate; if a site needs it qualified, write `lopdf::dictionary!`.

- [ ] **1.2 Declare the module so the test compiles into the crate.** In `crates/core/src/lib.rs`, add `mod doc_io;` alongside the other `mod` declarations (e.g. right after `mod draw;`). Keep it private (only `crate::doc_io::*` is needed internally).

- [ ] **1.3 Run and expect FAIL.** `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml doc_io`. Expect a compile error: cannot find function `load_pdf` / cannot find value `ENCRYPTED_PREFIX` in module `doc_io`.

- [ ] **1.4 Implement the helper (real code).** Prepend the `load_pdf` function, `ENCRYPTED_PREFIX` const, and the `use lopdf::Document;` import (exactly as in the Interfaces block above) to the top of `crates/core/src/doc_io.rs`, before the `#[cfg(test)]` module.

- [ ] **1.5 Run and expect PASS.** `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml doc_io`. Both `load_pdf_rejects_encrypted_trailer` and `load_pdf_accepts_plain_pdf` pass.

- [ ] **1.6 Wire every production load path through the helper.** Replace each of the following lines with `let <binding> = crate::doc_io::load_pdf(<arg>)?;`, keeping the original binding name and mutability:
  - `crates/core/src/pages.rs:48` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/outline.rs:131` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/metadata.rs:84` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/metadata.rs:117` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/embed.rs:70` → `let src = crate::doc_io::load_pdf(src_bytes)?;`
  - `crates/core/src/pageops.rs:263` → `let mut doc = crate::doc_io::load_pdf(&docs_blob[d.offset..end])?;`
  - `crates/core/src/fill.rs:21` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/forms.rs:36` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/pagetree.rs:45` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/flatten.rs:27` → `let doc = crate::doc_io::load_pdf(data)?;`
  - `crates/core/src/draw.rs:787` → `let doc = crate::doc_io::load_pdf(data)?;`
  Do NOT touch any `Document::load_mem` inside `#[cfg(test)]` modules. After editing, confirm no production `load_mem` remains: `grep -rn "Document::load_mem" crates/core/src` should show only test-module lines.

- [ ] **1.7 Run the full Rust suite + clippy and expect PASS.**
  `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml` then
  `source ~/.cargo/env && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`.
  Both must be clean. (If clippy flags the `format!` with inlined args, keep the inlined-args form it prefers.)

- [ ] **1.8 Commit.** `git add -A && git commit` with message: `feat(core): detect encrypted PDFs at load and fail fast` (Co-Authored-By trailer per repo convention).

---

## Task 2 — TS `EncryptedPdfError` + boundary mapping + export + TS test

**Files:**
- `src/core/errors.ts` (add the class; map the prefix inside `toPdfError`)
- `src/index.ts` (export the class)
- `src/index.browser.ts` (export the class)
- `tests/errors.test.ts` (new test)
- `scripts/make-fixtures.ts` (only if a committed fixture is chosen — see step 2.1)

**Interfaces (exact):**
```ts
// src/core/errors.ts — new subclass (place near PdfCoreError, matching style)
/**
 * Thrown when loading or operating on an encrypted PDF. Encryption is not
 * supported; the document must be decrypted before use.
 */
export class EncryptedPdfError extends PdfError {
  constructor(
    message = "this PDF is encrypted; encrypted PDFs are not supported",
  ) {
    super(message);
  }
}
```

```ts
// src/core/errors.ts — mapping point (the function referenced at line 96)
export function toPdfError(e: unknown): PdfError {
  if (e instanceof PdfError) return e;
  const message = e instanceof Error ? e.message : String(e);
  if (message.includes("ENCRYPTED:")) return new EncryptedPdfError();
  return new PdfCoreError(message);
}
```
(Use `.includes("ENCRYPTED:")` rather than `.startsWith` because wasm-bindgen / `JsError` may prefix the core string with `Error: ` or similar when it crosses the boundary.)

### Steps

- [ ] **2.1 Choose and create the TS-level fixture.** The TS test needs encrypted bytes on disk to load through the public API. Use **fixture approach 2** for this test only: add a generator to `scripts/make-fixtures.ts` that writes `tests/fixtures/generated/encrypted-min.pdf`, and run it once to produce the committed file.
  - Read the existing `scripts/make-fixtures.ts` to match its style (how it constructs and writes the other `tests/fixtures/generated/*.pdf` files).
  - The generator must produce a minimal PDF whose trailer contains `/Encrypt` (no real encryption needed). If `make-fixtures.ts` already uses the wasm core or a JS PDF lib, prefer hand-writing the few bytes of a classic-xref PDF with an `/Encrypt N 0 R` entry in the trailer and a dummy `N 0 obj << /Filter /Standard /V 1 /R 2 >> endobj` object; otherwise generate it from the Rust side (e.g. a tiny `cargo run`-style helper) and copy the bytes. Document in the script comment that the file exists solely to exercise `EncryptedPdfError` and is NOT genuinely encrypted.
  - Run the generator (e.g. `bun run scripts/make-fixtures.ts`, or whatever invocation the script documents) and confirm `tests/fixtures/generated/encrypted-min.pdf` exists.
  - Sanity-check the fixture independently before relying on it: a temporary one-off `grep -a "/Encrypt" tests/fixtures/generated/encrypted-min.pdf` must match.

- [ ] **2.2 Write the failing TS test (real code).** Append to `tests/errors.test.ts`:
  ```ts
  import { EncryptedPdfError } from "../src/index.ts";

  test("loading an encrypted PDF throws EncryptedPdfError (a PdfError)", async () => {
    const bytes = new Uint8Array(
      readFileSync(
        join(import.meta.dir, "fixtures/generated/encrypted-min.pdf"),
      ),
    );
    const doc = await PdfDocument.load(bytes);
    // Encryption surfaces on the first read into the core.
    let err: unknown;
    try {
      doc.getForm();
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(EncryptedPdfError);
    expect(err).toBeInstanceOf(PdfError);
    expect((err as Error).name).toBe("EncryptedPdfError");
  });
  ```
  Add `EncryptedPdfError` to the existing import from `../src/index.ts` at the top of the file (the import already pulls `PdfDocument`, `PdfError`, etc.). NOTE: `getForm()` is synchronous and reads via `readFields`, so a `try/catch` is correct here; do not use `.rejects`. If `getForm()` turns out to be lazy in a way that defers the core read, fall back to `doc.getPage(0)` or `await doc.save()` (with `.rejects.toBeInstanceOf(EncryptedPdfError)`) — pick whichever first call actually reaches the core, and verify by observing the failing-then-passing transition.

- [ ] **2.3 Build wasm, run, expect FAIL.** `bun run build` (rebuilds the wasm package the TS layer imports) then `bun test tests/errors.test.ts`. Expect failure: `EncryptedPdfError` is not exported (import error) or, once exported but unmapped, the error is a `PdfCoreError` rather than `EncryptedPdfError`.

- [ ] **2.4 Add the class (real code).** Insert the `EncryptedPdfError` class into `src/core/errors.ts` immediately after `PdfCoreError` (line 72), exactly as in the Interfaces block.

- [ ] **2.5 Map the prefix in the boundary (real code).** Replace the body of `toPdfError` in `src/core/errors.ts` (lines ~97–100) with the mapped version from the Interfaces block (extract `message` once, check `.includes("ENCRYPTED:")`, else `PdfCoreError`).

- [ ] **2.6 Export from both barrels.** Add `EncryptedPdfError` to the error export block in `src/index.ts` (the `export { PdfError, ... InvalidRotationError } from "./core/errors.js";` list) and to the identical block in `src/index.browser.ts`.

- [ ] **2.7 Rebuild and run, expect PASS.** `bun run build` then `bun test tests/errors.test.ts`. The new test passes; re-run `bun test` for the full suite to confirm no regressions.

- [ ] **2.8 Commit.** `git add -A && git commit` with message: `feat(errors): add EncryptedPdfError and map ENCRYPTED core prefix` (Co-Authored-By trailer).

---

## Task 3 — Docs: README + limitations + CHANGELOG + version bump

**Files:**
- `README.md`
- `docs/site/src/content/docs/reference/limitations.md`
- `CHANGELOG.md`
- `package.json`
- `crates/core/Cargo.toml`

### Steps

- [ ] **3.1 README — error list.** In `README.md`, add a bullet to the error-types list (after the `InvalidImageError` bullet at line ~601):
  ```
  - `EncryptedPdfError` — the PDF is encrypted; encrypted PDFs are not supported, so loading/reading fails fast with this typed error instead of a confusing downstream failure.
  ```

- [ ] **3.2 README — limitations.** In `README.md`, replace the `- No encrypted PDF support.` bullet (line ~738) with:
  ```
  - No encrypted PDF support — encrypted PDFs are detected on load and rejected with a typed `EncryptedPdfError` (decrypt the file first).
  ```

- [ ] **3.3 Docs site — limitations.** In `docs/site/src/content/docs/reference/limitations.md`, replace the `- No encrypted PDF support.` bullet with:
  ```
  - No encrypted PDF support — encrypted PDFs are detected on load (an `/Encrypt` trailer entry) and rejected with a typed `EncryptedPdfError`, so they fail fast with a clear, catchable error rather than breaking somewhere downstream.
  ```

- [ ] **3.4 Version bump.** Set `"version": "0.16.0"` in `package.json` and `version = "0.16.0"` in `crates/core/Cargo.toml`. Verify they match: `grep '"version"' package.json && grep '^version' crates/core/Cargo.toml`.

- [ ] **3.5 CHANGELOG.** In `CHANGELOG.md`, replace the empty `## [Unreleased]` section with a `## [0.16.0] - 2026-06-20` section (keep an empty `## [Unreleased]` above it, matching the existing house style where released sections sit below Unreleased):
  ```
  ## [Unreleased]

  ## [0.16.0] - 2026-06-20

  ### Added

  - Encrypted PDFs are now detected on load (an `/Encrypt` trailer entry) and rejected with a new typed `EncryptedPdfError` (a `PdfError`), exported from both the Node and browser entry points. Encryption remains unsupported; this turns a confusing downstream failure into an explicit, catchable error.
  ```

- [ ] **3.6 Final full verification.** Run, in order, and confirm all pass:
  - `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml`
  - `source ~/.cargo/env && cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`
  - `bun run build`
  - `bun test`

- [ ] **3.7 Commit.** `git add -A && git commit` with message: `docs: note typed EncryptedPdfError; release 0.16.0` (Co-Authored-By trailer).

---

## Done criteria

- A production-path `Document::load_mem` no longer exists outside `#[cfg(test)]` (all route through `crate::doc_io::load_pdf`).
- Loading an encrypted PDF (real or `/Encrypt`-trailer fixture) and touching the core throws `EncryptedPdfError` from the public API.
- Rust tests + clippy (`-D warnings`) and the full `bun test` suite are green after `bun run build`.
- README, docs-site limitations, and CHANGELOG describe the typed error; `package.json` and `Cargo.toml` are both at `0.16.0`.

## Per-skill note: finishing the branch

After Task 3 passes, follow the repo convention (memory: "Always merge to master") — merge the finished branch locally rather than opening the merge/PR options menu.
