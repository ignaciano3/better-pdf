---
title: Typed forms
description: Generate a schema module so field names and values are compile-checked.
---

Generate a TypeScript module from an existing PDF, then field names, types, and
option values become compile errors when wrong — at zero runtime cost (the
schema is referenced only via `typeof`).

## Generate the module

```bash
better-pdf-generate-types form.pdf src/form-types.ts --name EnrollmentForm
```

Encrypted PDFs need a password — pass `--password s3cret`, or `--password ''`
for owner-locked files that open without a user password.

The generated module exports field-name unions and literal schema metadata for
every field: type, dropdown/listbox options, radio states, the
read-only/required/exported/multi-select flags, text flags (`password`,
`multiline`, `comb`, `maxLength`), editable combo boxes, alignment, tooltip, the
`/DA` font name and size, the author-declared default (`defaultValue`, i.e.
`/DV`), and the page indices the field's widgets sit on. That makes it a
standalone description of the form — useful for inspecting a PDF's fields even
without using the rest of this library.

```ts
myFormFields["applicant.name"].pages;     // readonly [0]
myFormFields["applicant.name"].maxLength; // 40
```

It does not carry each field's current value (`/V`) by default, so generating
types from a filled form never bakes answers (or other PII) into source control.
Read values at runtime with `form.getFields()` — or opt them in with
`--include-values` on the CLI (`includeValues: true` in the API) when the input
is a blank or reference form and a snapshot of its contents is what you want.

## Use it

```ts
import { myFormFields } from "./form-types.js";

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```

Unknown field names, wrong-type access, and invalid option/state values become
compile errors. The untyped `doc.getForm()` keeps working unchanged.
