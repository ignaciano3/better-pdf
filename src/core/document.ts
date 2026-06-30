import { PdfForm, kFormQueue, kFlattenQueue } from "../forms/form.js";
import { toPdfError, PageOutOfRangeError, PdfError, toInvalidImageError, EncryptedPdfError } from "./errors.js";
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
import type { OutlineItem } from "../generate/outline.js";

/** @internal */
type PageStructureOp =
  | { op: "appendBlank"; width: number; height: number }
  | { op: "insertBlank"; index: number; width: number; height: number }
  | { op: "removePage"; index: number }
  | { op: "movePage"; from: number; to: number };

/** WASM bindings a PdfDocument needs; satisfied by both wasm.ts and wasm-browser.ts. @internal */
export interface CoreWasm {
  decryptPdf(data: Uint8Array, password: string): Uint8Array;
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
  setOutline(data: Uint8Array, json: string): Uint8Array;
  insertPages(data: Uint8Array, opsJson: string): Uint8Array;
  applyAll(
    data: Uint8Array,
    planJson: string,
    fillImages: Uint8Array,
    drawImages: Uint8Array,
    fonts: Uint8Array,
  ): Uint8Array;
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
  private outlineItems?: OutlineItem[];
  private readonly structureOps: PageStructureOp[] = [];
  private readonly appendedPages: PdfPage[] = [];

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
        if (this.outlineItems !== undefined) {
          this.drawQueue.pushOutline(this.outlineItems);
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

    // Structural page ops (insert/remove/move) rebuild the page tree, which the
    // single-pass applyAll cannot compose with draw's page-index resolution.
    // Fall back to the chained pipeline (one WASM call per operation) only then.
    if (this.structureOps.length > 0) {
      return this.saveChained(form);
    }

    // Fast path: apply every queued operation in one parse → mutate → save pass.
    const empty = new Uint8Array(0);
    const plan: Record<string, unknown> = {};
    let fillImages: Uint8Array = empty;
    let drawImages: Uint8Array = empty;
    let fonts: Uint8Array = empty;

    if (form && form[kFormQueue].length > 0) {
      const { opsJson, images } = form[kFormQueue].toPayload();
      plan["fill"] = JSON.parse(opsJson);
      fillImages = images;
    }
    if (form && form[kFlattenQueue].length > 0) {
      plan["flatten"] = form[kFlattenQueue];
    }
    if (this.drawQueue.length > 0) {
      const resolve = this.buildPageIndexResolver();
      const { opsJson, images, fonts: f, fontsJson } = this.drawQueue.toDrawPayload(resolve);
      plan["draw"] = { ops: JSON.parse(opsJson), fonts: JSON.parse(fontsJson) };
      drawImages = images;
      fonts = f;
    }
    if (this.metadataDirty) {
      plan["metadata"] = this.metadata;
    }
    if (this.outlineItems !== undefined) {
      plan["outline"] = this.outlineItems;
    }

    if (Object.keys(plan).length === 0) {
      return this.bytes.slice();
    }

    try {
      return this.wasm.applyAll(this.bytes, JSON.stringify(plan), fillImages, drawImages, fonts);
    } catch (e) {
      throw toPdfError(e);
    }
  }

