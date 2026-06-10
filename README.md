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
`required`, `exported` (false when the field has the `NoExport` flag), `maxLength`
(a text field's `/MaxLen`, or `null`), and `widgets` — one entry per widget
annotation giving its 0-based `page` index and `rect` (`[x0, y0, x1, y1]` in PDF
points, origin bottom-left). `setText()` throws if its value exceeds `maxLength`.

List boxes are single-select in this version.

### Errors

Every error thrown by the form API subclasses `PdfError`, so you can catch the
whole family or a specific case:

- `UnknownFieldError` — no field with that name (`.field`).
- `FieldTypeError` — field accessed as the wrong type, e.g. `getDropdown()` on a
  text field (`.field`, `.actual`, `.expected`).
- `InvalidOptionError` — selecting a value that is not one of the field's options
  (`.field`, `.fieldType`, `.value`, `.options`).
- `MaxLengthExceededError` — `setText()` value longer than the field's `/MaxLen`
  (`.field`, `.maxLength`, `.actualLength`).
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

`better-pdf` is consistently faster than `pdf-lib` on end-to-end mutation
workloads, thanks to its Rust/WebAssembly core and append-only incremental
saves. Indicative results from `bun run bench` on the bundled fixture corpus
(25 iterations after warmup):

### Small mixed form

`Form.-D.P.-2.4.1-Ficha-personal.pdf` — 57 KB, 30 fields: text, radio, dropdown.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.03 ms | 1.39 ms | 45.2× |
| load + read fields | 1.43 ms | 0.82 ms | 0.6× |
| fill 24 text fields + save | 3.07 ms | 6.13 ms | 2.0× |
| fill 2 choice fields + save | 2.64 ms | 4.91 ms | 1.9× |
| flatten all + save | 2.79 ms | 5.03 ms | 1.8× |

### Medium dense form

`Modulo-de-Diabetes.pdf` — 259 KB, 109 fields: text, radio, checkbox, dropdown, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.08 ms | 13.77 ms | 163.0× |
| load + read fields | 5.52 ms | 5.62 ms | 1.0× |
| fill 24 text fields + save | 11.67 ms | 26.76 ms | 2.3× |
| fill 19 choice fields + save | 11.70 ms | 27.24 ms | 2.3× |
| stamp 2 signature images + save | 16.98 ms | n/a | n/a |
| stamp first signature + flatten it | 19.31 ms | n/a | n/a |
| flatten all + save | 13.18 ms | error | n/a |

### Large signature form

`Convenio-OSFATUN-Discapacidad-2022.pdf` — 735 KB, 22 fields: text, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.26 ms | 1.39 ms | 5.3× |
| load + read fields | 1.19 ms | 0.74 ms | 0.6× |
| fill 20 text fields + save | 2.88 ms | 4.25 ms | 1.5× |
| stamp 2 signature images + save | 8.36 ms | n/a | n/a |
| stamp first signature + flatten it | 6.80 ms | n/a | n/a |
| flatten all + save | 2.62 ms | error | n/a |

In the two `error` rows, `pdf-lib` threw `Unexpected N type: undefined` while
flattening real-world fixtures. Absolute timings vary by machine; reproduce
them on yours with `bun run bench` (set `BENCH_ITER` to change the iteration
count).

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

Prerequisites: `bun`, the Rust toolchain, the `wasm32-unknown-unknown` target, `wasm-pack`, and `wasm-opt` from Binaryen.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
# macOS: brew install binaryen
# Linux: install the binaryen package for your distro
bun install
bun run build      # compile Rust core to pkg/ + pkg-web/ and TypeScript API to dist/
bun test           # run TS API tests
bun run test:browser-entry
bun run test:browser  # load the web build in headless Chromium (needs `bunx playwright install chromium`)
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

API reference (TypeDoc → `docs/api`):

```bash
bun run docs
```

### Releasing

Publishing is automated by `.github/workflows/release.yml`: push a `vX.Y.Z` tag
that matches `package.json`'s `version` and it builds and runs
`npm publish --provenance`. It needs an `NPM_TOKEN` repo secret (an npm
automation token). Provenance is attached via OIDC, so publishing only works from
CI — a local `npm publish` fails because `publishConfig.provenance` is `true`.

```bash
npm version patch   # or minor / major — bumps package.json + creates the tag
git push --follow-tags
```
