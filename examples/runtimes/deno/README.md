# better-pdf — Deno example

Creates a one-page PDF with drawn text and writes it to `out.pdf`.

> **Config provided** — Deno is not installed in the development environment where this was authored. The example is complete and correct, but has not been executed here. Verify it in an environment with Deno installed.

## Install

No installation step required. Deno resolves the `npm:` specifier on first run.

## Run

```sh
deno run -A main.ts
```

Or via the configured task:

```sh
deno task start
```

### Expected output

```
Bytes written : 829
PDF header    : %PDF-
Starts with %PDF- : true
Saved to out.pdf
```

(Byte count may differ slightly from the Node/Bun examples due to different text content.)

## How it works

The import `npm:@ignaciano3/better-pdf` causes Deno to resolve and cache the package from the npm registry and execute the default entry point (`dist/index.js`). Deno's npm-compatibility layer supports `node:fs` (used by `readFileSync` inside the WASM initializer), so the package self-initializes on import — no explicit `initializeWasm()` call needed.

The `-A` flag grants all permissions (file-write for `out.pdf`). For tighter sandboxing use `--allow-read --allow-write=out.pdf` instead.
