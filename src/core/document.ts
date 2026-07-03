import { PdfForm, kFormQueue, kFlattenQueue } from "../forms/form.js";
import {
  toPdfError,
  PageOutOfRangeError,
  PdfError,
  toInvalidImageError,
  EncryptedPdfError,
  FormSealedError,
} from "./errors.js";
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
import { type DocumentMetadata } from "../generate/metadata.js";
import { MetadataState } from "./metadata-state.js";
import type { OutlineItem } from "../generate/outline.js";

/** @internal Page descriptor returned by the core's `readPages`. */
type PageInfo = { index: number; width: number; height: number; rotation: number };

/**
 * Call a WASM function that returns a JSON string and parse it, mapping any
 * thrown error through `mapErr` (default {@link toPdfError}).
 * @internal
 */
function callJson<T>(fn: () => string, mapErr: (e: unknown) => Error = toPdfError): T {
  try {
    return JSON.parse(fn()) as T;
  } catch (e) {
    throw mapErr(e);
  }
}

/**
 * Call a WASM function that returns bytes, mapping any thrown error through
 * `mapErr` (default {@link toPdfError}).
 * @internal
 */
function callBytes(fn: () => Uint8Array, mapErr: (e: unknown) => Error = toPdfError): Uint8Array {
  try {
    return fn();
  } catch (e) {
    throw mapErr(e);
  }
}

/** @internal */
type PageStructureOp =
  | { op: "appendBlank"; width: number; height: number }
  | { op: "insertBlank"; index: number; width: number; height: number }
  | { op: "removePage"; index: number }
  | { op: "movePage"; from: number; to: number };

/**
 * Map each page's stable slot id to its final zero-based index after applying
 * `structureOps` (in order) to a document with `loadedCount` original pages.
 * Loaded pages use their original index as slot id; appended pages use a
 * negative sentinel `-(k+1)`; inserted blanks have no handle (`null`). The
 * returned resolver throws if a drawn-on page was removed before save.
 * @internal
 */
