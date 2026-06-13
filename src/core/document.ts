import { PdfForm } from "../forms/form.js";
import { toPdfError, PageOutOfRangeError, PdfError } from "./errors.js";
import type { FormSchema, TypedPdfForm } from "../forms/schema.js";
import { PdfPage } from "../generate/page.js";
import { DrawQueue } from "../generate/draw-queue.js";
import { type PageSize, PageSizes } from "../generate/page-sizes.js";

/** WASM bindings a PdfDocument needs; satisfied by both wasm.ts and wasm-browser.ts. @internal */
export interface CoreWasm {
  readFields(data: Uint8Array): string;
  fillFields(data: Uint8Array, opsJson: string, images: Uint8Array): Uint8Array;
  flattenFields(data: Uint8Array, namesJson: string): Uint8Array;
  readPages(data: Uint8Array): string;
  applyDrawOps(data: Uint8Array, opsJson: string): Uint8Array;
  createDocument(opsJson: string): Uint8Array;
}

export class PdfDocumentBase {
  private form?: PdfForm;
  private pages?: PdfPage[];
  private readonly createdPages: PdfPage[] = [];
  private readonly drawQueue = new DrawQueue();

  /** @internal */
  protected constructor(
    protected readonly bytes: Uint8Array,
    private readonly wasm: CoreWasm,
    private readonly mode: "load" | "create" = "load",
  ) {}

  /**
   * Save the document and return the resulting PDF bytes.
   *
   * Queued field fills are applied first, then queued field flattens. If no
   * changes were queued, this returns a copy of the original PDF bytes.
   *
   * Calling `save()` does not mutate the original loaded bytes. Calling it
   * again with the same queued operations returns the same PDF output.
   *
   * @returns The saved PDF bytes.
   * @throws `PdfCoreError` when the PDF core rejects an operation, such as an
   * unsupported image, XFA form, or malformed PDF.
   *
   * @example
   * ```ts
   * form.getTextField("invoice.total").setText("$42.00");
   * form.flattenField("invoice.total");
   *
   * const pdfBytes = await doc.save();
   * await Bun.write("filled.pdf", pdfBytes);
   * ```
   */
  async save(): Promise<Uint8Array> {
    if (this.mode === "create") {
      try {
        return this.wasm.createDocument(this.drawQueue.toCreateJson());
      } catch (e) {
        throw toPdfError(e);
      }
    }

    const form = this.form;
    let bytes = this.bytes;
    try {
      if (form && form.queue.length > 0) {
        const { opsJson, images } = form.queue.toPayload();
        bytes = this.wasm.fillFields(bytes, opsJson, images);
      }
      if (form && form.flattenQueue.length > 0) {
        bytes = this.wasm.flattenFields(bytes, JSON.stringify(form.flattenQueue));
      }
      if (this.drawQueue.length > 0) {
        bytes = this.wasm.applyDrawOps(bytes, this.drawQueue.toJson());
      }
    } catch (e) {
      throw toPdfError(e);
    }
    if (bytes === this.bytes) {
      return this.bytes.slice();
    }
    return bytes;
  }

  /** Number of pages in the document. */
  getPageCount(): number {
    return this.mode === "create" ? this.createdPages.length : this.loadPages().length;
  }

  /** All pages, in document order. The same instances are returned every time. */
  getPages(): PdfPage[] {
    return this.mode === "create" ? [...this.createdPages] : [...this.loadPages()];
  }

  /** Get one page by zero-based index. */
  getPage(index: number): PdfPage {
    const pages = this.mode === "create" ? this.createdPages : this.loadPages();
    const page = pages[index];
    if (page === undefined) throw new PageOutOfRangeError(index, pages.length);
    return page;
  }

  /** Append a page to a document created with {@link PdfDocument.create}. Size defaults to A4. */
  addPage(size: PageSize = PageSizes.A4): PdfPage {
    if (this.mode !== "create") {
      throw new PdfError("addPage is only available on documents created with PdfDocument.create()");
    }
    const [width, height] = size;
    const index = this.createdPages.length;
    this.drawQueue.pushAddPage(width, height);
    const page = new PdfPage(index, width, height, 0, this.drawQueue);
    this.createdPages.push(page);
    return page;
  }

  private loadPages(): PdfPage[] {
    if (!this.pages) {
      let infos: { index: number; width: number; height: number; rotation: number }[];
      try {
        infos = JSON.parse(this.wasm.readPages(this.bytes));
      } catch (e) {
        throw toPdfError(e);
      }
      this.pages = infos.map(
        (p) => new PdfPage(p.index, p.width, p.height, p.rotation, this.drawQueue),
      );
    }
    return this.pages;
  }

  /**
   * Get the document's AcroForm.
   *
   * The same `PdfForm` instance is returned every time. Field changes are queued
   * on the form and applied when you call `doc.save()`.
   *
   * @returns The document's form API.
   *
   * @example
   * ```ts
   * const form = doc.getForm();
   * const fields = form.getFields();
   *
   * for (const field of fields) {
   *   console.log(field.name, field.type, field.value);
   * }
   * ```
   */
  getForm(): PdfForm;
  /**
   * Get a compile-time typed view of the document's AcroForm.
   *
   * Pass a generated field metadata object as the type argument. The runtime
   * object is the same `PdfForm`; the schema is used only by TypeScript to catch
   * unknown field names, wrong field accessors, and invalid choice values.
   *
   * @returns The document's form API narrowed by the generated schema.
   *
   * @example
   * ```ts
   * import { enrollmentFormFields } from "./form-types.js";
   *
   * const form = doc.getForm<typeof enrollmentFormFields>();
   * form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
   * form.getDropdown("beneficiario.estado_civil").select("Casado");
   * ```
   */
  getForm<S extends FormSchema>(): TypedPdfForm<S>;
  getForm(): PdfForm {
    if (this.mode === "create") {
      throw new PdfError(
        "getForm is not available on documents created with PdfDocument.create(); creating AcroForm fields is not supported",
      );
    }
    if (!this.form) this.form = new PdfForm(this.bytes, this.wasm.readFields);
    return this.form;
  }
}
