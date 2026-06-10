# Milestone 16 — v1 Release Hygiene

**Status:** ✅ Implemented and merged.

**Goal:** Close the credibility gaps that block a professional first publish —
metadata, CI, a changelog — without changing library behavior.

## What shipped

- **`package.json`** — real `version` `0.1.0`, plus `author`, `repository`,
  `homepage`, `bugs`, `engines` (`node >= 18`), and
  `publishConfig` (`{ access: "public", provenance: true }`).
- **`crates/core/Cargo.toml`** — `version` `0.1.0`, `description`, `repository`
  (silences the wasm-pack metadata warning).
- **`CHANGELOG.md`** — Keep a Changelog format, shipped in the npm `files` list.
- **CI** — `.github/workflows/ci.yml` runs, on push/PR: Rust `cargo test` +
  `clippy -D warnings`, the WASM build, `bun run typecheck`, `bun test`, and the
  JS dist build.
- **README** — reworded "pre-alpha" → "0.1.x, pre-1.0"; documented limitations.

## Decision

- Version `0.1.0` (0.x = the public API may still change before 1.0), not `1.0.0`.

## Verification

- `bun test`, `bun run typecheck`, `cargo build` at the new version, and
  `npm pack --dry-run` to confirm the tarball contents.
