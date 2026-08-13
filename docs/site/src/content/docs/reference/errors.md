---
title: Errors
description: The PdfError family and when each is thrown.
---

Every error thrown by the library subclasses `PdfError`, so you can catch the
whole family or a specific case.

| Error | When | Fields |
| --- | --- | --- |
| `UnknownFieldError` | no field with that name | `.field` |
| `FieldTypeError` | field accessed as the wrong type (e.g. `getDropdown()` on a text field) | `.field`, `.actual`, `.expected` |
| `InvalidOptionError` | selecting a value not in the field's options | `.field`, `.fieldType`, `.value`, `.options` |
| `MaxLengthExceededError` | `setText()` value longer than the field's `/MaxLen` | `.field`, `.maxLength`, `.actualLength` |
| `MissingOnStateError` | checking a checkbox with no declared on-state | `.field` |
| `PdfCoreError` | an operation the core rejected at `save()` (XFA forms, unsupported images, malformed PDFs); the core's message is preserved | — |
| `EncryptedPdfError` | loading an encrypted PDF without a `password` (`load(bytes)` does not decrypt) | — |
| `IncorrectPasswordError` | the password passed to `load(bytes, { password })` is wrong or missing | — |
| `PageOutOfRangeError` | `getPage(i)` called with an index outside `[0, pageCount)` | — |
| `InvalidImageError` | `embedJpg`/`embedPng` rejected the image bytes (unsupported format or CMYK JPEG) | — |

```ts
import { FieldTypeError } from "@ignaciano3/better-pdf";

try {
  form.getDropdown("some.text.field");
} catch (e) {
  if (e instanceof FieldTypeError) console.log(e.actual, e.expected);
}
```
