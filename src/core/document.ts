import { PdfForm } from "../forms/form.js";
import { toPdfError, PageOutOfRangeError, PdfError, toInvalidImageError } from "./errors.js";
import type { FormSchema, TypedPdfForm } from "../forms/schema.js";
import { PdfPage } from "../generate/page.js";
import { DrawQueue } from "../generate/draw-queue.js";
import { type PageSize, PageSizes } from "../generate/page-sizes.js";
import { PdfImage } from "../generate/image.js";
import { EmbeddedPdfPage } from "../generate/embedded-page.js";
import { PdfFont } from "../generate/font.js";
import { StandardFonts } from "../generate/fonts.js";
import { FormBuilder } from "../generate/form-builder.js";
import type { FieldDef } from "../generate/form-builder.js";
import { toPdfDate, fromPdfDate, type DocumentMetadata } from "../generate/metadata.js";

/** WASM bindings a PdfDocument needs; satisfied by both wasm.ts and wasm-browser.ts. @internal */
export interface CoreWasm {
  readFields(data: Uint8Array): string;
  fillFields(data: Uint8Array, opsJson: string, images: Uint8Array): Uint8Array;
  flattenFields(data: Uint8Array, namesJson: string): Uint8Array;
  readPages(data: Uint8Array): string;
  applyDrawOps(
    data: Uint8Array,
    opsJson: string,
    images: Uint8Array,
    fonts?: Uint8Array,
    fontsJson?: string,
  ): Uint8Array;
  createDocument(
    opsJson: string,
    images?: Uint8Array,
    fonts?: Uint8Array,
    fontsJson?: string,
    fieldsJson?: string,
  ): Uint8Array;
  imageInfo(data: Uint8Array): string;
  measureText(font: string, size: number, text: string): number;
  measureTextEmbedded(font: Uint8Array, size: number, text: string): number;
  readMetadata(data: Uint8Array): string;
  setMetadata(data: Uint8Array, metaJson: string): Uint8Array;
  manipulatePages(docsBlob: Uint8Array, docsJson: string, planJson: string): Uint8Array;
}

