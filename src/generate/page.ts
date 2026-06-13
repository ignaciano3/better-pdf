import { StandardFonts } from "./fonts.js";
import { rgb, type Color } from "./color.js";
import type { DrawQueue } from "./draw-queue.js";
import { PdfImage } from "./image.js";

/** Options for {@link PdfPage.drawText}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawTextOptions {
  x: number;
  y: number;
  /** Font size in points. Must be > 0. */
  size: number;
  /** One of the 14 standard fonts. Defaults to Helvetica. */
  font?: StandardFonts;
  /** Text color. Defaults to black. */
  color?: Color;
  /** Distance between baselines for multiline text ("\n"). Defaults to 1.15 * size. */
  lineHeight?: number;
}

/** Options for {@link PdfPage.drawImage}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawImageOptions {
  x: number;
  y: number;
  /** Width in PDF points. Defaults to the image's intrinsic pixel width. */
  width?: number;
  /** Height in PDF points. Defaults to the image's intrinsic pixel height. */
  height?: number;
}

/**
 * A page of a PdfDocument. Drawing methods queue operations that are
 * applied when the document is saved.
 */
export class PdfPage {
  /** @internal */
  constructor(
    /** Zero-based page index. */
    readonly index: number,
    /** Page width in PDF points. */
    readonly width: number,
    /** Page height in PDF points. */
    readonly height: number,
    /** Page rotation in degrees (0, 90, 180, or 270). */
    readonly rotation: number,
    private readonly queue: DrawQueue,
  ) {}

  /**
   * Draw text on the page at `(x, y)` (baseline of the first line, origin
   * bottom-left). Standard-14 fonts only; characters outside WinAnsi are
   * substituted at save time by the core.
   */
  drawText(text: string, options: DrawTextOptions): void {
    if (!Number.isFinite(options.size) || options.size <= 0) {
      throw new RangeError(`size must be > 0, got ${options.size}`);
    }
    if (!Number.isFinite(options.x) || !Number.isFinite(options.y)) {
      throw new RangeError(`x and y must be finite numbers`);
    }
    if (
      options.lineHeight !== undefined &&
      (!Number.isFinite(options.lineHeight) || options.lineHeight <= 0)
    ) {
      throw new RangeError(`lineHeight must be > 0, got ${options.lineHeight}`);
    }
    this.queue.pushText(this.index, text, {
      x: options.x,
      y: options.y,
      size: options.size,
      font: options.font ?? StandardFonts.Helvetica,
      color: options.color ?? rgb(0, 0, 0),
      lineHeight: options.lineHeight,
    });
  }

  /**
   * Draw an embedded image on the page at `(x, y)` (bottom-left corner, origin
   * bottom-left). The image must first be embedded via `doc.embedJpg()` or
   * `doc.embedPng()`.
   */
  drawImage(image: PdfImage, options: DrawImageOptions): void {
    const width = options.width ?? image.width;
    const height = options.height ?? image.height;
    for (const [v, name] of [
      [options.x, "x"],
      [options.y, "y"],
      [width, "width"],
      [height, "height"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    if (width <= 0 || height <= 0) throw new RangeError("width and height must be > 0");
    this.queue.pushImage(this.index, image.bytes, {
      x: options.x,
      y: options.y,
      width,
      height,
    });
  }
}
