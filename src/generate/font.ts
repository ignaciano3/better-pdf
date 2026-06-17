import { StandardFonts } from "./fonts.js";

type MeasureStandard = (font: string, size: number, text: string) => number;
type MeasureEmbedded = (bytes: Uint8Array, size: number, text: string) => number;

/**
 * A font handle for measuring (and, for embedded fonts, drawing) text.
 *
 * Obtain a standard-14 handle with `doc.getFont(...)` or an embedded handle
 * with `doc.embedFont(...)`.
 */
export class PdfFont {
  /** @internal Embedded-font id within the document's draw queue; undefined for standard-14. */
  readonly _fontId?: number;
  /** @internal Embedded-font bytes; undefined for standard-14. */
  readonly _bytes?: Uint8Array;
  private readonly measureEmbedded?: MeasureEmbedded;

  /** @internal */
  constructor(
    /** The standard-14 base font name (also a {@link StandardFonts} value). */
    readonly name: StandardFonts,
    private readonly measure: MeasureStandard,
  ) {}

  /** @internal Construct an embedded-font handle. */
  static embedded(fontId: number, bytes: Uint8Array, measureEmbedded: MeasureEmbedded): PdfFont {
    // name is unused for embedded fonts; keep a placeholder to satisfy the field.
    const font = new PdfFont(StandardFonts.Helvetica, () => 0);
    Object.assign(font, { _fontId: fontId, _bytes: bytes, measureEmbedded });
    return font;
  }

  /** Width in points of `text` at `size` in this font. */
  widthOfTextAtSize(text: string, size: number): number {
    if (!Number.isFinite(size) || size <= 0) {
      throw new RangeError(`size must be > 0, got ${size}`);
    }
    if (this._fontId !== undefined && this._bytes !== undefined && this.measureEmbedded) {
      return this.measureEmbedded(this._bytes, size, text);
    }
    return this.measure(this.name, size, text);
  }
}
