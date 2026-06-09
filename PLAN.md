# Better-PDF

I want to generate a better alternative to [pdf-lib](https://www.npmjs.com/package/pdf-lib) that is not mantained since 2021. 


## Requirements

It has to fulfill this requirements for the first release:

- Fill Pdf Acrofields
    - Set text on Text Fields on Text Area Fields
    - Choose a radio on RadioGroups
    - Choose an option on dropdowns
    - Add a *visual* signature on signature fields (embed an image/appearance into the field widget — no cryptography in the first release; cryptographic/PAdES signing is explicitly out of scope for v1, API should leave room to add it later)
    - Select on checkboxes

- Flatten acrofields
    - One specific
    - All

### Scope

The first release operates on **existing** PDFs only: load → fill/flatten → save. It does **not** create PDFs from scratch or draw arbitrary page content (beyond what is needed to render field appearances and the visual signature image).

**Input PDFs it must handle:**
- **Primary (mandatory):** well-formed PDFs with **classic cross-reference tables** and `FlateDecode` streams — this is what the entire fixture corpus (22 real OSFATUN forms, all PDF 1.3) uses.
- **Secondary (goal):** modern compressed PDFs with cross-reference streams and object streams (PDF 1.5+). Not exercised by the current fixtures, so it needs dedicated test PDFs.

**What the real fixtures tell us (and require):**
- All 22 forms set `/NeedAppearances true` and ship without value appearance streams → the appearance engine is essential; we generate appearances ourselves (can't rely on existing ones), which is also mandatory for flattening.
- The default appearance font is **Helvetica (`Helv`)** across the corpus → standard-14 font metrics fully cover v1.
- Field mix: ~774 text, ~140 button (checkbox/radio), ~52 signature, ~4 choice/dropdown → dropdowns have thin real-world coverage; note this in tests.
- Button export/on-state values are **domain-specific** (`Yes/Off`, `F/M`, `SI/NO`, `Titular/Familiar`, multi-option groups) → the API must expose each field's valid states and operate on **actual** export/on-state values; never assume `/Yes`.

**Explicitly out of scope for v1** (candidates for later releases):
- Encrypted PDFs
- Lenient recovery of malformed / off-spec PDFs
- Cryptographic / PAdES digital signatures
- Creating PDFs from scratch / general content drawing

### Technical Requirements

- It has to be a public npm package
- It has to use Rust for CPU intensive tasks (e.g. PDF parsing), compiled to **WebAssembly** so it runs in both the browser and server (Node/Bun/Deno/edge), matching pdf-lib's "runs everywhere" property
- The public API is plain JS/TS; the WASM binary is bundled inside the package
- Architecture: **Rust core + thin JS API**. Rust owns the full PDF document model (parse, object graph, field mutation, appearance-stream generation, serialization). JS is a thin, fully-typed, ergonomic wrapper. The JS↔WASM boundary is coarse (bytes in → operations → bytes out, plus field-metadata queries) to minimize crossing overhead.
- Use bun as package manager

### Non funcional Requirements

- Everything has to be thoroughly tested (preferrably using TDD)
- It has to be fast
- It should have minimal external dependencies — only the ones that are truly required (zero npm/runtime dependencies on the JS side; Rust crates allowed as build-time dependencies, kept to the minimum needed, e.g. for PDF parsing)
- Everything should be documented to later create a docs page

## New

- It should be AI agents ready
    - Decide if adding a skill for using this package is useful or not
- It should be tree-shakeable
- It should be as typed as possible
- Check for usefull skills on https://www.skills.sh/
- Add benchmarks comparing to pdf-lib and other relevant libraries
