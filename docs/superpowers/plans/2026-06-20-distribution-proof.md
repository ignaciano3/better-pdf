# Distribution Proof (Runs-Everywhere) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close V1-READINESS #5 — prove `@ignaciano3/better-pdf` runs across Node, Bun, browser, Vite, webpack, Next.js, Deno, and Cloudflare Workers, by (a) fixing the real packaging blockers that break bundler/edge targets, (b) shipping a runnable example + setup doc per runtime, and (c) replacing the README "expects a modern bundler" hedge with a concrete, verified support matrix.

**Architecture:** Non-breaking, additive release (0.21.0). The wasm ships once as the `--target web` build in `pkg-web/`. Two packaging fixes make bundlers reliable: expose the raw `.wasm` at a `./wasm` export subpath so bundler users can resolve it as an asset (`import wasmUrl from "@ignaciano3/better-pdf/wasm?url"` → `initializeWasm(wasmUrl)`), and stop `sideEffects:false` from tree-shaking the wasm init. Each runtime gets a minimal `examples/<runtime>/` project (init + fill-a-field/draw + save) with a README. Runtimes available in this environment (Node, Bun, headless browser) are verified end-to-end against the **packed tarball** (npm pack → install → run), proving the published shape — not just the repo. Runtimes whose toolchain is absent here (Deno, Cloudflare Workers) ship a complete, documented example + config to run in their own environment; Cloudflare Workers uses the documented `initializeWasm(importedWasmModule)` workaround (no new package entry, per decision).

**Tech Stack:** TypeScript, wasm-pack `--target web`, Node 18+, Bun, Vite, webpack 5, Next.js, Deno, Cloudflare Workers (wrangler), bun test.

