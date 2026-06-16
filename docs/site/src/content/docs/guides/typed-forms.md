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

The generated module exports field-name unions and literal metadata for field
types, dropdown/listbox options, radio states, read-only flags, and current
values.

## Use it

```ts
import { myFormFields } from "./form-types.js";

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
```

Unknown field names, wrong-type access, and invalid option/state values become
compile errors. The untyped `doc.getForm()` keeps working unchanged.
