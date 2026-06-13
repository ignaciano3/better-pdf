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
}

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
