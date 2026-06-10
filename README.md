# better-pdf

A maintained, fast alternative to `pdf-lib` for filling and flattening existing PDF AcroForms.

`better-pdf` exposes a TypeScript API backed by a Rust core compiled to WebAssembly. The current package focuses on existing PDFs: load bytes, inspect form fields, queue field mutations, flatten fields, and save an incremental PDF update.

> Status: pre-alpha. The core AcroForm workflows are implemented for the bundled PDF 1.3 fixture corpus. Browser-specific packaging and generated form types are still future milestones.

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

Wrong-type access throws a clear error, for example calling `getDropdown()` on a text field.

### Signature Images

Visual signatures are appearances only. They do not create cryptographic/PAdES signatures.

Supported image inputs:

- JPEG, embedded directly as `/DCTDecode`.
- PNG, for 8-bit non-interlaced grayscale, RGB, grayscale+alpha, or RGBA images.

PNG alpha is currently dropped rather than preserved as a PDF soft mask.

## Limitations

- Existing PDFs only. Creating PDFs from scratch is out of scope for v1.
- No encrypted PDF support.
- No lenient recovery for malformed PDFs.
- No cryptographic signing.
- Primary test coverage is classic-xref PDF 1.3 forms from the bundled fixture corpus.
- Browser-specific package initialization is still a future milestone.

## Develop

Prerequisites: `bun`, the Rust toolchain, the `wasm32-unknown-unknown` target, and `wasm-pack`.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
bun install
bun run build      # compile Rust core to pkg/ and TypeScript API to dist/
bun test           # run TS API tests
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
