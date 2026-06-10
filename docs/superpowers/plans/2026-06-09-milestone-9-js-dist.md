# Milestone 9 - JavaScript Distribution Build

**Goal:** Publish normal JavaScript and declaration files instead of using TypeScript source files as the npm runtime entry.

**Scope for this slice:**

- Emit `dist/*.js` and `dist/*.d.ts` from the TypeScript source.
- Keep generated WASM bindings in `pkg/`.
- Update `package.json` `main`, `types`, `exports`, and `files` to use `dist/`.
- Use standard ESM `.js` import specifiers in source so emitted JS runs in Node.
- Verify package root import from the built output.

**Deferred:**

- Browser-specific WASM target.
- Bundled single-file distribution.
- Dual CJS/ESM output.

## Tasks

- [x] Add a build tsconfig that emits JS and declaration files.
- [x] Update source imports to JS extension specifiers.
- [x] Update package manifest scripts and publish files.
- [x] Verify source tests still run.
- [x] Verify built `dist/` works through the package export.
- [x] Run package dry-run.
