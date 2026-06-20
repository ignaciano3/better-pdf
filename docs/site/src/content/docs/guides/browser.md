---
title: Browser usage
description: Use better-pdf in the browser with the explicit browser entry.
---

Browser bundlers can import the explicit browser entry, or use the package root
when the bundler honors the `browser` export condition:

```ts
import { PdfDocument } from "@ignaciano3/better-pdf/browser";

const input = new Uint8Array(await file.arrayBuffer());
const doc = await PdfDocument.load(input);
const fields = doc.getForm().getFields();
const output = await doc.save();
```

`PdfDocument.load()` initializes the browser WASM module on first use.

:::note[Bundler requirements]
Browser bundlers must resolve and serve the `.wasm` binary from the
`@ignaciano3/better-pdf/wasm` asset subpath, then pass its URL to
`initializeWasm()` before any PDF operation:

- **Vite**: `import wasmUrl from "@ignaciano3/better-pdf/wasm?url"` → `initializeWasm(wasmUrl)`
- **webpack 5**: `new URL("@ignaciano3/better-pdf/wasm", import.meta.url).href` → `initializeWasm(url)`
- **Next.js**: copy wasm to `public/`, then `initializeWasm("/better_pdf_core_bg.wasm")`

See the [per-runtime guide](/guides/runtimes/) and
[examples/runtimes/](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/)
for complete working examples.
:::
