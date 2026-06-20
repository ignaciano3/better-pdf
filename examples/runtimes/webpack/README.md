# better-pdf — webpack 5 example

**Verified (build) — webpack v5.107.2**

Demonstrates loading the `@ignaciano3/better-pdf` WASM in the browser via webpack 5's `new URL(specifier, import.meta.url)` pattern and `asyncWebAssembly` experiment.

## How it works

```js
import { PdfDocument, initializeWasm, rgb, StandardFonts }
  from "@ignaciano3/better-pdf/browser";

// webpack 5 statically analyses `new URL(…, import.meta.url)` and emits the
// referenced file as a content-hashed asset, replacing the expression with
// the runtime URL.
const wasmUrl = new URL("@ignaciano3/better-pdf/wasm", import.meta.url);

await initializeWasm(wasmUrl.href);
const doc = await PdfDocument.create();
// ...
const bytes = await doc.save();
```

`webpack.config.js` sets:
- `experiments.asyncWebAssembly: true` — enables async WASM module loading
- `module.rules[].type: "asset/resource"` for `.wasm` files — emits them as static assets

## Install

> When using the published package, replace the tarball path with `@ignaciano3/better-pdf`.

```sh
npm install
```

## Build for production

```sh
npm run build
# or equivalently:
npx webpack --mode=production
```

### Verified build output

```
asset 321e162d5927503d3c6a.wasm 1.83 MiB [emitted] [immutable] [from: node_modules/.../better_pdf_core_bg.wasm]
asset bundle.js 41.5 KiB [emitted] [minimized] (name: main)

WARNING in asset size limit: The following asset(s) exceed the recommended size limit (244 KiB).
  321e162d5927503d3c6a.wasm (1.83 MiB)

webpack 5.107.2 compiled with 2 warnings in 627 ms
```

The wasm-size warning is expected — the WASM binary is ~1.8 MB. For production deployments consider serving it with Brotli/gzip compression.

## Serve locally

Open `index.html` from an HTTP server (not `file://`, as modules won't load):

```sh
npx serve .
```

Then open `http://localhost:3000` in a browser.
