/**
 * @public
 *
 * Browser barrel for `@ignaciano3/better-pdf/browser`. All exports from this
 * file constitute the stable public API as of 1.0.0. Symbols tagged
 * `@internal` are excluded from the stability guarantee regardless of where
 * they appear.
 *
 * See [docs/STABILITY.md](../docs/STABILITY.md) for the full semver and
 * deprecation policy.
 */

import * as wasm from "./core/wasm-browser.js";
import { initializeWasm } from "./core/wasm-browser.js";
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
 * import { PdfDocument } from "@ignaciano3/better-pdf/browser";
 *
 * const input = await file.arrayBuffer();
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
   * This method accepts either a `Uint8Array` or an `ArrayBuffer`. In the
   * browser build, it also initializes the WASM module on first use.
   *
   * @param input - The bytes of an existing PDF file.
   * @returns A loaded `PdfDocument`.
   *
   * @example
   * ```ts
   * const bytes = await file.arrayBuffer();
   * const doc = await PdfDocument.load(bytes);
   * ```
   */
  static async load(
    input: Uint8Array | ArrayBuffer,
    opts?: { password?: string },
  ): Promise<PdfDocument> {
    await initializeWasm();
    return new PdfDocument(PdfDocumentBase.loadBytes(wasm, input, opts), wasm);
  }

  /**
   * Report whether a PDF is encrypted, without decrypting it or needing a
   * password. See {@link PdfDocument.isEncrypted} in the Node build.
   */
  static async isEncrypted(input: Uint8Array | ArrayBuffer): Promise<boolean> {
    await initializeWasm();
    return PdfDocumentBase.isEncryptedImpl(wasm, input);
  }

  /**
   * Classify how a password authorizes an encrypted PDF (`"owner"` / `"user"` /
   * `null`). See {@link PdfDocument.passwordType} in the Node build.
   */
  static async passwordType(
    input: Uint8Array | ArrayBuffer,
    password: string,
  ): Promise<"owner" | "user" | null> {
    await initializeWasm();
    return PdfDocumentBase.passwordTypeImpl(wasm, input, password);
  }

  /** Create a new, empty document. Add pages with {@link PdfDocument.addPage}. */
  static async create(): Promise<PdfDocument> {
    await initializeWasm();
    return new PdfDocument(new Uint8Array(), wasm, "create");
  }

  /**
   * Assemble a new PDF from an ordered selection of pages across one or more
   * source documents. See {@link PdfDocument.assemble} in the Node build.
   *
   * @param options - Optional; `{ objectStreams?: boolean }` packs object streams
   *   for smaller output (default false).
   */
  static async assemble(
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
    options?: ManipulateOptions,
  ): Promise<Uint8Array> {
    await initializeWasm();
    return PdfDocumentBase.assembleImpl(wasm, docs, selections, options?.objectStreams ?? false);
  }

  /**
   * Merge multiple PDFs into one, preserving all pages in order.
   *
   * @param options - Optional; `{ objectStreams?: boolean }` packs object streams
   *   for smaller output (default false).
   */
  static async merge(docs: Uint8Array[], options?: ManipulateOptions): Promise<Uint8Array> {
    await initializeWasm();
    return PdfDocumentBase.mergeImpl(wasm, docs, options?.objectStreams ?? false);
  }
}

export * from "./exports-common.js";
export { initializeWasm } from "./core/wasm-browser.js";