function buildPageIndexResolver(
  structureOps: PageStructureOp[],
  loadedCount: number,
): (slot: number) => number {
  // slots[i] holds the slot id occupying final position i; null = a blank
  // inserted page (no handle, never resolved).
  const slots: (number | null)[] = [];
  for (let i = 0; i < loadedCount; i++) slots.push(i);
  let appendSeq = 0;
  for (const op of structureOps) {
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

/** WASM bindings a PdfDocument needs; satisfied by both wasm.ts and wasm-browser.ts. @internal */
export interface CoreWasm {
  decryptPdf(data: Uint8Array, password: string): Uint8Array;
  readFields(data: Uint8Array): string;
  fillFields(data: Uint8Array, opsJson: string, images: Uint8Array, compress?: boolean): Uint8Array;
  flattenFields(data: Uint8Array, namesJson: string, compress?: boolean): Uint8Array;
  readPages(data: Uint8Array): string;
  applyDrawOps(
    data: Uint8Array,
    opsJson: string,
    images: Uint8Array,
    fonts?: Uint8Array,
    fontsJson?: string,
    compress?: boolean,
  ): Uint8Array;
  createDocument(
    opsJson: string,
    images?: Uint8Array,
    fonts?: Uint8Array,
    fontsJson?: string,
    fieldsJson?: string,
    compress?: boolean,
    objectStreams?: boolean,
  ): Uint8Array;
  imageInfo(data: Uint8Array): string;
  measureText(font: string, size: number, text: string): number;
  measureTextEmbedded(font: Uint8Array, size: number, text: string): number;
  readMetadata(data: Uint8Array): string;
  setMetadata(data: Uint8Array, metaJson: string, compress?: boolean): Uint8Array;
  manipulatePages(
    docsBlob: Uint8Array,
    docsJson: string,
    planJson: string,
    compress?: boolean,
    objectStreams?: boolean,
  ): Uint8Array;
  setOutline(data: Uint8Array, json: string, compress?: boolean): Uint8Array;
  insertPages(data: Uint8Array, opsJson: string, compress?: boolean): Uint8Array;
  injectFields(
    data: Uint8Array,
    fieldsJson: string,
    fonts?: Uint8Array,
    fontsJson?: string,
    compress?: boolean,
  ): Uint8Array;
  applyAll(
    data: Uint8Array,
    planJson: string,
    fillImages: Uint8Array,
    drawImages: Uint8Array,
    fonts: Uint8Array,
    compress?: boolean,
  ): Uint8Array;
}

/** Options for {@link PdfDocumentBase.save}. */
export interface SaveOptions {
  /** Deflate-compress generated streams. Defaults to `true`. */
  compress?: boolean;
  /**
   * Pack non-stream objects into PDF object streams (+ cross-reference streams)
   * for smaller output. Defaults to `false`. Honored only for created documents
   * saved directly via `save()` (a created document materialized by `getForm()`
   * becomes sealed and takes the incremental path, which ignores this flag).
   * Ignored for loaded-document (incremental) saves.
   */
  objectStreams?: boolean;
}

/** Options for the full-document assembly operations (merge/assemble/copy/split). */
export interface ManipulateOptions {
  /**
   * Pack non-stream objects into PDF object streams (+ cross-reference streams)
   * for smaller output. Defaults to `false`.
   */
  objectStreams?: boolean;
}

export class PdfDocumentBase {
  private form?: PdfForm;
  private pages?: PdfPage[];
  private readonly createdPages: PdfPage[] = [];
  private readonly drawQueue = new DrawQueue();
  private readonly fieldDefs: FieldDef[] = [];
  private readonly fieldNames = new Set<string>();
  private readonly meta = new MetadataState();
  private outlineItems?: OutlineItem[];
  private readonly structureOps: PageStructureOp[] = [];
  private readonly appendedPages: PdfPage[] = [];
  private sealed = false;
  private formFlushed = false;

  /** @internal */
  protected constructor(
    protected bytes: Uint8Array,
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
  async save(options: SaveOptions = {}): Promise<Uint8Array> {
    const compress = options.compress ?? true;
    const objectStreams = options.objectStreams ?? false;

    if (this.mode === "create" && !this.sealed) {
      try {
        return this.buildCreatedBytes(compress, objectStreams);
      } catch (e) {
        throw toPdfError(e);
      }
    }

    this.injectPendingFields(compress); // load-mode: bake any pending builder fields

    const form = this.form;

    // Structural page ops (insert/remove/move) rebuild the page tree, which the
    // single-pass applyAll cannot compose with draw's page-index resolution.
    // Fall back to the chained pipeline (one WASM call per operation) only then.
    if (this.structureOps.length > 0) {
      return this.saveChained(form, compress);
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
    if (!this.sealed && this.drawQueue.length > 0) {
      const resolve = this.buildPageIndexResolver();
      const { opsJson, images, fonts: f, fontsJson } = this.drawQueue.toDrawPayload(resolve);
      plan["draw"] = { ops: JSON.parse(opsJson), fonts: JSON.parse(fontsJson) };
      drawImages = images;
      fonts = f;
    }
    if (this.meta.dirty) {
      plan["metadata"] = this.meta.wire;
    }
    if (this.outlineItems !== undefined) {
      plan["outline"] = this.outlineItems;
    }

    if (Object.keys(plan).length === 0) {
      return this.bytes.slice();
    }

    return callBytes(() =>
      this.wasm.applyAll(this.bytes, JSON.stringify(plan), fillImages, drawImages, fonts, compress),
    );
  }

  /**
   * Legacy chained save: one WASM round-trip per operation. Used only when
   * page-structure ops are queued (see `save()`), since those cannot be merged
   * into the single-pass `applyAll`.
   */
  private async saveChained(form: PdfForm | undefined, compress: boolean): Promise<Uint8Array> {
    let bytes = this.bytes;
    try {
      if (form && form[kFormQueue].length > 0) {
        const { opsJson, images } = form[kFormQueue].toPayload();
        bytes = this.wasm.fillFields(bytes, opsJson, images, compress);
      }
      if (form && form[kFlattenQueue].length > 0) {
        bytes = this.wasm.flattenFields(bytes, JSON.stringify(form[kFlattenQueue]), compress);
      }
      if (this.structureOps.length > 0) {
        bytes = this.wasm.insertPages(bytes, JSON.stringify(this.structureOps), compress);
      }
      if (!this.sealed && this.drawQueue.length > 0) {
        const resolve = this.buildPageIndexResolver();
        const { opsJson, images, fonts, fontsJson } = this.drawQueue.toDrawPayload(resolve);
        bytes = this.wasm.applyDrawOps(bytes, opsJson, images, fonts, fontsJson, compress);
      }
      if (this.meta.dirty) {
        bytes = this.wasm.setMetadata(bytes, JSON.stringify(this.meta.wire), compress);
      }
      if (this.outlineItems !== undefined) {
        bytes = this.wasm.setOutline(bytes, JSON.stringify(this.outlineItems), compress);
      }
    } catch (e) {
      throw toPdfError(e);
    }
    if (bytes === this.bytes) {
      return this.bytes.slice();
    }
    return bytes;
  }

  /**
   * Build the finished PDF bytes for a created document: bake queued metadata
   * and outline, then run the single-pass createDocument with all fields.
   * Shared by `save()` (create mode) and `getForm()` materialization.
   */
  private buildCreatedBytes(compress = true, objectStreams = false): Uint8Array {
    if (this.meta.dirty) {
      this.drawQueue.pushMetadata(this.meta.wire);
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
      compress,
      objectStreams,
    );
  }

  /** Set the document title metadata. */
  setTitle(value: string): void {
    this.meta.setTitle(value);
  }

  /** Set the document author metadata. */
  setAuthor(value: string): void {
    this.meta.setAuthor(value);
  }

  /** Set the document subject metadata. */
  setSubject(value: string): void {
    this.meta.setSubject(value);
  }

  /** Set the document keywords metadata. The array is joined with ", ". */
  setKeywords(values: string[]): void {
    this.meta.setKeywords(values);
  }

  /** Set the document creator metadata. */
  setCreator(value: string): void {
    this.meta.setCreator(value);
  }

  /** Set the document producer metadata. */
  setProducer(value: string): void {
    this.meta.setProducer(value);
  }

  /** Set the document creation date metadata. */
  setCreationDate(date: Date): void {
    this.meta.setCreationDate(date);
  }

  /** Set the document modification date metadata. */
  setModificationDate(date: Date): void {
    this.meta.setModificationDate(date);
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

    // Locally-set values win.
    return this.meta.merge(wire);
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
    this.assertNotSealed();
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
    this.assertNotSealed();
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
    this.assertNotSealed();
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
    this.assertNotSealed();
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
    return buildPageIndexResolver(this.structureOps, this.loadPages().length);
  }

  private assertNotSealed(): void {
    if (this.sealed) throw new FormSealedError();
  }

  /**
   * Begin building an AcroForm.
   *
   * Returns a {@link FormBuilder} that accumulates field definitions. The
   * builder shares state with the document; the added fields are serialized to
   * Rust when the document is finalized.
   *
   * On a document created with {@link PdfDocument.create}, the fields are baked
   * in when the created document is materialized (the first `getForm()` or
   * `save()`).
   *
   * On a document opened with {@link PdfDocument.load}, the fields are injected
   * into the existing PDF on the first `getForm()` or `save()`, whichever comes
   * first. All `createForm()` field-adds must therefore happen *before* the
   * first `getForm()`.
   *
   * @throws `PdfError` when called after the form has already been built for
   *   this document (i.e. after the first `getForm()`).
   * @throws `FormSealedError` on a created document whose content was sealed by
   *   a prior `getForm()`.
   */
  createForm(): FormBuilder {
    if (this.mode === "create") {
      this.assertNotSealed();
    } else if (this.form || this.formFlushed) {
      throw new PdfError(
        "createForm() must be called before getForm(); the form has already been built for this document",
      );
    }
    return new FormBuilder(this.fieldDefs, this.fieldNames);
  }

  private loadPages(): PdfPage[] {
    if (!this.pages) {
      const infos = callJson<PageInfo[]>(() => this.wasm.readPages(this.bytes));
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
    const info = callJson<{ width: number; height: number }>(
      () => this.wasm.imageInfo(bytes),
      toInvalidImageError,
    );
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
    const infos = callJson<PageInfo[]>(() => this.wasm.readPages(src));
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
    if (this.mode === "create" && !this.sealed) {
      this.materializeCreatedForm();
    } else {
      this.injectPendingFields();
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

  /**
   * Turn a created document into a load-backed, sealed one: build real bytes,
   * swap them in, freeze the draw queue, and clear the metadata/outline that
   * are now baked into those bytes. The load-mode save pipeline takes over.
   */
  private materializeCreatedForm(): void {
    let bytes: Uint8Array;
    try {
      bytes = this.buildCreatedBytes();
    } catch (e) {
      throw toPdfError(e);
    }
    this.bytes = bytes;
    this.drawQueue.seal();
    this.sealed = true;
    this.meta.clearDirty();
    this.outlineItems = undefined;
    this.formFlushed = true;
  }

  /**
   * Loaded-doc analogue of `materializeCreatedForm`: bake any pending
   * `createForm()`-queued fields into `this.bytes` via `inject_fields`, then
   * clear the pending state so the load-mode form path takes over. No-op on
   * created documents (handled by `materializeCreatedForm`) and when nothing
   * was queued, so the hot load→mutate→save path is untouched.
   */
  private injectPendingFields(compress = true): void {
    if (this.mode !== "load" || this.fieldDefs.length === 0) return;
    const { fonts, fontsJson } = this.drawQueue.toCreatePayload();
    let bytes: Uint8Array;
    try {
      bytes = this.wasm.injectFields(
        this.bytes,
        JSON.stringify(this.fieldDefs),
        fonts,
        fontsJson,
        compress,
      );
    } catch (e) {
      throw toPdfError(e);
    }
    this.bytes = bytes;
    this.fieldDefs.length = 0;
    this.fieldNames.clear();
    this.formFlushed = true;
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
  async copyPages(indices: number[], options: ManipulateOptions = {}): Promise<Uint8Array> {
    if (this.mode !== "load") {
      throw new PdfError("copyPages is only available on documents opened with PdfDocument.load()");
    }
    const selections = indices.map((i) => ({ docIndex: 0, pageIndex: i }));
    return PdfDocumentBase.runAssemble(
      [this.bytes],
      selections,
      this.wasm,
      options.objectStreams ?? false,
    );
  }

  /**
   * Split the document into individual single-page PDFs.
   *
   * Only available on documents opened with {@link PdfDocument.load}.
   *
   * @returns An array of PDF byte arrays, one per page, in document order.
   * @throws `PdfError` when called on a document created with `PdfDocument.create()`.
   */
  async splitPages(options: ManipulateOptions = {}): Promise<Uint8Array[]> {
    if (this.mode !== "load") {
      throw new PdfError(
        "splitPages is only available on documents opened with PdfDocument.load()",
      );
    }
    const objectStreams = options.objectStreams ?? false;
    const count = this.getPageCount();
    const results: Uint8Array[] = [];
    for (let i = 0; i < count; i++) {
      results.push(
        await PdfDocumentBase.runAssemble(
          [this.bytes],
          [{ docIndex: 0, pageIndex: i }],
          this.wasm,
          objectStreams,
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
    objectStreams = false,
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
    return callBytes(() =>
      wasmBinding.manipulatePages(blob, docsJson, planJson, true, objectStreams),
    );
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
    const password = opts.password;
    return callBytes(() => wasmBinding.decryptPdf(raw, password));
  }

  /** @internal Shared `PdfDocument.assemble` body. */
  protected static assembleImpl(
    wasmBinding: CoreWasm,
    docs: Uint8Array[],
    selections: { docIndex: number; pageIndex: number }[],
    objectStreams = false,
  ): Uint8Array {
    return PdfDocumentBase.runAssemble(docs, selections, wasmBinding, objectStreams);
  }

  /** @internal Shared `PdfDocument.merge` body: every page of every doc, in order. */
  protected static mergeImpl(
    wasmBinding: CoreWasm,
    docs: Uint8Array[],
    objectStreams = false,
  ): Uint8Array {
    const selections: { docIndex: number; pageIndex: number }[] = [];
    for (let docIndex = 0; docIndex < docs.length; docIndex++) {
      const pageInfos = callJson<PageInfo[]>(() => wasmBinding.readPages(docs[docIndex]!));
      for (let pageIndex = 0; pageIndex < pageInfos.length; pageIndex++) {
        selections.push({ docIndex, pageIndex });
      }
    }
    return PdfDocumentBase.runAssemble(docs, selections, wasmBinding, objectStreams);
  }
}
