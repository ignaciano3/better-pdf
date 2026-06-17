# better-pdf

A maintained, fast alternative to `pdf-lib` for PDF AcroForms and document generation.

`better-pdf` exposes a TypeScript API backed by a Rust core compiled to WebAssembly. It covers two workflows: (1) **AcroForm-first** — load an existing PDF, inspect fields, fill/flatten/sign, and save an incremental update; and (2) **generate & draw** — create new PDFs from scratch or stamp text, images, and vector graphics onto existing pages.

> **Status:** 0.4.x, pre-1.0. The core AcroForm workflows — reading, filling, flattening, visual signatures, and typed form-type generation — are implemented and tested against the bundled PDF 1.3 fixture corpus. PDF generation (create, addPage, drawText, drawImage, drawRectangle, drawLine, drawEllipse) is available, and custom TTF/OTF font embedding with Unicode/CJK support is new in 0.4.0. The public API may still change before 1.0.

Coming from pdf-lib? See the [migration guide](docs/migrating-from-pdf-lib.md).

## Features

- Read AcroForm fields with fully-qualified names, types, values, options, and button states.
- Fill text fields and text areas.
- Check/uncheck checkboxes using the real on-state value.
- Select radio options using real export values.
- Select dropdown and list-box options.
- Add visual-only signature images from JPEG or supported PNG bytes.
- Flatten one field or all fields after filling.
- Save append-only incremental PDF updates.
- Create new PDFs with `PdfDocument.create()` and standard page sizes.
- Draw text, images, lines, rectangles, and ellipses on new and existing pages.
- Embed custom TTF/OTF fonts with glyph subsetting for full Unicode text (CJK, accented Latin, any script) — selectable and searchable in PDF viewers.
- Create fillable AcroForm fields (text, checkbox, radio, dropdown, listbox, signature) on generated documents with `doc.createForm()`.

## Install

```bash
bun add @ignaciano3/better-pdf
```

For local development from this repository:

```bash
bun install
bun run build
```

## Usage

```ts
import { PdfDocument } from "@ignaciano3/better-pdf";

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

## Generating & drawing

Import from the `./generate` subpath (or the package root — both export the same classes):

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";
```

### (a) Create a document, draw text

```ts
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);               // 595 × 842 pt

const font = doc.getFont(StandardFonts.Helvetica);
const text = "Hello, world!";
const textWidth = font.widthOfTextAtSize(text, 24);   // centre-align helper

page.drawText(text, {
  x: (PageSizes.A4[0] - textWidth) / 2,
  y: 750,
  size: 24,
  font,
  color: rgb(0.1, 0.2, 0.8),
});

const output = await doc.save();   // returns Uint8Array
await Bun.write("hello.pdf", output);
```

