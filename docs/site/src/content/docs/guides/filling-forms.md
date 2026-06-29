---
title: Filling & flattening forms
description: Inspect, fill, and flatten AcroForm fields on an existing PDF.
---

## Encrypted PDFs

`PdfDocument.load` decrypts encrypted PDFs (RC4 / AES-128 / AES-256) when you pass
a `password`. Use `""` for owner-locked files (an empty user password):

```ts
const ownerLocked = await PdfDocument.load(bytes, { password: "" });
const protected_ = await PdfDocument.load(bytes, { password: "secret" });
```

Decryption is opt-in: bare `load(bytes)` does not decrypt, so an encrypted file
loaded without a `password` throws `EncryptedPdfError` (pass a password). A wrong
password throws `IncorrectPasswordError`. Saving an edited encrypted PDF produces
a **decrypted** (unencrypted) output.

## Inspect fields

`form.getFields()` returns plain `FieldInfo[]`:

```ts
const doc = await PdfDocument.load(input);
const form = doc.getForm();

for (const field of form.getFields()) {
  console.log(field.name, field.type, field.value, field.options);
}
```

Each `FieldInfo` carries `name`, `type`, `value`, `defaultValue` (the `/DV`
reset value, or `null`), `states`, `options`, `readOnly`, `required`, `exported`
(false when the field has the `NoExport` flag), `maxLength` (a text field's
`/MaxLen`, or `null`), `multiline` (text-area fields), `password` (masked text
fields), `comb` (fixed-pitch per-character text fields), `editable` (combo boxes
that accept custom values), `align` (`"left"`/`"center"`/`"right"`, from `/Q`),
`tooltip` (the `/TU` descriptive name, or `null`), `fontName` / `fontSize` (the
effective `/DA` font resource name and size for variable-text fields, else
`null`), `multiSelect` (multi-select list boxes), and `widgets` — one entry per
widget annotation giving its 0-based `page` index, `rect` (`[x0, y0, x1, y1]`
in PDF points, origin bottom-left), and the annotation visibility flags
`hidden` / `print` / `noView` (from `/F`).

## Fill

```ts
form.getTextField("name").setText("GARCIA, IGNACIO");
form.getCheckBox("agree").check();           // uses the field's real on-state
form.getCheckBox("agree").uncheck();
form.getRadioGroup("kind").select("Titular"); // must be a real export value
form.getDropdown("status").select("Casado");  // must be a real option
form.getListBox("plan").select("basic");
form.getListBox("plan").selectMultiple(["basic", "pro"]); // multi-select list boxes only
```

:::caution[Use real values]
`select()` and `check()` use the field's *actual* export values. Read them from
`field.states` (radios/checkboxes) and `field.options` (dropdowns/listboxes) —
never assume `Yes`/`On`. Invalid values throw (see [Errors](/better-pdf/reference/errors/)).
:::

`setText()` throws `MaxLengthExceededError` if the value exceeds the field's
`/MaxLen`.

### Default (reset) value

Each field can carry a default value (`/DV`) — what a viewer's "reset form"
restores. It is independent of the current value and set with a parallel set of
methods. Their arguments validate exactly like `select`/`check`/`setText`:

```ts
form.getTextField("name").setDefaultText("");        // empty by default
form.getCheckBox("agree").setDefaultChecked(false);
form.getRadioGroup("kind").setDefaultSelected("Titular");
form.getDropdown("status").setDefaultSelected("Casado");
form.getListBox("plan").setDefaultSelected("basic");
```

### Reset

`reset()` restores fields to their default value (`/DV`), or clears them when
they have none — the equivalent of a viewer's "reset form". Signature and
push-button fields are skipped by `reset()`.

```ts
form.resetField("status");  // reset one field
form.reset();               // reset every value-bearing field
```

### Flags and visibility

Beyond their value, fields expose setters that change their **flags** on a
loaded document. They apply to every field type and take effect on `save()`:

```ts
const field = form.getTextField("name");

field.setReadOnly(true);   // /Ff ReadOnly — displayed but not editable
field.setRequired(true);   // /Ff Required — viewers may block submit while empty
field.setExported(false);  // /Ff NoExport — exclude from form submission

field.hide();              // /F Hidden  — hide on screen and in print
field.show();              // clear Hidden
field.setPrintable(true);  // /F Print   — include in printed output
field.setNoView(true);     // /F NoView  — hide on screen but still printable
```

`setReadOnly` / `setRequired` / `setExported` flip the field-level `/Ff` flags,
while `hide` / `show` / `setPrintable` / `setNoView` flip the `/F` flags on each
of the field's widget annotations. The change is reflected on the field's
`FieldInfo` (and `FieldInfo.widgets`) immediately and written to the PDF on
`save()`.

### Appearance-affecting text-field flags

`multiline`, `comb`, and `password` change how a text field's value is drawn, so
toggling them on a loaded field **regenerates the field's appearance** from its
current value. They are exposed on `PdfTextField` (text fields only):

```ts
const field = form.getTextField("notes");

field.setMultiline(true);   // /Ff Multiline — wrap and top-align the value
field.setComb(true, 8);     // /Ff Comb — 8 fixed-pitch cells (writes /MaxLen)
field.setComb(false);       // clear Comb
field.setPassword(true);    // /Ff Password — draw an empty appearance (value kept)
```

Enabling `comb` requires a cell count; the `setComb` overload makes `maxLen`
mandatory when the first argument is `true`. A `password` field keeps its `/V`
value but never renders it into the appearance stream.

## Flatten

```ts
form.flattenField("name");  // flatten one field
form.flatten();             // flatten all fields
```

Appearances are generated on fill, so flattening works on PDFs where pdf-lib
throws `Unexpected N type: undefined`.

## Save

```ts
const output = await doc.save();   // Promise<Uint8Array>
await Bun.write("filled.pdf", output);
```

`save()` applies queued fills first, then queued flattens. It always starts from
the originally loaded bytes (calling it twice returns the same result), and
`FieldInfo.value` reflects queued mutations as soon as they are made.
