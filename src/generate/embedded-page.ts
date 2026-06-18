/** A reference to a page from a source PDF, ready to be drawn onto another page. Obtain via `doc.embedPdfPage()`. */
export class EmbeddedPdfPage {
  /** @internal */
  constructor(
    /** @internal raw source PDF bytes; embedded into the target PDF at save time */
    readonly bytes: Uint8Array,
    /** Zero-based page index within the source PDF. */
    readonly srcPage: number,
    /** Intrinsic width of the source page in PDF points. */
    readonly width: number,
    /** Intrinsic height of the source page in PDF points. */
    readonly height: number,
  ) {}
}