> **Coordinate system:** origin is bottom-left, y increases upward — same as pdf-lib and raw PDF.
> **Fonts:** standard-14 (Helvetica, HelveticaBold, Courier, CourierBold, TimesRoman, TimesBold, TimesItalic, TimesBoldItalic, and more) are the default; use `doc.embedFont(bytes)` for Unicode/CJK text (see [(f) Custom fonts](#f-custom-fonts)).

### (b) Stamp onto an existing PDF

```ts
import { PdfDocument, rgb } from "@ignaciano3/better-pdf";

const bytes = new Uint8Array(await Bun.file("existing.pdf").arrayBuffer());
const doc = await PdfDocument.load(bytes);

const imgBytes = new Uint8Array(await Bun.file("logo.png").arrayBuffer());
const img = await doc.embedPng(imgBytes);             // PdfImage with .width / .height
const scaled = img.scale(0.5);                        // { width, height }

const page = doc.getPage(0);
page.drawImage(img, { x: 40, y: 700, width: scaled.width, height: scaled.height });
page.drawText("Confidential", { x: 40, y: 680, size: 12, color: rgb(0.8, 0, 0) });

const output = await doc.save();
```

`embedJpg` works the same way for JPEG files. Both methods are available on loaded and created documents.

### (c) Vector graphics

```ts
// filled + bordered rectangle with transparency
page.drawRectangle({
  x: 50, y: 50, width: 200, height: 100,
  color: rgb(0.9, 0.95, 1),
  borderColor: rgb(0.2, 0.4, 0.8),
  borderWidth: 2,
  opacity: 0.85,
});

// line
page.drawLine({
  start: { x: 50, y: 40 },
  end:   { x: 250, y: 40 },
  thickness: 1.5,
  color: rgb(0.5, 0.5, 0.5),
});

// ellipse — (x, y) is the centre; xScale/yScale are the x and y radii
page.drawEllipse({ x: 150, y: 200, xScale: 60, yScale: 30, color: rgb(1, 0.8, 0) });
```

### (d) Text layout with `widthOfTextAtSize`

```ts
const font = doc.getFont(StandardFonts.HelveticaBold);
const label = "Invoice #1234";
const w = font.widthOfTextAtSize(label, 16);
page.drawText(label, { x: pageWidth - w - 40, y: pageHeight - 60, size: 16, font });
```

### (e) Custom fonts (Unicode / CJK)

Embed any TTF or OTF font to draw Unicode text. The font is stored as a
Type0/CIDFontType2 composite with a ToUnicode CMap, so text is selectable and
searchable. Works on both created and loaded documents.

```ts
import { PdfDocument, PageSizes } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);

const fontBytes = new Uint8Array(await Bun.file("NotoSansCJK-Regular.ttf").arrayBuffer());

// subset: true (default) — strip unused glyphs; keeps file size small
const font = await doc.embedFont(fontBytes, { subset: true });

const text = "日本語テキスト — Héllo Wörld";
const textWidth = font.widthOfTextAtSize(text, 18);

page.drawText(text, {
  x: (PageSizes.A4[0] - textWidth) / 2,
  y: 700,
  size: 18,
  font,
});

const output = await doc.save();
await Bun.write("unicode.pdf", output);
```

> **OpenType-CFF:** The subsetter supports TrueType (`glyf`) outlines. `.otf`
> files with CFF outlines may fail to subset — pass `{ subset: false }` for
> those. Characters with no glyph are silently skipped.

### (f) Creating form fields

On a document created with `PdfDocument.create()`, call `doc.createForm()` to get
a chainable `FormBuilder` and declare AcroForm fields. There are six field types —
`addTextField`, `addCheckBox`, `addRadioGroup`, `addDropdown`, `addListBox`, and
`addSignatureField` — each placed by `page` index plus a position/size in PDF
points. The fields are serialized into the document on `save()`.

```ts
import { PdfDocument, PageSizes, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
doc.addPage(PageSizes.A4);

const form = doc
  .createForm()
  .addTextField("applicant.name", {
    page: 0, x: 56, y: 740, width: 240, height: 22,
    value: "GARCIA, IGNACIO",
    maxLength: 64,
    border: { color: rgb(0.1, 0.1, 0.4), width: 1 },
    background: rgb(0.97, 0.97, 1),
  })
  .addTextField("applicant.notes", {
    page: 0, x: 56, y: 660, width: 240, height: 60, multiline: true,
  })
  .addCheckBox("applicant.agree", {
    page: 0, x: 56, y: 620, size: 14, checked: true, required: true,
  })
  .addRadioGroup("applicant.kind", {
    selected: "primary",
    options: [
      { value: "primary", page: 0, x: 56, y: 590, size: 14 },
      { value: "dependent", page: 0, x: 120, y: 590, size: 14 },
    ],
  })
  .addDropdown("applicant.status", {
    page: 0, x: 56, y: 550, width: 160, height: 22,
    options: ["single", "married"], selected: "married",
  })
  .addListBox("applicant.plan", {
    page: 0, x: 56, y: 500, width: 160, height: 48,
    options: ["basic", "plus", "premium"],
  })
  .addSignatureField("applicant.signature", {
    page: 0, x: 56, y: 440, width: 200, height: 48,
  });

console.log(form.getFieldNames()); // typed array of the declared names

const output = await doc.save();
await Bun.write("form.pdf", output);
```

> **Created documents only.** `createForm()` throws on documents opened with
> `PdfDocument.load()`. The field names are accumulated into the builder's type,
> so `getFieldNames()` is statically typed.
>
> **A normal fillable form.** The result is a standard AcroForm: reload it with
> `PdfDocument.load(output)` and you can fill it (`getForm().getTextField(...)`,
> `.getCheckBox(...).check()`, …) and flatten it with this same library.

Every field supports `required`, `readOnly`, `tooltip`, and the optional
`border` (`{ color, width? }`) / `background` (a `Color`) appearance — colors come
from `rgb(r, g, b)` and `grayscale(v)` (0–1).

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

## API

### `PdfDocument`

- `PdfDocument.load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument>` — open an existing PDF
- `PdfDocument.create(): Promise<PdfDocument>` — create a new empty document
- `doc.addPage(size: [number, number]): PdfPage` — add a page; `PageSizes.A4` etc. are `[width, height]` tuples
- `doc.getPageCount(): number`
- `doc.getPages(): PdfPage[]`
- `doc.getPage(index: number): PdfPage` — throws `PageOutOfRangeError` if out of bounds
- `doc.getFont(font: StandardFonts): PdfFont` — standard-14 fonts (sync)
- `doc.embedFont(bytes: Uint8Array, options?: { subset?: boolean }): Promise<PdfFont>` — embed a TTF/OTF font; `subset` defaults to `true`
- `doc.embedJpg(bytes: Uint8Array): Promise<PdfImage>`
- `doc.embedPng(bytes: Uint8Array): Promise<PdfImage>`
- `doc.getForm(): PdfForm`
- `doc.save(): Promise<Uint8Array>`

`save()` applies queued fills first, then queued flattens. With no queued operations it returns a byte-identical round trip.
`save()` always starts from the originally loaded bytes (calling it twice
returns the same result), and `FieldInfo.value` reflects queued mutations as
soon as they are made.

**`PageSizes`**: `A3`, `A4`, `A5`, `Letter`, `Legal`, `Tabloid` — each is a `[width, height]` tuple in PDF points.

**`StandardFonts`** (12 standard fonts): `Helvetica`, `HelveticaBold`, `HelveticaOblique`, `HelveticaBoldOblique`, `Courier`, `CourierBold`, `CourierOblique`, `CourierBoldOblique`, `TimesRoman`, `TimesBold`, `TimesItalic`, `TimesBoldItalic`. (`Symbol` and `ZapfDingbats` are intentionally omitted.)

### `PdfPage`

- `page.drawText(text, options)` — `options`: `{ x, y, size, font?, color?, lineHeight? }`
- `page.drawImage(image, options)` — `options`: `{ x, y, width?, height? }`
- `page.drawLine(options)` — `options`: `{ start: {x,y}, end: {x,y}, thickness?, color?, opacity? }`
- `page.drawRectangle(options)` — `options`: `{ x, y, width, height, color?, borderColor?, borderWidth?, opacity? }`
- `page.drawEllipse(options)` — `options`: `{ x, y, xScale, yScale, color?, borderColor?, borderWidth?, opacity? }` (`x`,`y` = center; `xScale`,`yScale` = radii)

Available on both loaded pages (`doc.getPage(i)`) and created pages (`doc.addPage(...)`).

### `PdfImage`

- `image.width: number`
- `image.height: number`
- `image.scale(factor: number): { width: number; height: number }`

### `PdfFont`

- `font.widthOfTextAtSize(text: string, size: number): number`

### Color helpers

```ts
import { rgb, grayscale } from "@ignaciano3/better-pdf";
```

- `rgb(r, g, b)` — values 0–1
- `grayscale(v)` — value 0–1

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

Every error thrown by the library subclasses `PdfError`, so you can catch the
whole family or a specific case:

- `UnknownFieldError` — no field with that name (`.field`).
- `FieldTypeError` — field accessed as the wrong type, e.g. `getDropdown()` on a
  text field (`.field`, `.actual`, `.expected`).
- `InvalidOptionError` — selecting a value that is not one of the field's options
  (`.field`, `.fieldType`, `.value`, `.options`).
- `MaxLengthExceededError` — `setText()` value longer than the field's `/MaxLen`
  (`.field`, `.maxLength`, `.actualLength`).
- `MissingOnStateError` — checking a checkbox with no declared on-state (`.field`).
- `PdfCoreError` — an operation the core rejected at `save()` time (XFA forms,
  unsupported images, malformed PDFs); the core's message is preserved.
- `PageOutOfRangeError` — `getPage(i)` called with an index outside `[0, pageCount)`.
- `InvalidImageError` — `embedJpg`/`embedPng` rejected the image bytes (unsupported format or CMYK JPEG).

```ts
import { FieldTypeError } from "@ignaciano3/better-pdf";

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

- JPEG (grayscale or RGB), embedded directly as `/DCTDecode`. CMYK JPEGs are rejected.
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
(50 iterations after warmup):

The **fill** and **flatten** rows are the like-for-like comparison. The
*load + save unchanged* rows compare better-pdf's no-op incremental round-trip
(it returns the original bytes) against pdf-lib's full parse + re-serialize —
they showcase the architectural difference, not parser speed.

### Small mixed form

`Form.-D.P.-2.4.1-Ficha-personal.pdf` — 57 KB, 30 fields: text, radio, dropdown.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.02 ms | 1.29 ms | 58.4× |
| load + read fields | 0.48 ms | 0.79 ms | 1.7× |
| fill 24 text fields + save | 1.10 ms | 5.86 ms | 5.3× |
| fill 2 choice fields + save | 0.80 ms | 4.57 ms | 5.7× |
| flatten all + save | 0.89 ms | 4.83 ms | 5.5× |

### Medium dense form

`Modulo-de-Diabetes.pdf` — 259 KB, 109 fields: text, radio, checkbox, dropdown, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.07 ms | 13.89 ms | 186.4× |
| load + read fields | 1.70 ms | 5.57 ms | 3.3× |
| fill 24 text fields + save | 3.87 ms | 26.43 ms | 6.8× |
| fill 19 choice fields + save | 3.77 ms | 27.03 ms | 7.2× |
| stamp 2 signature images + save | 8.31 ms | n/a | n/a |
| stamp first signature + flatten it | 7.49 ms | n/a | n/a |
| flatten all + save | 4.68 ms | error | n/a |

### Large signature form

`Convenio-OSFATUN-Discapacidad-2022.pdf` — 735 KB, 22 fields: text, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.24 ms | 1.33 ms | 5.5× |
| load + read fields | 0.36 ms | 0.78 ms | 2.1× |
| fill 20 text fields + save | 1.02 ms | 4.36 ms | 4.3× |
| stamp 2 signature images + save | 5.94 ms | n/a | n/a |
| stamp first signature + flatten it | 3.81 ms | n/a | n/a |
| flatten all + save | 0.96 ms | error | n/a |

### PDF generation

Building or stamping documents from scratch (no fixture). The `create + draw`
rows compare against `pdf-lib`'s equivalent generation API; vector shapes have
no direct `pdf-lib` one-liner equivalent.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| create + draw text | 0.15 ms | 1.25 ms | 8.2× |
| stamp text on existing | 1.10 ms | 2.16 ms | 2.0× |
| create + draw image | 0.07 ms | 0.50 ms | 7.3× |
| create + vector shapes | 0.09 ms | n/a | n/a |

In the two `error` rows, `pdf-lib` threw `Unexpected N type: undefined` while
flattening real-world fixtures. Absolute timings vary by machine; reproduce
them on yours with `bun run bench` (set `BENCH_ITER` to change the iteration
count).

## Limitations

- XFA forms are detected and rejected on fill/flatten (reading fields still works).
- No encrypted PDF support.
- No lenient recovery for malformed PDFs.
- No cryptographic signing.
- List boxes are single-select; multi-select list boxes are not yet supported.
- Text fields are single-line; multi-line wrapping is not yet generated.
- Drawing APIs support standard-14 fonts and custom TTF/OTF font embedding via
  `doc.embedFont(bytes)` (Type0/CIDFontType2, full Unicode including CJK).
  OpenType-CFF subsetting may be unsupported — use `{ subset: false }` for CFF-outline `.otf` fonts.
  Characters with no glyph in the font are silently skipped.
- Appearance metrics cover the standard 14 text fonts (with Arial / Times New
  Roman / Courier New aliases and subset-prefix handling) and any simple font
  carrying a `/Widths` array; unrecognized fonts fall back to Helvetica metrics.
- Color: RGB and grayscale only; CMYK is not supported.
- Primary test coverage is the bundled fixture corpus (classic-xref PDF 1.3 forms,
  plus generated xref-stream/object-stream variants).
- Browser support expects a modern bundler/runtime that can serve the packaged
  `.wasm` asset referenced from the browser entry.

## Develop

Prerequisites: `bun`, the Rust toolchain, the `wasm32-unknown-unknown` target, and `wasm-pack` (it downloads and runs its own `wasm-opt`, so no system Binaryen is needed).

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
bun install
bun run build      # compile Rust core to pkg-web/ and TypeScript API to dist/
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
