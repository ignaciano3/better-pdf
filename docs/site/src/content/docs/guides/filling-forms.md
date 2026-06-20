---
title: Filling & flattening forms
description: Inspect, fill, and flatten AcroForm fields on an existing PDF.
---

## Inspect fields

`form.getFields()` returns plain `FieldInfo[]`:

```ts
const doc = await PdfDocument.load(input);
const form = doc.getForm();

for (const field of form.getFields()) {
  console.log(field.name, field.type, field.value, field.options);
}
```

Each `FieldInfo` carries `name`, `type`, `value`, `states`, `options`,
`readOnly`, `required`, `exported` (false when the field has the `NoExport`
flag), `maxLength` (a text field's `/MaxLen`, or `null`), and `widgets` — one
entry per widget annotation giving its 0-based `page` index and `rect`
(`[x0, y0, x1, y1]` in PDF points, origin bottom-left).

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
