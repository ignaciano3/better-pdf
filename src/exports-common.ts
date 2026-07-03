/**
 * @public
 *
 * Shared public exports for both the Node (`./index.ts`) and browser
 * (`./index.browser.ts`) barrels. Each barrel re-exports everything here plus
 * its own runtime-specific `PdfDocument` class (and, in the browser build,
 * `initializeWasm`). Keeping the list in one place prevents the two entry
 * points from drifting.
 */

export { PdfPage } from "./generate/page.js";
export type {
  DrawTextOptions,
  DrawImageOptions,
  DrawPageOptions,
  DrawLineOptions,
  DrawRectangleOptions,
  DrawEllipseOptions,
  DrawLinkOptions,
  DrawSvgPathOptions,
  DrawPolygonOptions,
} from "./generate/page.js";
export { PdfFont } from "./generate/font.js";
export { PdfImage } from "./generate/image.js";
export { EmbeddedPdfPage } from "./generate/embedded-page.js";
export type { DocumentMetadata } from "./generate/metadata.js";
export { PageSizes } from "./generate/page-sizes.js";
export type { PageSize } from "./generate/page-sizes.js";
export { StandardFonts } from "./generate/fonts.js";
export { rgb, grayscale } from "./generate/color.js";
export type { Color } from "./generate/color.js";
export { PageOutOfRangeError } from "./core/errors.js";
export type { SaveOptions } from "./core/document.js";
export type { ManipulateOptions } from "./core/document.js";
export { PdfForm } from "./forms/form.js";
export type { FieldInfo, FieldType, FieldWidget } from "./forms/form.js";
export {
  PdfField,
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfListBox,
  PdfSignature,
} from "./forms/fields.js";
export type { FieldFlagChanges } from "./forms/fields.js";
export {
  PdfError,
  FormSealedError,
  UnknownFieldError,
  FieldTypeError,
  InvalidOptionError,
  MaxLengthExceededError,
  MissingOnStateError,
  MultiSelectError,
  PdfCoreError,
  EncryptedPdfError,
  IncorrectPasswordError,
  InvalidImageError,
  InvalidRotationError,
} from "./core/errors.js";
export { generateFormTypes } from "./forms/typegen.js";
export type { GenerateFormTypesOptions } from "./forms/typegen.js";
export type {
  FieldMeta,
  FormSchema,
  FieldNameOf,
  NameOfType,
  OptionsOf,
  TypedPdfForm,
} from "./forms/schema.js";
export { FormBuilder } from "./generate/form-builder.js";
export type {
  TextFieldOptions,
  CheckBoxOptions,
  RadioGroupOptions,
  RadioOption,
  ChoiceOptions,
  SignatureFieldOptions,
  FieldBorder,
  FieldAlign,
  CheckStyle,
} from "./generate/form-builder.js";
export type { OutlineItem } from "./generate/outline.js";
