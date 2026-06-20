---
title: Runtime setup
description: Per-runtime init snippets for better-pdf — Node, Bun, Deno, Vite, webpack, Next.js, and Cloudflare Workers.
---

`better-pdf` ships a single `.wasm` binary. Node, Bun, and Deno load it
automatically; browser runtimes and edge workers need one extra call to tell the
runtime where the binary lives. This page gives the exact snippet per target.

Runnable, self-contained examples live in
[`examples/runtimes/`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/).

---

## Node.js — **Verified** (v24.16.0)

Zero-config. The package default entry reads the `.wasm` file at import time via
`node:fs`. No `initializeWasm()` call is needed.

```js
// examples/runtimes/node/index.mjs
import { writeFileSync } from "node:fs";
import { PdfDocument, rgb, StandardFonts } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage();
page.drawText("Hello from Node.js!", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica, color: rgb(0.1, 0.3, 0.9) });
const bytes = await doc.save();
writeFileSync("out.pdf", bytes);
```

Install: `npm install @ignaciano3/better-pdf`

---

## Bun — **Verified** (v1.3.14)

Zero-config. Same default entry, same self-init.

```ts
// examples/runtimes/bun/index.ts
import { PdfDocument, rgb, StandardFonts } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage();
page.drawText("Hello from Bun!", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica, color: rgb(0.1, 0.7, 0.3) });
const bytes = await doc.save();
await Bun.write("out.pdf", bytes);
```

Install: `bun add @ignaciano3/better-pdf`

---

## Deno — Config provided

Use the `npm:` specifier. Deno resolves it through the npm compatibility layer,
which includes the Node entry that self-initializes the wasm.

```ts
// examples/runtimes/deno/main.ts
import { PdfDocument, rgb, StandardFonts } from "npm:@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage();
page.drawText("Hello from Deno!", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica, color: rgb(0.6, 0.1, 0.8) });
const bytes = await doc.save();
await Deno.writeFile("out.pdf", bytes);
```

`deno.json`:
```json
{
  "tasks": { "start": "deno run -A main.ts" },
  "nodeModulesDir": "auto"
}
```

Run: `deno task start`

---

## Vite — **Verified** (v5.4.21 build)

Import the `/browser` entry and use Vite's `?url` suffix to resolve the wasm
asset URL. Call `initializeWasm(wasmUrl)` before first use.

```ts
// src/main.ts
import { PdfDocument, initializeWasm, rgb, StandardFonts } from "@ignaciano3/better-pdf/browser";
import wasmUrl from "@ignaciano3/better-pdf/wasm?url";

let initialized = false;

async function generatePdf(): Promise<Uint8Array> {
  if (!initialized) {
    await initializeWasm(wasmUrl);
    initialized = true;
  }
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from vite", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica, color: rgb(0, 0, 1) });
  return doc.save();
}
```

`vite.config.ts` requires no special wasm plugin — the `./wasm` subpath resolves
to the raw `.wasm` file, and `?url` handles the rest.

See [`examples/runtimes/vite/`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/vite/).

---

## webpack 5 — **Verified** (v5.107.2 build)

Use `new URL(specifier, import.meta.url)` — webpack 5 emits the wasm asset and
resolves it at runtime without any extra loader.

```js
// src/index.js
import { PdfDocument, initializeWasm, rgb, StandardFonts } from "@ignaciano3/better-pdf/browser";

const wasmUrl = new URL("@ignaciano3/better-pdf/wasm", import.meta.url);

let initialized = false;

async function generatePdf() {
  if (!initialized) {
    await initializeWasm(wasmUrl.href);
    initialized = true;
  }
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from webpack", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica, color: rgb(0, 0, 1) });
  return doc.save();
}
```

`webpack.config.js` needs `experiments: { asyncWebAssembly: true }` and
`type: "asset/resource"` for the `.wasm` rule. See
[`examples/runtimes/webpack/`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/webpack/).

---

## Next.js — **Verified** (v15.5.19 build)

Dynamic-import the `/browser` entry to avoid SSR issues. Copy the wasm file to
`public/` so Next.js serves it from the root.

```tsx
// app/page.tsx (client component)
"use client";

async function loadAndGenerate(): Promise<Uint8Array> {
  const { PdfDocument, initializeWasm, rgb, StandardFonts } = await import(
    "@ignaciano3/better-pdf/browser"
  );
  // The .wasm is copied into public/ — Next.js serves public/ at root.
  await initializeWasm("/better_pdf_core_bg.wasm");
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from Next.js", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica, color: rgb(0, 0, 1) });
  return doc.save();
}
```

Copy the wasm (run once after install):

```sh
cp node_modules/@ignaciano3/better-pdf/pkg-web/better_pdf_core_bg.wasm public/
```

Or automate via a `postinstall` script. See
[`examples/runtimes/nextjs/`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/nextjs/).

---

## Cloudflare Workers — Config provided

Two constraints drive the setup:

1. The default entry uses `node:fs` to self-init; Workers have no `node:fs`. Use
   the `/browser` entry instead.
2. Workers cannot fetch arbitrary binaries at runtime. Import the `.wasm` as a
   module — wrangler/esbuild compiles it into a `WebAssembly.Module` binding at
   bundle time.

```ts
// src/index.ts
import { PdfDocument, initializeWasm, StandardFonts } from "@ignaciano3/better-pdf/browser";
import wasmModule from "@ignaciano3/better-pdf/wasm";

export default {
  async fetch(): Promise<Response> {
    await initializeWasm(wasmModule);
    const doc = await PdfDocument.create();
    const page = doc.addPage();
    page.drawText("hello from a worker", { x: 50, y: 700, size: 24, font: StandardFonts.Helvetica });
    const bytes = await doc.save();
    return new Response(bytes, { headers: { "content-type": "application/pdf" } });
  },
};
```

`wrangler.toml` must declare the wasm module binding. See
[`examples/runtimes/cloudflare-workers/`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/cloudflare-workers/).

---

## Summary

| Runtime | Entry | Init |
| --- | --- | --- |
| Node.js | `@ignaciano3/better-pdf` (default) | zero-config |
| Bun | `@ignaciano3/better-pdf` (default) | zero-config |
| Deno | `npm:@ignaciano3/better-pdf` | zero-config |
| Vite | `@ignaciano3/better-pdf/browser` | `initializeWasm(import "…/wasm?url")` |
| webpack 5 | `@ignaciano3/better-pdf/browser` | `initializeWasm(new URL("…/wasm", import.meta.url).href)` |
| Next.js | `@ignaciano3/better-pdf/browser` (dynamic import) | `initializeWasm("/better_pdf_core_bg.wasm")` |
| Cloudflare Workers | `@ignaciano3/better-pdf/browser` | `initializeWasm(import "…/wasm" as WebAssembly.Module)` |
