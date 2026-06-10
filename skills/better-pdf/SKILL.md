---
name: better-pdf
description: Fill and flatten PDF AcroForm fields (text, checkbox, radio, dropdown, visual signature) in existing PDFs with the better-pdf npm package, and generate TypeScript types from a PDF form for compile-time-safe filling. Use when filling or flattening PDF forms, reading AcroForm fields, embedding a visual signature image, or when the user mentions better-pdf, pdf-lib, or AcroFields.
---

# better-pdf

A maintained, fast alternative to pdf-lib for **filling and flattening AcroForm fields in existing PDFs**. Rust→WASM core + a fully-typed TS API; runs in Node/Bun/Deno and the browser. Zero runtime npm deps.

## Quick start

```ts
import { PdfDocument } from "better-pdf";

const doc = await PdfDocument.load(bytes);        // Uint8Array | ArrayBuffer
const form = doc.getForm();

for (const f of form.getFields()) {
  console.log(f.type, f.name, f.states, f.options); // inspect BEFORE writing
}

form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getCheckBox("conformidad.acepto").check();
const out = await doc.save();                      // Promise<Uint8Array>
```

`load()` and `save()` are async (WASM init). `getForm()` returns the same instance each call, so mutations accumulate and apply on `save()`.

## Critical rules (agents get these wrong)

1. **Use the field's REAL export values — never assume `"Yes"`/`"On"`.** Corpus values are domain-specific (`F`/`M`, `SI`/`NO`, `Titular`/`Familiar`). Read them from `field.states` (checkbox/radio) or `field.options` (dropdown). `checkBox.check()` uses the field's actual on-state automatically; `uncheck()` sets `Off`.
2. **Existing PDFs only.** No creation from scratch / arbitrary drawing. Load → fill/flatten → save.
3. **Signatures are visual only** — an embedded image/appearance, NOT cryptographic/PAdES signing. `getSignature(name).setImage(jpegOrPngBytes)`.
4. **`save()` is an incremental (append-only) update** — output begins with the original bytes verbatim. With nothing queued it returns a byte-identical round-trip.
5. **Wrong-type access throws** (e.g. `getDropdown()` on a text field), and invalid options/states throw before save.

## Typed filling (recommended workflow)

Generate a types module from the PDF, then pass it to `getForm` for compile-time safety — unknown field names, wrong-type access, and invalid option/state values become **compile errors**, at zero runtime cost:

```bash
npx better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
```

```ts
import { myFormFields } from "./form-types.js";          // generated `…Fields` const

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```

The pure generator is also importable: `import { generateFormTypes } from "better-pdf/typegen"` (WASM-free, tree-shakeable).

## Flattening

```ts
form.flattenField("beneficiario.apellidos_nombres"); // one field → page graphics
form.flatten();                                       // all fields
await doc.save();
```

Flattened fields are stamped onto the page and removed from the AcroForm (no longer editable).

## API reference

| Call | Purpose |
|------|---------|
| `PdfDocument.load(bytes)` → `Promise<PdfDocument>` | Load an existing PDF |
| `doc.save()` → `Promise<Uint8Array>` | Apply queued fills+flattens (incremental) |
| `doc.getForm()` / `doc.getForm<typeof schema>()` | Untyped / type-narrowed form view |
| `form.getFields()` / `form.getField(name)` | `FieldInfo[]` / one `FieldInfo` |
| `form.getTextField(name).setText(v)` | Set text |
| `form.getCheckBox(name).check()` / `.uncheck()` | Toggle using real on-state |
| `form.getRadioGroup(name).select(v)` | Select by real export value |
| `form.getDropdown(name).select(v)` | Select by real option value |
| `form.getSignature(name).setImage(bytes)` | Embed visual signature (JPEG/PNG) |
| `form.flattenField(name)` / `form.flatten()` | Flatten one / all fields |
| `generateFormTypes(fields, { typeName })` | Emit a typed `…Fields` module (string) |

`FieldInfo = { name, type, value, states, options, readOnly, required, widgets }`, where `widgets: { page, rect: [x0,y0,x1,y1] }[]` gives each widget's 0-based page index and `/Rect` in PDF points (origin bottom-left). `type` ∈ `text | checkbox | radio | dropdown | listbox | signature | pushbutton | unknown`.

## Browser

Import from `better-pdf/browser` (initializes a web-target WASM build); same API.
