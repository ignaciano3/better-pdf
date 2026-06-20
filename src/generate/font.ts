import { StandardFonts } from "./fonts.js";
import { kFontId, kFontBytes } from "../core/internal.js";

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
  readonly [kFontId]?: number;
  /** @internal Embedded-font bytes; undefined for standard-14. */
  readonly [kFontBytes]?: Uint8Array;
  private readonly measureEmbedded?: MeasureEmbedded;

  /** @internal */
  constructor(
    /**
     * The standard-14 base font name (also a {@link StandardFonts} value).
     *
     * **Note:** For embedded fonts this returns a Helvetica placeholder and
     * should not be relied upon. Use the font handle for drawing and measuring
     * only; do not inspect `name` to identify an embedded font.
     */
    readonly name: StandardFonts,
    private readonly measure: MeasureStandard,
  ) {}

  /** @internal Construct an embedded-font handle. */
  static embedded(fontId: number, bytes: Uint8Array, measureEmbedded: MeasureEmbedded): PdfFont {
    // name is unused for embedded fonts; keep a placeholder to satisfy the field.
    const font = new PdfFont(StandardFonts.Helvetica, () => 0);
    Object.assign(font, { [kFontId]: fontId, [kFontBytes]: bytes, measureEmbedded });
    return font;
  }

  /** Width in points of `text` at `size` in this font. */
  widthOfTextAtSize(text: string, size: number): number {
    if (!Number.isFinite(size) || size <= 0) {
      throw new RangeError(`size must be > 0, got ${size}`);
    }
    const fontId = this[kFontId];
    const fontBytes = this[kFontBytes];
    if (fontId !== undefined && fontBytes !== undefined && this.measureEmbedded) {
      return this.measureEmbedded(fontBytes, size, text);
    }
    return this.measure(this.name, size, text);
  }
}

export { kFontId, kFontBytes };
