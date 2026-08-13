---
title: Signatures
description: Add visual signature images to a signature field.
---

```ts
const signature = new Uint8Array(await Bun.file("signature.png").arrayBuffer());
form.getSignature("firma.titular").setImage(signature);

const output = await doc.save();
```

:::danger[Visual only]
Visual signatures are appearances only. They do **not** create
cryptographic/PAdES signatures.
:::

## Supported image inputs

- **JPEG** (grayscale or RGB), embedded directly as `/DCTDecode`. CMYK JPEGs are
  rejected.
- **PNG**, for 8-bit non-interlaced grayscale, RGB, grayscale+alpha, or RGBA
  images.

PNG alpha is preserved as a PDF soft mask (`/SMask`), so transparent signature
images composite correctly over the page.

Invalid bytes throw `InvalidImageError` at `save()` time.
