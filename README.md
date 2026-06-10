# better-pdf

A maintained, fast alternative to `pdf-lib` for filling and flattening existing PDF AcroForms.

`better-pdf` exposes a TypeScript API backed by a Rust core compiled to WebAssembly. The current package focuses on existing PDFs: load bytes, inspect form fields, queue field mutations, flatten fields, and save an incremental PDF update.

> **Status:** pre-alpha. The core AcroForm workflows — reading, filling, flattening, visual signatures, and typed form-type generation — are implemented and tested against the bundled PDF 1.3 fixture corpus.

## Features

- Read AcroForm fields with fully-qualified names, types, values, options, and button states.
- Fill text fields and text areas.
- Check/uncheck checkboxes using the real on-state value.
- Select radio options using real export values.
- Select dropdown/listbox options.
- Add visual-only signature images from JPEG or supported PNG bytes.
- Flatten one field or all fields after filling.
- Save append-only incremental PDF updates.

## Install

```bash
bun add better-pdf
```

For local development from this repository:

```bash
bun install
bun run build
```

## Usage

```ts
import { PdfDocument } from "better-pdf";

const input = new Uint8Array(await Bun.file("form.pdf").arrayBuffer());
const doc = await PdfDocument.load(input);
const form = doc.getForm();

for (const field of form.getFields()) {
  console.log(field.name, field.type, field.value);
}

form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA, IGNACIO");
form.getRadioGroup("beneficiario.tipo_beneficiario").select("Titular");
form.getDropdown("beneficiario.estado_civil").select("Casado");
form.getCheckBox("declaracion.acepta").check();

const signature = new Uint8Array(await Bun.file("signature.png").arrayBuffer());
form.getSignature("firma.titular").setImage(signature);

form.flattenField("beneficiario.apellidos_nombres");

const output = await doc.save();
await Bun.write("filled.pdf", output);
```

Browser bundlers can import the explicit browser entry, or use the package root
when the bundler honors the `browser` export condition:

```ts
import { PdfDocument } from "better-pdf/browser";

const input = new Uint8Array(await file.arrayBuffer());
const doc = await PdfDocument.load(input);
const fields = doc.getForm().getFields();
const output = await doc.save();
```

`PdfDocument.load()` initializes the browser WASM module on first use.

## API

### `PdfDocument`

- `PdfDocument.load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument>`
- `doc.getForm(): PdfForm`
- `doc.save(): Promise<Uint8Array>`

`save()` applies queued fills first, then queued flattens. With no queued operations it returns a byte-identical round trip.

### `PdfForm`

- `form.getFields(): FieldInfo[]`
- `form.getField(name: string): FieldInfo | undefined`
- `form.getTextField(name).setText(value)`
- `form.getCheckBox(name).check()`
- `form.getCheckBox(name).uncheck()`
- `form.getRadioGroup(name).options`
- `form.getRadioGroup(name).select(value)`
- `form.getDropdown(name).options`
- `form.getDropdown(name).select(value)`
- `form.getSignature(name).setImage(bytes)`
- `form.flattenField(name)`
- `form.flatten()`

Each `FieldInfo` carries `name`, `type`, `value`, `states`, `options`, `readOnly`,
`required`, and `widgets` — one entry per widget annotation giving its 0-based
`page` index and `rect` (`[x0, y0, x1, y1]` in PDF points, origin bottom-left).

Wrong-type access throws a clear error, for example calling `getDropdown()` on a text field.

### Generate Form Types

Generate a TypeScript module from an existing PDF:

```bash
better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
```

The generated module exports field-name unions and literal metadata for field
types, dropdown/listbox options, radio states, read-only flags, and current
values.

Then pass the generated metadata as a type argument to get a fully-narrowed
form — unknown field names, wrong-type access, and invalid option/state values
become compile errors, at zero runtime cost (the schema is referenced only via
`typeof`):

```ts
import { myFormFields } from "./form-types.js";

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```

The untyped `doc.getForm()` keeps working unchanged.

### Signature Images

