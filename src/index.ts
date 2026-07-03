/**
 * @public
 *
 * Root barrel for `@ignaciano3/better-pdf`. All exports from this file
 * constitute the stable public API as of 1.0.0. Symbols tagged `@internal`
 * are excluded from the stability guarantee regardless of where they appear.
 *
 * See [docs/STABILITY.md](../docs/STABILITY.md) for the full semver and
 * deprecation policy.
 */

import * as wasm from "./core/wasm.js";
import { PdfDocumentBase } from "./core/document.js";
import type { ManipulateOptions } from "./core/document.js";

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
  static async load(
    input: Uint8Array | ArrayBuffer,
    opts?: { password?: string },
  ): Promise<PdfDocument> {
    return new PdfDocument(PdfDocumentBase.loadBytes(wasm, input, opts), wasm);
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
    options?: ManipulateOptions,
  ): Promise<Uint8Array> {
    return PdfDocumentBase.assembleImpl(wasm, docs, selections, options?.objectStreams ?? false);
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
  static async merge(docs: Uint8Array[], options?: ManipulateOptions): Promise<Uint8Array> {
    return PdfDocumentBase.mergeImpl(wasm, docs, options?.objectStreams ?? false);
  }
}

export * from "./exports-common.js";
