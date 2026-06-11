# Migrating from pdf-lib

`@ignaciano3/better-pdf` covers pdf-lib's form-filling workflow with a faster
core and stricter validation. This guide maps the APIs.

## API mapping

| pdf-lib | better-pdf |
| --- | --- |
| `PDFDocument.load(bytes)` | `PdfDocument.load(bytes)` |
| `pdfDoc.getForm()` | `doc.getForm()` |
| `form.getFields()` | `form.getFields()` → plain `FieldInfo[]` (name/type/value/states/options/…) |
| `form.getTextField(n).setText(v)` | `form.getTextField(n).setText(v)` |
| `form.getCheckBox(n).check()` / `.uncheck()` | same — uses the field's real on-state automatically |
| `form.getRadioGroup(n).select(v)` | same — `v` must be a real export value (`field.states`) |
| `form.getDropdown(n).select(v)` | same — `v` must be a real option (`field.options`) |
| `form.getOptionList(n).select(v)` | `form.getListBox(n).select(v)` (single-select) |
| `field.acroField.getWidgets()` | `field` info's `widgets` (`{page, rect}` per widget) |
| `form.flatten()` | `form.flatten()` (or `form.flattenField(name)`) |
| `pdfDoc.save()` | `doc.save()` — **incremental, append-only** |
| `form.updateFieldAppearances()` | not needed — appearances are generated on fill |

## Semantic differences

- **Saves are incremental.** Output begins with the original bytes verbatim and
  appends an update section. `save()` always starts from the loaded bytes, so
  calling it twice yields the same result.
- **Validation is strict and typed.** Unknown fields, wrong-type access, invalid
  options/states, over-`maxLength` text, and core rejections (XFA, CMYK JPEGs)
  throw `PdfError` subclasses instead of writing garbage.
- **Appearances are always generated** (pdf-lib often leaves `/NeedAppearances`
  rendering to the viewer); flattening therefore works on PDFs where pdf-lib
  throws `Unexpected N type: undefined`.
- **Signatures are visual only** — image appearances, not cryptographic signing.
- **Scope:** existing PDFs only. No document creation, page drawing, or
  encryption — if you need those pdf-lib features, keep pdf-lib alongside.

## Typed forms (no pdf-lib equivalent)

Generate a schema module once, then field names, types, and option values are
compile-checked:

```bash
npx better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
```

```ts
const form = doc.getForm<typeof enrollmentFormFields>();
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```