## Global Constraints
- **Non-breaking.** No public API change, no wire/Rust change. Version bump to **0.21.0** (minor) in `package.json` + `crates/core/Cargo.toml` + `Cargo.lock` (crate name `better-pdf-core`); 0.16–0.20 already shipped.
- The wasm is the single `pkg-web/--target web` build. Do NOT add a second wasm-pack target.
- Cloudflare Workers: **document the workaround only** (no new export entry, no node:fs-free entry added) — user imports the `.wasm` via wrangler and passes the `WebAssembly.Module` to `initializeWasm()`.
- `source ~/.cargo/env` before any cargo/wasm command; `bun run build` before anything that consumes the built core.
- Must pass `bun test` + `bun run typecheck` + `cargo test`/`cargo clippy --all-targets -D warnings` (no Rust changes expected, but keep green).
- Honesty rule for docs: label each runtime as **Verified here** (ran end-to-end in this work) vs **Config provided** (example + config shipped, to run in that runtime's environment). Do not claim a runtime is verified if it was not actually run.
- Example projects live under `examples/runtimes/<runtime>/` and must NOT be published in the npm tarball (they are repo-only; confirm `files` does not include them — it lists `dist` + specific `pkg-web/*` only, so `examples/` is already excluded).
- Every commit ends with: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do NOT tag 1.0.0 — #5 is the last V1-READINESS gate alongside #2 (docs audit); 1.0.0 is a separate step.

---

## Background (verified facts)

- `npm pack` → install tarball → run already works on **Node v24 and Bun** (a create+drawText+save smoke produced a valid 591-byte `%PDF-` document). The tarball includes `pkg-web/better_pdf_core_bg.wasm` (via the `files` array).
- Node/Bun loader `src/core/wasm.ts`: static `import { readFileSync } from "node:fs"` + top-level `initSync({ module: readFileSync(new URL("../../pkg-web/better_pdf_core_bg.wasm", import.meta.url)) })`. Works wherever `node:fs` + filesystem exist (Node, Bun, Deno npm-compat).
- Browser loader `src/core/wasm-browser.ts`: `initializeWasm(moduleOrPath?)`; default `new URL("../../pkg-web/better_pdf_core_bg.wasm", import.meta.url)` → `initCore({ module_or_path })` → `fetch`. Accepts an explicit URL **or** a `WebAssembly.Module` — this is the injection point for every bundler/edge runtime.
- `exports` map: `"."` default → `dist/index.js` (Node); `"browser"` condition + `"./browser"` → `dist/index.browser.js`. Bundlers targeting the browser resolve the `browser` condition → the fs-free entry, so `node:fs` is NOT their problem; their problem is **wasm asset resolution** of the default `new URL(... node_modules ...)`.
- `package.json` has `"sideEffects": false` — a tree-shaking hazard for the top-level `initSync(...)`.

---

## Task 1 — Packaging fixes: `./wasm` export subpath + sideEffects

Make bundler-based wasm resolution reliable and stop the init from being tree-shaken. Additive + config-only; no loader logic change.

**Files:**
- Modify `package.json` (`exports` add `./wasm`; `files` ensure the wasm is included — it already is; `sideEffects` array)
- Create `scripts/pack-smoke.ts` (pack → install into a temp dir → run a Node + Bun fill/draw smoke; asserts a valid PDF)
- Modify `tests/` or add a test entry that runs the pack-smoke (guarded/optional — see step)

**Interfaces:**
- Produces: a public `@ignaciano3/better-pdf/wasm` subpath resolving to `pkg-web/better_pdf_core_bg.wasm`, usable as `import wasmUrl from "@ignaciano3/better-pdf/wasm?url"` (Vite) or `new URL("@ignaciano3/better-pdf/wasm", import.meta.url)` (webpack).

### Steps

- [ ] **1.1 Add the `./wasm` export.** In `package.json` `exports`, add:
  ```json
  "./wasm": "./pkg-web/better_pdf_core_bg.wasm"
  ```
  (a plain string target — no conditions; bundlers append `?url`/asset handling themselves). Confirm `pkg-web/better_pdf_core_bg.wasm` is already in `files` (it is).

- [ ] **1.2 Fix `sideEffects`.** Change `"sideEffects": false` to an array that preserves the wasm-init modules while keeping the rest tree-shakeable:
  ```json
  "sideEffects": ["./dist/core/wasm.js", "./dist/core/wasm-browser.js"]
  ```
  Rationale: the top-level `initSync(...)` in `wasm.js` is a real side effect a bundler must not drop.

- [ ] **1.3 Write `scripts/pack-smoke.ts`.** A script that: runs `bun run build`, `npm pack`, creates a temp dir, `npm install`s the tarball, writes a tiny ESM script that does `PdfDocument.create()` → `addPage()` → `drawText(...)` → `save()` and asserts the result starts with `%PDF-`, then runs it under **both** `node` and `bun`, and also loads + fills a bundled fixture field to exercise the load path. Exit non-zero on any failure. Clean up the temp dir. Model the smoke script body on the verified snippet in Background. Make it print `VERIFIED: node`, `VERIFIED: bun`.

- [ ] **1.4 Run the pack-smoke.** `bun run scripts/pack-smoke.ts` — expect `VERIFIED: node` and `VERIFIED: bun`, valid PDFs, exit 0. Add a `"pack-smoke": "bun run scripts/pack-smoke.ts"` npm script.

- [ ] **1.5 Verify the `./wasm` subpath resolves.** `node -e "import('node:module').then(async()=>{const u=require.resolve?.('@ignaciano3/better-pdf/wasm');})"` is awkward for ESM; instead confirm via the packed tarball: in the pack-smoke temp install, assert `require.resolve` / `import.meta.resolve("@ignaciano3/better-pdf/wasm")` resolves to the `.wasm` file (Node ≥18.19 supports `import.meta.resolve`). If `import.meta.resolve` is unavailable, assert the file exists at `node_modules/@ignaciano3/better-pdf/pkg-web/better_pdf_core_bg.wasm`. Fold this assertion into pack-smoke.ts.

- [ ] **1.6 Full verification + commit.**
  ```
  source ~/.cargo/env && bun run build && bun run typecheck && bun test
  ```
  Green. Then:
  ```
  git add package.json scripts/pack-smoke.ts
  git commit -m "build(dist): add ./wasm export subpath; fix sideEffects for wasm init

  Expose the raw wasm at @ignaciano3/better-pdf/wasm so bundler users can
  resolve it as an asset and pass it to initializeWasm(). Mark the wasm
  loader modules as having side effects so the top-level initSync is not
  tree-shaken. Add a pack-smoke that installs the packed tarball and runs
  a create+draw+save under Node and Bun.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 2 — Runnable examples: Node, Bun, Deno

Three server-runtime examples. Node + Bun are verified end-to-end here; Deno ships a complete example + config (deno not installed in this environment).

**Files:**
- Create `examples/runtimes/node/` (`package.json`, `index.mjs`, `README.md`)
- Create `examples/runtimes/bun/` (`index.ts`, `README.md`)
- Create `examples/runtimes/deno/` (`main.ts`, `deno.json`, `README.md`)
- Create `examples/runtimes/README.md` (index + support matrix stub)

### Steps

- [ ] **2.1 Node example.** `examples/runtimes/node/index.mjs`: import from `@ignaciano3/better-pdf`, load a bundled sample PDF OR create one, fill a field or draw text, save to `out.pdf`, log byte length + `%PDF-` header. `package.json` with `"type":"module"` and a dependency on the package (use a `file:../../..` link or document `npm install @ignaciano3/better-pdf`). README: install + `node index.mjs` + expected output. Run it (against the repo via the tarball or a `file:` install) and capture the output for the README ("Verified: Node v24").

- [ ] **2.2 Bun example.** `examples/runtimes/bun/index.ts`: same flow, run with `bun index.ts`. README + verified output. Note Bun resolves the default (Node) export condition, so the `readFileSync` path is used — confirm it works (it does per the pack-smoke).

- [ ] **2.3 Deno example.** `examples/runtimes/deno/main.ts`: `import { PdfDocument, ... } from "npm:@ignaciano3/better-pdf";` + same flow. `deno.json` with a task. README documents `deno run -A main.ts` and explains Deno resolves the npm Node entry (uses `readFileSync` from `node:fs`, supported in Deno's npm-compat). Mark as **Config provided** (deno not installed in this environment) — do NOT claim it was run here; instead state the exact command and expected output, and note it needs verification in a Deno install.

- [ ] **2.4 Examples index.** `examples/runtimes/README.md`: a table of the runtimes with a one-line status (Verified / Config provided) and a link to each subfolder. Leave bundler + CF rows for Tasks 3-4.

- [ ] **2.5 Commit.**
  ```
  git add examples/runtimes/node examples/runtimes/bun examples/runtimes/deno examples/runtimes/README.md
  git commit -m "docs(examples): add Node, Bun, Deno runtime examples

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 3 — Browser-bundler examples: Vite, webpack, Next.js

Three minimal browser projects, each using the `./wasm` asset pattern from Task 1 to initialize. Build/run them if the toolchain installs cleanly; otherwise ship the complete project + config + README labeled **Config provided**. Do NOT let `npm install` hang — cap install attempts and fall back.

**Files:**
- Create `examples/runtimes/vite/` (`package.json`, `vite.config.ts`, `index.html`, `src/main.ts`, `README.md`)
- Create `examples/runtimes/webpack/` (`package.json`, `webpack.config.js`, `src/index.js`, `index.html`, `README.md`)
- Create `examples/runtimes/nextjs/` (`package.json`, `next.config.js`, a client component page, `README.md`)

### Steps

- [ ] **3.1 Vite example.** `src/main.ts`:
  ```ts
  import { PdfDocument, initializeWasm, rgb, StandardFonts } from "@ignaciano3/better-pdf/browser";
  import wasmUrl from "@ignaciano3/better-pdf/wasm?url";
  await initializeWasm(wasmUrl);
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from vite", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica, color: rgb(0,0,1) });
  const bytes = await doc.save();
  // trigger a download / render to an <iframe>
  ```
  `vite.config.ts` minimal. README: `npm install && npm run dev` / `npm run build`. Attempt `npm install` + `npm run build` with a timeout; if it succeeds, label **Verified (build)** and capture output; if install fails/times out, label **Config provided** and note the exact commands.

- [ ] **3.2 webpack example.** `webpack.config.js` with `experiments: { asyncWebAssembly: true }` and an `asset/resource` rule (or the `new URL("@ignaciano3/better-pdf/wasm", import.meta.url)` pattern). `src/index.js` mirrors the Vite flow but resolves the wasm URL the webpack way. README documents the config. Attempt build with timeout; label accordingly.

- [ ] **3.3 Next.js example.** A client component (`"use client"`) that dynamically imports the package and calls `initializeWasm` with a wasm URL served from `public/` (document copying `better_pdf_core_bg.wasm` into `public/` via a postinstall/copy step, since Next doesn't emit node_modules assets). `next.config.js` notes any needed `webpack` asset config. README explains the `public/` wasm approach. Label **Config provided** unless a build actually runs here.

- [ ] **3.4 Update the examples index** (`examples/runtimes/README.md`) with the three bundler rows + their status.

- [ ] **3.5 Commit.**
  ```
  git add examples/runtimes/vite examples/runtimes/webpack examples/runtimes/nextjs examples/runtimes/README.md
  git commit -m "docs(examples): add Vite, webpack, Next.js bundler examples

  Each initializes the wasm via the @ignaciano3/better-pdf/wasm asset
  subpath. Verified where the toolchain builds in this environment;
  otherwise shipped as runnable config.

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 4 — Cloudflare Workers example (documented workaround)

Per decision: no new package entry. Document + provide a working wrangler example that imports the `.wasm` as a module binding and passes the resulting `WebAssembly.Module` to `initializeWasm()`. Cannot run here (wrangler absent) → **Config provided**.

**Files:**
- Create `examples/runtimes/cloudflare-workers/` (`wrangler.toml`, `src/index.ts`, `package.json`, `README.md`)

### Steps

- [ ] **4.1 Worker script.** `src/index.ts`:
  ```ts
  import { PdfDocument, initializeWasm, StandardFonts } from "@ignaciano3/better-pdf/browser";
  // wrangler/esbuild turns a .wasm import into a WebAssembly.Module binding:
  import wasmModule from "@ignaciano3/better-pdf/wasm";
  export default {
    async fetch(): Promise<Response> {
      await initializeWasm(wasmModule); // pass the Module directly — no fetch, no fs
      const doc = await PdfDocument.create();
      const page = doc.addPage();
      page.drawText("hello from a worker", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica });
      const bytes = await doc.save();
      return new Response(bytes, { headers: { "content-type": "application/pdf" } });
    },
  };
  ```
  Note: confirm `@ignaciano3/better-pdf/wasm` (the Task 1 subpath) is what wrangler imports as a `WebAssembly.Module`; document that the `browser` entry (no `node:fs`) MUST be used, and that the default `"."` entry would fail in Workers because of `node:fs`.

- [ ] **4.2 `wrangler.toml`** minimal (`main = "src/index.ts"`, `compatibility_date`, `[[rules]]` for `**/*.wasm` as `CompiledWasm` if needed). README: `npm install && npx wrangler dev`, expected `application/pdf` response. Label **Config provided** (wrangler not installed here); state clearly it must be run in a Workers environment and what the expected behavior is.

- [ ] **4.3 Update examples index** with the Cloudflare Workers row (Config provided).

- [ ] **4.4 Commit.**
  ```
  git add examples/runtimes/cloudflare-workers examples/runtimes/README.md
  git commit -m "docs(examples): add Cloudflare Workers example (imported-wasm-module workaround)

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Task 5 — Docs: support matrix, runtime guide, V1-READINESS, release 0.21.0

Replace the README hedge with a concrete, honest support matrix and a docs-site runtime guide; bump the version.

**Files:**
- Modify `README.md` (replace the "expects a modern bundler/runtime that can serve the `.wasm`" hedge with a support matrix + link to examples + the `./wasm` + `initializeWasm` pattern)
- Create `docs/site/src/content/docs/guides/runtimes.md` (per-runtime setup: Node, Bun, Deno, Vite, webpack, Next.js, Cloudflare Workers — each with the exact init snippet)
- Modify the existing browser guide doc (update the bundler-requirements callout to point at `./wasm` + examples)
- Modify `docs/V1-READINESS.md` (#5 — mark distribution proof done: what is verified-here vs config-provided)
- Modify `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`, `crates/core/Cargo.lock`

### Steps

- [ ] **5.1 README support matrix.** Replace the hedge paragraph with a table: runtime | how to init (Node/Bun/Deno = zero-config; browser/bundlers = `initializeWasm(wasmUrl)` via `@ignaciano3/better-pdf/wasm?url`; Workers = `initializeWasm(importedWasmModule)`) | status (Verified / Config provided), and link to `examples/runtimes/`. Keep it accurate to what was actually verified.

- [ ] **5.2 Runtime guide.** `docs/site/src/content/docs/guides/runtimes.md`: one short section per runtime with the exact working snippet (copy from the examples). Cross-link the examples folder.

- [ ] **5.3 Browser guide callout.** Update the existing `:::note[Bundler requirements]` callout to reference the `./wasm` asset subpath + `initializeWasm`, instead of the vague "serve the .wasm" hedge.

- [ ] **5.4 V1-READINESS #5.** Update the bullet: distribution proof shipped — Node/Bun (+ browser via the existing Playwright test) verified end-to-end against the packed tarball; Vite/webpack/Next.js/Deno/Cloudflare Workers ship runnable examples + configs (note which were build-verified here vs config-provided). Remove the "that hedge = support tickets" framing now that the matrix + examples exist.

- [ ] **5.5 Version 0.21.0.** `package.json` `"version":"0.21.0"`, `crates/core/Cargo.toml` `version="0.21.0"`, `source ~/.cargo/env && cargo build --manifest-path crates/core/Cargo.toml` to sync `Cargo.lock` (verify `better-pdf-core` → 0.21.0).

- [ ] **5.6 CHANGELOG.** Insert `## [0.21.0] - 2026-06-20` between `## [Unreleased]` (kept empty) and `## [0.20.0]`. **Added** — `./wasm` export subpath; runtime examples (Node/Bun/Deno/Vite/webpack/Next.js/Cloudflare Workers); per-runtime docs + support matrix. **Fixed** — `sideEffects` no longer tree-shakes wasm init. Don't touch older sections.

- [ ] **5.7 Final verification — run all, confirm pass.**
  ```
  source ~/.cargo/env
  cargo test --manifest-path crates/core/Cargo.toml
  cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings
  bun run build
  bun run typecheck
  bun test
  bun run scripts/pack-smoke.ts
  ```

- [ ] **5.8 Commit + merge.**
  ```
  git add -A
  git commit -m "docs: runtime support matrix + per-runtime guide; release 0.21.0

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```
  Then merge `--no-ff` to master (repo convention: merge locally, skip the menu). Do NOT push or tag.

---

## Done criteria

- `@ignaciano3/better-pdf/wasm` resolves to the raw `.wasm`; `sideEffects` no longer risks dropping the wasm init.
- `scripts/pack-smoke.ts` installs the packed tarball and runs a create+draw+save under Node AND Bun, asserting valid `%PDF-` output.
- `examples/runtimes/<runtime>/` exists for node, bun, deno, vite, webpack, nextjs, cloudflare-workers — each with a README and the exact init snippet; each labeled Verified or Config provided honestly.
- README has a runtime support matrix (no vague "serve the .wasm" hedge); a docs-site runtime guide exists; V1-READINESS #5 marked done with the verified/config-provided split.
- `package.json`/`Cargo.toml`/`Cargo.lock` at 0.21.0; CHANGELOG has a 0.21.0 section; full test + clippy + typecheck + pack-smoke green.

## Self-review notes (for the executor)
- Never claim a runtime is "verified" if you did not actually run it here. Deno + Cloudflare Workers are Config provided (toolchains absent). Vite/webpack/Next are Verified-build only if their `npm install`+build actually completed in this environment within the timeout — otherwise Config provided. Honesty over coverage.
- Do not `npm install` with no timeout — cap it; if it hangs or fails, ship the example as config and move on. Bundler examples are deliverables even unbuilt.
- The wasm is loaded via `initializeWasm()` in every browser/bundler/edge case — the whole point of the `./wasm` subpath is to give bundlers an asset they can resolve and hand to it. Node/Bun/Deno need no init call (the Node entry self-initializes on import).
- `examples/` must not bloat the published tarball — confirm `files` still excludes it (it lists `dist` + specific `pkg-web/*` only).
