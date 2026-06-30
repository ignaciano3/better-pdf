import { toPdfDate, fromPdfDate, type DocumentMetadata } from "../generate/metadata.js";

/**
 * Mutable accumulator for a document's metadata edits. Holds the wire-format
 * (key → string) map plus a dirty flag, exposes the typed setters, and merges
 * locally-set values over whatever was read from the PDF into the public
 * {@link DocumentMetadata} shape. Extracted from `PdfDocumentBase` so the
 * document class just delegates.
 * @internal
 */
export class MetadataState {
  private readonly values: Record<string, string> = {};
  private dirtyFlag = false;

  /** True once any setter has run; gates whether `save()` writes metadata. */
  get dirty(): boolean {
    return this.dirtyFlag;
  }

  /** The raw wire map handed to the core. Only meaningful when {@link dirty}. */
  get wire(): Record<string, string> {
    return this.values;
  }

  private set(key: string, value: string): void {
    this.values[key] = value;
    this.dirtyFlag = true;
  }

  setTitle(value: string): void {
    this.set("title", value);
  }

  setAuthor(value: string): void {
    this.set("author", value);
  }

  setSubject(value: string): void {
    this.set("subject", value);
  }

  /** The array is joined with ", " to match the PDF keywords convention. */
  setKeywords(values: string[]): void {
    this.set("keywords", values.join(", "));
  }

  setCreator(value: string): void {
    this.set("creator", value);
  }

  setProducer(value: string): void {
    this.set("producer", value);
  }

  setCreationDate(date: Date): void {
    this.set("creationDate", toPdfDate(date));
  }

  setModificationDate(date: Date): void {
    this.set("modDate", toPdfDate(date));
  }

  /**
   * Merge locally-set values over `wire` (the map read from the PDF, empty for
   * created documents) into the public {@link DocumentMetadata} shape. Locally-
   * set values win.
   */
  merge(wire: Record<string, string>): DocumentMetadata {
    const merged = { ...wire, ...this.values };

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
}
