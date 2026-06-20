# better-pdf — Next.js example

**Verified (build) — Next.js v15.5.19**

App Router example demonstrating `@ignaciano3/better-pdf` in a `"use client"` component. The WASM binary is served from `public/` (Next.js doesn't emit `node_modules` assets to the browser bundle).

## How it works

### The WASM copy step

Next.js doesn't bundle `.wasm` files from `node_modules` for browser use. Instead, the `postinstall` script copies the wasm binary into `public/` so Next.js serves it at `/better_pdf_core_bg.wasm`:

```sh
# Runs automatically after `npm install`, or manually:
node scripts/copy-wasm.mjs
```

### Client component

```tsx
"use client";  // required — WASM can only run in the browser

import { useCallback } from "react";

async function loadAndGenerate(): Promise<Uint8Array> {
  const { PdfDocument, initializeWasm, rgb, StandardFonts } = await import(
    "@ignaciano3/better-pdf/browser"    // dynamic import keeps it off the server
  );

  // Fetch the wasm from public/ (served as a static asset by Next.js)
  await initializeWasm("/better_pdf_core_bg.wasm");

  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from Next.js", {
    x: 50, y: 700, size: 24,
    font: StandardFonts.Helvetica,
    color: rgb(0, 0, 1),
  });
  return doc.save();
}
```

Key points:
- `"use client"` prevents the component from running on the server.
- `await import(…)` defers the import until the button is clicked, avoiding hydration issues.
- `initializeWasm("/better_pdf_core_bg.wasm")` fetches the file from `public/`.

## Install

> When using the published package, replace the tarball path with `@ignaciano3/better-pdf`.

```sh
npm install
# postinstall automatically runs: node scripts/copy-wasm.mjs
```

If you skip `npm install` and install packages another way, run the copy step manually:

```sh
node scripts/copy-wasm.mjs
```

## Run dev server

```sh
npm run dev
```

Open `http://localhost:3000` and click **Generate PDF**.

## Build for production

```sh
npm run build
```

### Verified build output

```
▲ Next.js 15.5.19

Creating an optimized production build ...
✓ Compiled successfully in 890ms
Linting and checking validity of types ...
Collecting page data ...
Generating static pages (4/4)
Finalizing page optimization ...
Collecting build traces ...

Route (app)                                 Size  First Load JS
┌ ○ /                                      841 B         103 kB
└ ○ /_not-found                            992 B         103 kB
+ First Load JS shared by all             102 kB

○  (Static)  prerendered as static content
```

## Start production server

```sh
npm run start
```

## Project structure

```
nextjs/
├── app/
│   ├── layout.tsx       # Root layout
│   └── page.tsx         # "use client" PDF generation component
├── public/
│   └── better_pdf_core_bg.wasm  # Copied by postinstall (gitignored)
├── scripts/
│   └── copy-wasm.mjs    # Postinstall: copies wasm from node_modules → public/
├── next.config.js
├── tsconfig.json
└── package.json
```
