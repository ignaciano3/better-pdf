import * as wasm from "./core/wasm.js";
import { PdfDocumentBase } from "./core/document.js";

/**
 * Represents a loaded PDF document.
 *
 * `PdfDocument` is the entry point for reading AcroForm fields, queuing field
 * changes, flattening fields, and saving the result as PDF bytes.
 *
 * @example
 * ```ts
 * import { PdfDocument } from "@ignaciano3/better-pdf";
 *
 * const input = await fetch("form.pdf").then((res) => res.arrayBuffer());
 * const doc = await PdfDocument.load(input);
 * const form = doc.getForm();
 *
 * form.getTextField("person.name").setText("Ada Lovelace");
 * form.getCheckBox("person.accepted").check();
 *
 * const output = await doc.save();
 * ```
 */
export class PdfDocument extends PdfDocumentBase {
  /**
   * Load a PDF document from raw bytes.
   *
   * This method accepts either a `Uint8Array` or an `ArrayBuffer`. It is async
   * so the same code works in Node.js and browser builds.
   *
   * @param input - The bytes of an existing PDF file.
   * @returns A loaded `PdfDocument`.
   *
   * @example
   * ```ts
   * const bytes = new Uint8Array(await Bun.file("form.pdf").arrayBuffer());
   * const doc = await PdfDocument.load(bytes);
   * ```
   */
  static async load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument> {
    const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
    return new PdfDocument(bytes, wasm);
  }

  /** Create a new, empty document. Add pages with {@link PdfDocument.addPage}. */
  static async create(): Promise<PdfDocument> {
    return new PdfDocument(new Uint8Array(), wasm, "create");
  }
}

export { PdfPage } from "./generate/page.js";
export type { DrawTextOptions, DrawImageOptions, DrawLineOptions, DrawRectangleOptions, DrawEllipseOptions } from "./generate/page.js";
export { PdfFont } from "./generate/font.js";
export { PdfImage } from "./generate/image.js";
export { PageSizes } from "./generate/page-sizes.js";
export type { PageSize } from "./generate/page-sizes.js";
export { StandardFonts } from "./generate/fonts.js";
export { rgb, grayscale } from "./generate/color.js";
export type { Color } from "./generate/color.js";
export { PageOutOfRangeError } from "./core/errors.js";
export { PdfForm } from "./forms/form.js";
export type { FieldInfo, FieldType, FieldWidget } from "./forms/form.js";
export {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfListBox,
  PdfSignature,
} from "./forms/fields.js";
export {
  PdfError,
  UnknownFieldError,
  FieldTypeError,
  InvalidOptionError,
  MaxLengthExceededError,
  MissingOnStateError,
  PdfCoreError,
  InvalidImageError,
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
export type { TextFieldOptions, CheckBoxOptions, RadioGroupOptions, RadioOption, ChoiceOptions, SignatureFieldOptions, FieldBorder } from "./generate/form-builder.js";
