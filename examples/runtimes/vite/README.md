# better-pdf — Vite example

**Verified (build) — Vite v5.4.21**

Demonstrates loading the `@ignaciano3/better-pdf` WASM in the browser via Vite's native `?url` import suffix. Clicking a button generates a one-page PDF and renders it in an `<iframe>`.

## How it works

```ts
import { PdfDocument, initializeWasm, rgb, StandardFonts }
  from "@ignaciano3/better-pdf/browser";
import wasmUrl from "@ignaciano3/better-pdf/wasm?url";  // Vite resolves to asset URL

await initializeWasm(wasmUrl);
const doc = await PdfDocument.create();
// ...
const bytes = await doc.save(); // Uint8Array → Blob → <iframe>
```

Vite emits the `.wasm` file as a content-hashed asset and the `?url` import resolves to its final URL. No WASM plugin is required — the native `?url` suffix is sufficient.

## Install

> When using the published package, replace the tarball path with `@ignaciano3/better-pdf`.

```sh
npm install
```

## Run dev server

```sh
npm run dev
```

Then open `http://localhost:5173` in a browser.

## Build for production

```sh
npm run build
```

### Verified build output

```
vite v5.4.21 building for production...
transforming...
✓ 26 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                                   0.82 kB │ gzip:  0.48 kB
dist/assets/better_pdf_core_bg-CAu2J2md.wasm  1,919.87 kB
dist/assets/index-7WNiJ7Wq.js                    42.61 kB │ gzip: 12.28 kB
✓ built in 209ms
```

## Preview production build

```sh
npm run preview
```
