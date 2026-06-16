---
title: Quickstart
description: Load a form, fill some fields, sign, flatten, and save.
---

Load an existing PDF, inspect its fields, fill some, drop in a signature image,
flatten a field, and save.

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

## Key facts

- **`save()` is an incremental update.** Output begins with the original bytes
  verbatim and appends an update section. It always starts from the loaded bytes,
  so calling it twice yields the same result. With no queued operations it
  returns a byte-identical round trip.
- **Use a field's *real* export values.** Never assume `Yes`/`On` — read
  `field.states` / `field.options`.
- **Visual signatures are appearances only.** They do not create
  cryptographic/PAdES signatures.

Next: [filling & flattening](/better-pdf/guides/filling-forms/),
[generating documents](/better-pdf/guides/generating/), or the
[API reference](/better-pdf/reference/api/).