export class PdfDocumentBase {
  private form?: PdfForm;
  private pages?: PdfPage[];
  private readonly createdPages: PdfPage[] = [];
  private readonly drawQueue = new DrawQueue();
  private readonly fieldDefs: FieldDef[] = [];
  private readonly fieldNames = new Set<string>();
  private metadata: Record<string, string> = {};
  private metadataDirty = false;

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
        if (this.metadataDirty) {
          this.drawQueue.pushMetadata(this.metadata);
        }
        const { opsJson, images, fonts, fontsJson } = this.drawQueue.toCreatePayload();
        return this.wasm.createDocument(
          opsJson,
          images,
          fonts,
          fontsJson,
          JSON.stringify(this.fieldDefs),
        );
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
        const { opsJson, images, fonts, fontsJson } = this.drawQueue.toDrawPayload();
        bytes = this.wasm.applyDrawOps(bytes, opsJson, images, fonts, fontsJson);
      }
      if (this.metadataDirty) {
        bytes = this.wasm.setMetadata(bytes, JSON.stringify(this.metadata));
      }
    } catch (e) {
      throw toPdfError(e);
    }
    if (bytes === this.bytes) {
      return this.bytes.slice();
    }
    return bytes;
  }

  /** Set the document title metadata. */
  setTitle(value: string): void {
    this.metadata["title"] = value;
    this.metadataDirty = true;
  }

  /** Set the document author metadata. */
  setAuthor(value: string): void {
    this.metadata["author"] = value;
    this.metadataDirty = true;
  }

  /** Set the document subject metadata. */
  setSubject(value: string): void {
    this.metadata["subject"] = value;
    this.metadataDirty = true;
  }

  /** Set the document keywords metadata. The array is joined with ", ". */
  setKeywords(values: string[]): void {
    this.metadata["keywords"] = values.join(", ");
    this.metadataDirty = true;
  }

  /** Set the document creator metadata. */
  setCreator(value: string): void {
    this.metadata["creator"] = value;
    this.metadataDirty = true;
  }

  /** Set the document producer metadata. */
  setProducer(value: string): void {
    this.metadata["producer"] = value;
    this.metadataDirty = true;
  }

  /** Set the document creation date metadata. */
  setCreationDate(date: Date): void {
    this.metadata["creationDate"] = toPdfDate(date);
    this.metadataDirty = true;
  }

  /** Set the document modification date metadata. */
  setModificationDate(date: Date): void {
    this.metadata["modDate"] = toPdfDate(date);
    this.metadataDirty = true;
  }

  /**
   * Get the document metadata.
   *
   * For loaded documents, reads metadata from the PDF and overlays any locally-set values.
   * For created documents, returns the locally-set values.
   */
  async getMetadata(): Promise<DocumentMetadata> {
    let wire: Record<string, string> = {};

    if (this.mode === "load") {
      try {
        wire = JSON.parse(this.wasm.readMetadata(this.bytes)) as Record<string, string>;
      } catch {
        wire = {};
      }
    }

    // Locally-set values win
    const merged = { ...wire, ...this.metadata };

    const result: DocumentMetadata = {};
    if (merged["title"] !== undefined) result.title = merged["title"];
    if (merged["author"] !== undefined) result.author = merged["author"];
    if (merged["subject"] !== undefined) result.subject = merged["subject"];
    if (merged["keywords"] !== undefined) {
      result.keywords = merged["keywords"].split(/,\s*/);
    }
    if (merged["creator"] !== undefined) result.creator = merged["creator"];
    if (merged["producer"] !== undefined) result.producer = merged["producer"];
    if (merged["creationDate"] !== undefined) {
      const d = fromPdfDate(merged["creationDate"]);
      if (d !== undefined) result.creationDate = d;
    }
    if (merged["modDate"] !== undefined) {
      const d = fromPdfDate(merged["modDate"]);
      if (d !== undefined) result.modDate = d;
    }
    return result;
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

  /**
   * Begin building an AcroForm on a document created with {@link PdfDocument.create}.
   *
   * Returns a {@link FormBuilder} that accumulates field definitions. The
   * builder shares state with the document; calling `save()` serializes all
   * added fields to Rust.
   *
   * @throws `PdfError` when called on a document opened with `PdfDocument.load()`.
   */
  createForm(): FormBuilder {
    if (this.mode !== "create") {
      throw new PdfError("createForm is only available on documents created with PdfDocument.create()");
    }
    return new FormBuilder(this.fieldDefs, this.fieldNames);
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

  /** Embed a JPEG image. Returns a {@link PdfImage} with intrinsic size; pass it to `page.drawImage()`. */
  async embedJpg(bytes: Uint8Array): Promise<PdfImage> {
    return this.embedImage(bytes);
  }

  /** Embed a PNG image. Returns a {@link PdfImage} with intrinsic size; pass it to `page.drawImage()`. */
  async embedPng(bytes: Uint8Array): Promise<PdfImage> {
    return this.embedImage(bytes);
  }

  private embedImage(bytes: Uint8Array): PdfImage {
    let info: { width: number; height: number };
    try {
      info = JSON.parse(this.wasm.imageInfo(bytes));
    } catch (e) {
      throw toInvalidImageError(e);
    }
    return new PdfImage(bytes, info.width, info.height);
  }

  /**
   * Embed a page from a source PDF. Returns an {@link EmbeddedPdfPage} handle
   * with the intrinsic size of the source page; pass it to `page.drawPage()`.
   *
   * The source bytes are not embedded immediately — they ride the image blob
   * channel and are embedded into the target PDF at save time.
   *
   * @param src - The raw bytes of the source PDF.
   * @param pageIndex - Zero-based index of the page to embed.
   * @returns An `EmbeddedPdfPage` handle.
   * @throws `PageOutOfRangeError` when `pageIndex` is out of range.
   * @throws `PdfError` when the source PDF cannot be parsed.
   */
  async embedPdfPage(src: Uint8Array, pageIndex: number): Promise<EmbeddedPdfPage> {
    let infos: { index: number; width: number; height: number; rotation: number }[];
    try {
      infos = JSON.parse(this.wasm.readPages(src));
    } catch (e) {
      throw toPdfError(e);
    }
    const entry = infos[pageIndex];
    if (entry === undefined) {
      throw new PageOutOfRangeError(pageIndex, infos.length);
    }
    return new EmbeddedPdfPage(src, pageIndex, entry.width, entry.height);
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

  /** Get a standard-14 font handle for measuring or drawing text. */
  getFont(font: StandardFonts): PdfFont {
    return new PdfFont(font, (f, s, t) => this.wasm.measureText(f, s, t));
  }

  /**
   * Embed a TrueType/OpenType font from raw bytes. Returns a {@link PdfFont}
   * handle you can pass to `page.drawText({ font })` to render Unicode text.
   *
   * By default the font is subset to only the glyphs actually used. Pass
   * `{ subset: false }` to embed the full font program.
   */
  async embedFont(bytes: Uint8Array, opts: { subset?: boolean } = {}): Promise<PdfFont> {
    const id = this.drawQueue.registerFont(bytes, opts.subset ?? true);
    return PdfFont.embedded(id, bytes, (b, s, t) => this.wasm.measureTextEmbedded(b, s, t));
  }

  /**
   * Extract a subset of pages into a new PDF document.
   *
   * Only available on documents opened with {@link PdfDocument.load}.
   *
   * @param indices - Zero-based page indices to include, in the order given.
   * @returns A new PDF containing only the selected pages.
   * @throws `PdfError` when called on a document created with `PdfDocument.create()`.
   */
  async copyPages(indices: number[]): Promise<Uint8Array> {
    if (this.mode !== "load") {
      throw new PdfError("copyPages is only available on documents opened with PdfDocument.load()");
    }
    const selections = indices.map((i) => ({ docIndex: 0, pageIndex: i }));
    return PdfDocumentBase.runAssemble([this.bytes], selections, this.wasm);
  }

  /**
   * Split the document into individual single-page PDFs.
   *
   * Only available on documents opened with {@link PdfDocument.load}.
   *
   * @returns An array of PDF byte arrays, one per page, in document order.
   * @throws `PdfError` when called on a document created with `PdfDocument.create()`.
   */
  async splitPages(): Promise<Uint8Array[]> {
    if (this.mode !== "load") {
      throw new PdfError(
        "splitPages is only available on documents opened with PdfDocument.load()",
      );
    }
    const count = this.getPageCount();
    const results: Uint8Array[] = [];
    for (let i = 0; i < count; i++) {
      results.push(
        await PdfDocumentBase.runAssemble(
          [this.bytes],
          [{ docIndex: 0, pageIndex: i }],
          this.wasm,
        ),
      );
    }
    return results;
  }

  /**
   * @internal
   * Concatenate docs into a blob+table, call wasm.manipulatePages, return result.
   */
  protected static runAssemble(
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
    wasmBinding: CoreWasm,
  ): Uint8Array {
    // Build a single concatenated blob and an offset/length table
    let totalLength = 0;
    const table: { offset: number; length: number }[] = [];
    for (const doc of docs) {
      table.push({ offset: totalLength, length: doc.length });
      totalLength += doc.length;
    }
    const blob = new Uint8Array(totalLength);
    for (let i = 0; i < docs.length; i++) {
      blob.set(docs[i]!, table[i]!.offset);
    }
    const docsJson = JSON.stringify(table);
    const planJson = JSON.stringify(
      selections.map((s) => ({ doc: s.docIndex, page: s.pageIndex })),
    );
    try {
      return wasmBinding.manipulatePages(blob, docsJson, planJson);
    } catch (e) {
      throw toPdfError(e);
    }
  }
}
