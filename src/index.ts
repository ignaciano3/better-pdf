import * as wasm from "./core/wasm.js";
import { PdfDocumentBase } from "./core/document.js";
import { toPdfError } from "./core/errors.js";

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

  /**
   * Assemble a new PDF from an ordered selection of pages across one or more source documents.
   *
   * @param docs - Array of source PDF byte arrays.
   * @param selections - Ordered list of `{docIndex, pageIndex}` entries describing
   *   which page from which document to include. Indices are zero-based.
   * @returns A new PDF containing only the selected pages in the given order.
   *
   * @example
   * ```ts
   * // Take page 0 of doc A, then page 2 of doc B
   * const out = await PdfDocument.assemble([docA, docB], [
   *   { docIndex: 0, pageIndex: 0 },
   *   { docIndex: 1, pageIndex: 2 },
   * ]);
   * ```
   */
  static async assemble(
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
  ): Promise<Uint8Array> {
    try {
      return PdfDocumentBase.runAssemble(docs, selections, wasm);
    } catch (e) {
      throw toPdfError(e);
    }
  }

  /**
   * Merge multiple PDF documents into a single PDF, preserving all pages in order.
   *
   * @param docs - Array of source PDF byte arrays to merge.
   * @returns A new PDF containing all pages from all source documents in order.
   *
   * @example
   * ```ts
   * const merged = await PdfDocument.merge([docA, docB, docC]);
   * ```
   */
  static async merge(docs: Uint8Array[]): Promise<Uint8Array> {
    const selections: { docIndex: number; pageIndex: number }[] = [];
    for (let docIndex = 0; docIndex < docs.length; docIndex++) {
      let pageInfos: { index: number }[];
      try {
        pageInfos = JSON.parse(wasm.readPages(docs[docIndex]!)) as { index: number }[];
      } catch (e) {
        throw toPdfError(e);
      }
      for (let pageIndex = 0; pageIndex < pageInfos.length; pageIndex++) {
        selections.push({ docIndex, pageIndex });
      }
    }
    try {
      return PdfDocumentBase.runAssemble(docs, selections, wasm);
    } catch (e) {
      throw toPdfError(e);
    }
  }
}

export { PdfPage } from "./generate/page.js";
export type { DrawTextOptions, DrawImageOptions, DrawLineOptions, DrawRectangleOptions, DrawEllipseOptions } from "./generate/page.js";
export { PdfFont } from "./generate/font.js";
export { PdfImage } from "./generate/image.js";
export type { DocumentMetadata } from "./generate/metadata.js";
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
export type { TextFieldOptions, CheckBoxOptions, RadioGroupOptions, RadioOption, ChoiceOptions, SignatureFieldOptions, FieldBorder } from "./generate/form-builder.js";
