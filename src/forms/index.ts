// Runtime-neutral subpath entry: the AcroForm API without PdfDocument or any
// WASM import, so it loads identically under Node and browser bundlers.
// PdfDocument comes from the package root (or /browser) entry.
export { PdfForm } from "./form.js";
export type { FieldInfo, FieldType, FieldWidget } from "./form.js";
export {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfListBox,
  PdfSignature,
} from "./fields.js";
export {
  PdfError,
  UnknownFieldError,
  FieldTypeError,
  InvalidOptionError,
  MaxLengthExceededError,
  MissingOnStateError,
  PdfCoreError,
} from "../core/errors.js";
export { generateFormTypes } from "./typegen.js";
export type { GenerateFormTypesOptions } from "./typegen.js";
export type {
  FieldMeta,
  FormSchema,
  FieldNameOf,
  NameOfType,
  OptionsOf,
  TypedPdfForm,
} from "./schema.js";
