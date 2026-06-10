# Milestone 8 - Package Readiness

**Goal:** Bring the npm package surface closer to a publishable v1 shape now that the core fill, flatten, and visual-signature features exist.

**Scope for this slice:**

- Update `package.json` for a tree-shakeable public package surface.
- Add missing package metadata that matters for npm consumers.
- Refresh `README.md` from Milestone 1 status to current supported API.
- Keep local `.env` and manual signature assets out of Git.
- Verify TypeScript, tests, and package contents with a dry-run pack.

**Deferred:**

- Browser-specific WASM init/package target.
- Bundled JS transpilation to `dist/`.
- Generated form-type tooling.
- Benchmarks versus `pdf-lib`.

## Tasks

- [x] Update package manifest exports/metadata.
- [x] Add a license file for the declared MIT license.
- [x] Refresh README usage and feature docs.
- [x] Ignore local environment files and ad hoc signature assets.
- [x] Run type-check, tests, and package dry-run.
