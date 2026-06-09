# Milestone 6 - Visual Signatures

**Goal:** Allow callers to place a visual signature image on an AcroForm signature field without cryptographic signing.

**Scope for this slice:**

- Add `form.getSignature(name).setImage(bytes)` to the TS API.
- Accept JPEG bytes first. JPEG can be embedded directly in PDF as `/DCTDecode`, so this keeps v1 dependency-free.
- Generate an `/AP/N` Form XObject for each signature widget that draws the image fit-centered inside the field rectangle.
- Do not create a digital-signature `/V` dictionary. This is visual only by design.
- Flatten remains a separate user action; existing flatten will stamp the generated appearance.

**Deferred:**

- PNG decoding/embedding.
- Cryptographic/PAdES signing.
- Signature metadata dictionaries.

## Tasks

- [x] Add a `PdfSignature` class and `getSignature()` typed accessor.
- [x] Extend the fill queue JSON with `image: number[]` operations.
- [x] Teach Rust fill resolution to validate signature fields and build JPEG image appearances.
- [x] Add Rust tests for JPEG dimension parsing and signature appearance generation.
- [x] Add TS API tests for setting a visual signature and wrong-type access.
- [x] Rebuild WASM and run Rust + TS test suites.