  /**
   * Legacy chained save: one WASM round-trip per operation. Used only when
   * page-structure ops are queued (see `save()`), since those cannot be merged
   * into the single-pass `applyAll`.
   */
  private async saveChained(form: PdfForm | undefined): Promise<Uint8Array> {
    let bytes = this.bytes;
    try {
      if (form && form[kFormQueue].length > 0) {
        const { opsJson, images } = form[kFormQueue].toPayload();
        bytes = this.wasm.fillFields(bytes, opsJson, images);
      }
      if (form && form[kFlattenQueue].length > 0) {
        bytes = this.wasm.flattenFields(bytes, JSON.stringify(form[kFlattenQueue]));
      }
      if (this.structureOps.length > 0) {
        bytes = this.wasm.insertPages(bytes, JSON.stringify(this.structureOps));
      }
      if (this.drawQueue.length > 0) {
        const resolve = this.buildPageIndexResolver();
        const { opsJson, images, fonts, fontsJson } = this.drawQueue.toDrawPayload(resolve);
        bytes = this.wasm.applyDrawOps(bytes, opsJson, images, fonts, fontsJson);
      }
      if (this.metadataDirty) {
        bytes = this.wasm.setMetadata(bytes, JSON.stringify(this.metadata));
      }
      if (this.outlineItems !== undefined) {
        bytes = this.wasm.setOutline(bytes, JSON.stringify(this.outlineItems));
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
   * Set the document outline (bookmarks).
   *
   * Replaces any previously set outline. The outline is applied on `save()`.
   * For loaded documents, it is applied via incremental update. For created
   * documents, it is embedded as a create-op.
   *
   * @param items - Array of top-level {@link OutlineItem} entries. Each entry
   *   must reference a valid zero-based page index.
   */
  setOutline(items: OutlineItem[]): void {
    this.outlineItems = items;
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
      } catch (e) {
        const err = toPdfError(e);
        if (err instanceof EncryptedPdfError) throw err;
        // optional metadata: soft-fallback on any other error
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
      if (d !== undefined) result.modificationDate = d;
    }
    return result;
  }

  /** Number of pages in the document. */
  getPageCount(): number {
    if (this.mode === "create") return this.createdPages.length;
    const base = this.loadPages().length;
    const netDelta = this.structureOps.reduce((acc, op) => {
      if (op.op === "appendBlank" || op.op === "insertBlank") return acc + 1;
      if (op.op === "removePage") return acc - 1;
      return acc;
    }, 0);
    return base + netDelta;
  }

  /** All pages, in document order. The same instances are returned every time. */
  getPages(): PdfPage[] {
    if (this.mode === "create") return [...this.createdPages];
    return [...this.loadPages(), ...this.appendedPages];
  }

  /** Get one page by zero-based index. */
  getPage(index: number): PdfPage {
    if (this.mode === "create") {
      const page = this.createdPages[index];
      if (page === undefined) throw new PageOutOfRangeError(index, this.createdPages.length);
      return page;
    }
    const loaded = this.loadPages();
    if (index < loaded.length) {
      const page = loaded[index];
      if (page === undefined) throw new PageOutOfRangeError(index, this.getPageCount());
      return page;
    }
    const appendedIndex = index - loaded.length;
    const page = this.appendedPages[appendedIndex];
    if (page === undefined) throw new PageOutOfRangeError(index, this.getPageCount());
    return page;
  }

  /**
   * Append a blank page to the document. Size defaults to A4.
   *
   * On created documents: existing behavior (page is drawable immediately).
   * On loaded documents: queues an `appendBlank` structural op; the page is
   * drawable and the op is applied before draw ops at save time.
   */
  addPage(size: PageSize = PageSizes.A4): PdfPage {
    const [width, height] = size;
    if (this.mode === "create") {
      const index = this.createdPages.length;
      this.drawQueue.pushAddPage(width, height);
      const page = new PdfPage(index, width, height, 0, this.drawQueue);
      this.createdPages.push(page);
      return page;
    }
    // load mode: queue structural op, return a drawable PdfPage handle.
    // The handle carries a stable negative slot id (not its current index);
    // its final index is resolved at save time, so a later insert/remove/move
    // re-targets draws onto the right page.
    const slot = -(this.appendedPages.length + 1);
    this.structureOps.push({ op: "appendBlank", width, height });
    const index = this.getPageCount() - 1; // best-effort index at call time
    const page = new PdfPage(index, width, height, 0, this.drawQueue, slot);
    this.appendedPages.push(page);
    return page;
  }

  /**
   * Insert a blank page at the given index. Load-mode only.
   *
   * @throws `PdfError` when called on a document created with `PdfDocument.create()`.
   */
  insertPage(index: number, size: PageSize = PageSizes.A4): void {
    if (this.mode !== "load") {
      throw new PdfError("insertPage is only available on documents opened with PdfDocument.load()");
    }
    const count = this.getPageCount();
    if (!Number.isInteger(index) || index < 0 || index > count) {
      throw new PageOutOfRangeError(index, count + 1);
    }
    const [width, height] = size;
    this.structureOps.push({ op: "insertBlank", index, width, height });
  }

  /**
   * Remove the page at the given index. Load-mode only.
   *
   * @throws `PdfError` when called on a document created with `PdfDocument.create()`.
   */
  removePage(index: number): void {
    if (this.mode !== "load") {
      throw new PdfError("removePage is only available on documents opened with PdfDocument.load()");
    }
    const count = this.getPageCount();
    if (!Number.isInteger(index) || index < 0 || index >= count) {
      throw new PageOutOfRangeError(index, count);
    }
    this.structureOps.push({ op: "removePage", index });
  }

  /**
   * Move a page from one index to another. Load-mode only.
   *
   * @throws `PdfError` when called on a document created with `PdfDocument.create()`.
   */
  movePage(from: number, to: number): void {
    if (this.mode !== "load") {
      throw new PdfError("movePage is only available on documents opened with PdfDocument.load()");
    }
    const count = this.getPageCount();
    if (!Number.isInteger(from) || from < 0 || from >= count) {
      throw new PageOutOfRangeError(from, count);
    }
    if (!Number.isInteger(to) || to < 0 || to >= count) {
      throw new PageOutOfRangeError(to, count);
    }
    this.structureOps.push({ op: "movePage", from, to });
  }

  /**
   * Build a resolver mapping each page's stable slot id to its final
   * zero-based index after all queued structural ops are applied (in order).
   * Loaded pages use their original index as slot id; appended pages use a
   * negative sentinel `-(k+1)`. Throws if a drawn-on page was removed.
   * @internal
   */
  private buildPageIndexResolver(): (slot: number) => number {
    if (this.mode === "create") return (slot) => slot;
    const loadedCount = this.loadPages().length;
    // slots[i] holds the slot id occupying final position i; null = a blank
    // inserted page (no handle, never resolved).
    const slots: (number | null)[] = [];
    for (let i = 0; i < loadedCount; i++) slots.push(i);
    let appendSeq = 0;
    for (const op of this.structureOps) {
      switch (op.op) {
        case "appendBlank":
          slots.push(-(++appendSeq));
          break;
        case "insertBlank":
          slots.splice(op.index, 0, null);
          break;
        case "removePage":
          slots.splice(op.index, 1);
          break;
        case "movePage": {
          const [moved] = slots.splice(op.from, 1);
          slots.splice(op.to, 0, moved ?? null);
          break;
        }
      }
    }
    const map = new Map<number, number>();
    slots.forEach((s, i) => {
      if (s !== null) map.set(s, i);
    });
    return (slot) => {
      const idx = map.get(slot);
      if (idx === undefined) {
        throw new PdfError(
          "cannot draw on a page that was removed before save(); the page handle no longer maps to a page in the document",
        );
      }
      return idx;
    };
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
        (p) => new PdfPage(p.index, p.width, p.height, p.rotation, this.drawQueue, p.index),
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
    if (!this.form) {
      try {
        this.form = new PdfForm(this.bytes, this.wasm.readFields);
      } catch (e) {
        throw toPdfError(e);
      }
    }
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

  /**
   * @internal
   * Shared `PdfDocument.load` body: coerce input to bytes, decrypting when a
   * password is supplied. The entry barrels add the per-runtime init prelude.
   */
  protected static loadBytes(
    wasmBinding: CoreWasm,
    input: Uint8Array | ArrayBuffer,
    opts?: { password?: string },
  ): Uint8Array {
    const raw = input instanceof Uint8Array ? input : new Uint8Array(input);
    if (opts?.password === undefined) return raw;
    try {
      return wasmBinding.decryptPdf(raw, opts.password);
    } catch (e) {
      throw toPdfError(e);
    }
  }

  /** @internal Shared `PdfDocument.assemble` body. */
  protected static assembleImpl(
    wasmBinding: CoreWasm,
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
  ): Uint8Array {
    return PdfDocumentBase.runAssemble(docs, selections, wasmBinding);
  }

  /** @internal Shared `PdfDocument.merge` body: every page of every doc, in order. */
  protected static mergeImpl(wasmBinding: CoreWasm, docs: Uint8Array[]): Uint8Array {
    const selections: { docIndex: number; pageIndex: number }[] = [];
    for (let docIndex = 0; docIndex < docs.length; docIndex++) {
      let pageInfos: { index: number }[];
      try {
        pageInfos = JSON.parse(wasmBinding.readPages(docs[docIndex]!)) as { index: number }[];
      } catch (e) {
        throw toPdfError(e);
      }
      for (let pageIndex = 0; pageIndex < pageInfos.length; pageIndex++) {
        selections.push({ docIndex, pageIndex });
      }
    }
    return PdfDocumentBase.runAssemble(docs, selections, wasmBinding);
  }
}
