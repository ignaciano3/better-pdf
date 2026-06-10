# Milestone 10 - Browser WASM Packaging

**Goal:** Add a browser package entry that uses a browser-compatible WASM build while keeping the existing Node/Bun entry working.

**Scope for this slice:**

- Build a second WASM target with `wasm-pack --target web` into `pkg-web/`.
- Add a browser WASM bridge that async-initializes the web target.
- Add a browser package entry via conditional exports.
- Refactor shared form/document code just enough to reuse the same public classes.
- Verify Node/Bun tests still pass and the browser entry can load/read fields.

**Deferred:**

- Full browser test matrix.
- Bundler-specific examples for Vite/Webpack.
- Deno/edge-specific package entries.

## Tasks

- [x] Add browser WASM build script and package files.
- [x] Add browser WASM bridge and browser package entry.
- [x] Refactor `PdfForm` to avoid hard-wiring the Node WASM bridge.
- [x] Update package manifest/files/README.
- [x] Verify Node source tests and browser entry smoke test.
- [x] Run package dry-run.
