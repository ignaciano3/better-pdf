# better-pdf

A maintained, fast alternative to `pdf-lib` for filling and flattening existing PDF AcroForms.

`better-pdf` exposes a TypeScript API backed by a Rust core compiled to WebAssembly. The current package focuses on existing PDFs: load bytes, inspect form fields, queue field mutations, flatten fields, and save an incremental PDF update.

> **Status:** 0.1.x, pre-1.0. The core AcroForm workflows — reading, filling, flattening, visual signatures, and typed form-type generation — are implemented and tested against the bundled PDF 1.3 fixture corpus. The public API may still change before 1.0.

## Features

- Read AcroForm fields with fully-qualified names, types, values, options, and button states.
- Fill text fields and text areas.
- Check/uncheck checkboxes using the real on-state value.
- Select radio options using real export values.
- Select dropdown and list-box options.
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
- `form.getListBox(name).options`
- `form.getListBox(name).select(value)`
- `form.getSignature(name).setImage(bytes)`
- `form.flattenField(name)`
- `form.flatten()`

Each `FieldInfo` carries `name`, `type`, `value`, `states`, `options`, `readOnly`,
`required`, and `widgets` — one entry per widget annotation giving its 0-based
`page` index and `rect` (`[x0, y0, x1, y1]` in PDF points, origin bottom-left).

List boxes are single-select in this version.

### Errors

Every error thrown by the form API subclasses `PdfError`, so you can catch the
whole family or a specific case:

- `UnknownFieldError` — no field with that name (`.field`).
- `FieldTypeError` — field accessed as the wrong type, e.g. `getDropdown()` on a
  text field (`.field`, `.actual`, `.expected`).
- `InvalidOptionError` — selecting a value that is not one of the field's options
  (`.field`, `.fieldType`, `.value`, `.options`).
- `MissingOnStateError` — checking a checkbox with no declared on-state (`.field`).

```ts
import { FieldTypeError } from "better-pdf";

try {
  form.getDropdown("some.text.field");
} catch (e) {
  if (e instanceof FieldTypeError) console.log(e.actual, e.expected);
}
```

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
- List boxes are single-select; multi-select list boxes are not yet supported.
- Text fields are single-line; multi-line wrapping is not yet generated.
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