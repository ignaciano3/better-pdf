# Milestone 7 - PNG Visual Signatures

**Goal:** Allow `PdfSignature.setImage(bytes)` to accept PNG signatures as well as JPEG signatures.

**Scope for this slice:**

- Detect PNG bytes by signature.
- Support 8-bit, non-interlaced PNGs with grayscale, RGB, grayscale+alpha, or RGBA color.
- Inflate IDAT data, reverse PNG row filters, and drop alpha for PDF embedding.
- Reuse the existing visual signature appearance path and cover-scaling behavior.
- Keep cryptographic signing out of scope.

**Deferred:**

- Indexed-color/palette PNGs.
- 16-bit PNGs.
- Adam7 interlaced PNGs.
- Preserving alpha as a PDF soft mask.

## Tasks

- [x] Add direct `flate2` dependency for PNG IDAT inflation.
- [x] Add PNG parser/decoder helpers in the appearance engine.
- [x] Extend signature fill resolution to accept JPEG or PNG image data.
- [x] Add Rust tests for PNG decoding and rejection of unsupported PNG variants.
- [x] Add TS/playground verification with `signature.png`.
- [x] Rebuild WASM and run Rust + TS suites.
