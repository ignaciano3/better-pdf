# better-pdf — Bun example

Creates a one-page PDF with drawn text and writes it to `out.pdf`.

## Install

```sh
bun add @ignaciano3/better-pdf
```

## Run

```sh
bun index.ts
```

### Expected output

```
Bytes written : 825
PDF header    : %PDF-
Starts with %PDF- : true
Saved to out.pdf
```

## Verified: Bun v1.3.14

```
$ bun index.ts
Bytes written : 825
PDF header    : %PDF-
Starts with %PDF- : true
Saved to out.pdf
```

Run on 2026-06-20 using `@ignaciano3/better-pdf@0.21.0` (installed from local tarball; end users `bun add @ignaciano3/better-pdf`).

## How it works

Bun resolves the default export condition and loads `dist/index.js`, which initializes the WASM binary via `readFileSync` — the same Node.js path. No explicit `initializeWasm()` call is needed. TypeScript source (`index.ts`) runs directly without a build step.
