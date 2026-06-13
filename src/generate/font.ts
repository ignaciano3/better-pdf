import { StandardFonts } from "./fonts.js";

/** A standard-14 font handle for measuring text. Obtain with `doc.getFont(...)`. */
export class PdfFont {
  /** @internal */
  constructor(
    /** The standard-14 base font name (also a {@link StandardFonts} value). */
    readonly name: StandardFonts,
    private readonly measure: (font: string, size: number, text: string) => number,
  ) {}

  /** Width in points of `text` at `size` in this font. */
  widthOfTextAtSize(text: string, size: number): number {
    if (!Number.isFinite(size) || size <= 0) {
      throw new RangeError(`size must be > 0, got ${size}`);
    }
    return this.measure(this.name, size, text);
  }
}
