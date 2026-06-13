/** An embedded image. Obtain one with `doc.embedJpg(bytes)` or `doc.embedPng(bytes)`. */
export class PdfImage {
  /** @internal */
  constructor(
    /** @internal raw image bytes, embedded into the PDF at save time */
    readonly bytes: Uint8Array,
    /** Intrinsic image width in pixels. */
    readonly width: number,
    /** Intrinsic image height in pixels. */
    readonly height: number,
  ) {}

  /** Return `{ width, height }` scaled by `factor` (for passing to drawImage). */
  scale(factor: number): { width: number; height: number } {
    return { width: this.width * factor, height: this.height * factor };
  }
}