Visual signatures are appearances only. They do not create cryptographic/PAdES signatures.

Supported image inputs:

- JPEG, embedded directly as `/DCTDecode`.
- PNG, for 8-bit non-interlaced grayscale, RGB, grayscale+alpha, or RGBA images.

PNG alpha is currently dropped rather than preserved as a PDF soft mask.

## For AI agents

better-pdf ships an [agent skill](skills/better-pdf/SKILL.md) — procedural
knowledge for driving the library correctly (the load → inspect → generate
types → fill/flatten/sign → save workflow, plus the non-obvious rules: use a
field's *real* export values, never assume `Yes`/`On`; visual signatures are not
cryptographic; `save()` is an incremental update). It installs into 20+ agents
via [skills.sh](https://www.skills.sh).

The strongest agent-readiness feature is the typed workflow above: generate a
types module from the PDF and `doc.getForm<typeof myFormFields>()` turns
hallucinated field names and invalid values into compile errors.

## Benchmarks

`better-pdf` is consistently faster than `pdf-lib` on the same end-to-end
operations, thanks to its Rust/WebAssembly core and append-only incremental
saves. Indicative results on the bundled fixture (50 iterations after warmup):

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + read fields | 0.49 ms | 0.92 ms | 1.9× |
| fill 10 text fields + save | 1.05 ms | 5.41 ms | 5.1× |
| flatten all + save | 1.01 ms | 5.12 ms | 5.1× |

Absolute timings vary by machine; reproduce them on yours with `bun run bench`
(set `BENCH_ITER` to change the iteration count).

## Limitations

- Existing PDFs only. Creating PDFs from scratch is out of scope for v1.
- No encrypted PDF support.
- No lenient recovery for malformed PDFs.
- No cryptographic signing.
- Primary test coverage is classic-xref PDF 1.3 forms from the bundled fixture corpus.
- Browser support expects a modern bundler/runtime that can serve the packaged
  `.wasm` asset referenced from the browser entry.

## Develop

Prerequisites: `bun`, the Rust toolchain, the `wasm32-unknown-unknown` target, and `wasm-pack`.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
bun install
bun run build      # compile Rust core to pkg/ + pkg-web/ and TypeScript API to dist/
bun test           # run TS API tests
bun run test:browser-entry
bun run typecheck  # run TypeScript checks
npm pack --dry-run # inspect package contents
```

Rust checks:

```bash
cargo test --manifest-path crates/core/Cargo.toml
cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings
```

Manual playground:

```bash
bun run play
bun run play tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf signature.png
```

Benchmarks against `pdf-lib`:

```bash
bun run bench
```

## Release hygiene (real blockers for credible v1)

1. version still 0.0.0 → set real semver. Recommend 0.1.0 (0.x = API still settling), not 1.0.0.
2. Missing npm metadata — no repository, homepage, bugs, author, engines, publishConfig. Needed for npmjs links + npm publish --provenance. Same for crates/core/Cargo.toml (no description/repository — wasm-pack warns).
3. No CI (.github/). Add Actions: cargo test+clippy, bun test, typecheck, build on PR. Stops regressions.
4. No CHANGELOG.md.
5. README says "pre-alpha" — reword for release.

Functional gap (one real inconsistency)

6. No getListBox(). listbox is a FieldType and is readable, but there's no write accessor — getDropdown() on a listbox throws. Either add getListBox().select() (listbox /V can be multi-select → bit more work) or explicitly document listbox as read-only in v1. Corpus has ~no listboxes, so documenting is defensible.

Optional (post-v1 fine)

7. Typed error classes (10 sites throw bare Error) — nicer catch.
8. Multi-line text wrapping (deferred) + PNG-alpha drop — confirm both are in Limitations.
9. Real browser test (only smoke today) / typedoc API page.

My rec: do 1–5 (hygiene) now — they're cheap and genuinely gate a professional release. For 6, document listbox as read-only unless you expect listbox forms. Defer 7–9.

Want me to do the hygiene set (1–5) + document listbox?