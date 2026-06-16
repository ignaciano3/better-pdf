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
Browser support expects a modern bundler/runtime that can serve the packaged
`.wasm` asset referenced from the browser entry.
:::
