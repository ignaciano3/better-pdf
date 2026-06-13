import { StandardFonts } from "./fonts.js";
import { rgb, type Color } from "./color.js";
import type { DrawQueue, LineOp, RectangleOp, EllipseOp } from "./draw-queue.js";
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

/** Options for {@link PdfPage.drawLine}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawLineOptions {
  start: { x: number; y: number };
  end: { x: number; y: number };
  /** Stroke width in points. Default 1. */
  thickness?: number;
  /** Stroke color. Default black. */
  color?: Color;
  /** Opacity 0..1. Default 1 (opaque). */
  opacity?: number;
}

/** Options for {@link PdfPage.drawRectangle}. `(x, y)` is the lower-left corner. */
export interface DrawRectangleOptions {
  x: number;
  y: number;
  width: number;
  height: number;
  /** Fill color. Omit for no fill. */
  color?: Color;
  /** Border color. Omit for no border. */
  borderColor?: Color;
  /** Border width in points. Default 1 when borderColor is set. */
  borderWidth?: number;
  /** Opacity 0..1. Default 1. */
  opacity?: number;
}

/** Options for {@link PdfPage.drawEllipse}. `(x, y)` is the center. */
export interface DrawEllipseOptions {
  x: number;
  y: number;
  /** Horizontal radius in points. */
  xScale: number;
  /** Vertical radius in points. */
  yScale: number;
  /** Fill color. Omit for no fill. */
  color?: Color;
  /** Border color. Omit for no border. */
  borderColor?: Color;
  /** Border width in points. Default 1 when borderColor is set. */
  borderWidth?: number;
  /** Opacity 0..1. Default 1. */
  opacity?: number;
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

function validateOpacity(o: number | undefined): void {
  if (o !== undefined && (!Number.isFinite(o) || o < 0 || o > 1)) {
    throw new RangeError(`opacity must be in 0..1, got ${o}`);
  }
}

function validateBorderWidth(w: number | undefined, name = "borderWidth"): void {
  if (w !== undefined && (!Number.isFinite(w) || w < 0)) {
    throw new RangeError(`${name} must be >= 0, got ${w}`);
  }
}

function tuple(c: Color): [number, number, number] {
  return [c.red, c.green, c.blue];
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

  /**
   * Draw a straight line from `start` to `end`. Coordinates use the PDF
   * convention: origin bottom-left.
   */
  drawLine(options: DrawLineOptions): void {
    const { start, end, thickness, color, opacity } = options;
    for (const [v, name] of [
      [start.x, "start.x"],
      [start.y, "start.y"],
      [end.x, "end.x"],
      [end.y, "end.y"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    validateBorderWidth(thickness, "thickness");
    validateOpacity(opacity);
    const op: LineOp = {
      op: "line",
      page: this.index,
      x1: start.x,
      y1: start.y,
      x2: end.x,
      y2: end.y,
      ...(thickness !== undefined ? { thickness } : {}),
      ...(color !== undefined ? { color: tuple(color) } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
    };
    this.queue.pushLine(op);
  }

  /**
   * Draw a rectangle with optional fill and border. `(x, y)` is the lower-left
   * corner. Coordinates use the PDF convention: origin bottom-left.
   */
  drawRectangle(options: DrawRectangleOptions): void {
    const { x, y, width, height, color, borderColor, borderWidth, opacity } = options;
    for (const [v, name] of [
      [x, "x"],
      [y, "y"],
      [width, "width"],
      [height, "height"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    if (width <= 0) throw new RangeError(`width must be > 0, got ${width}`);
    if (height <= 0) throw new RangeError(`height must be > 0, got ${height}`);
    validateBorderWidth(borderWidth);
    validateOpacity(opacity);
    const op: RectangleOp = {
      op: "rectangle",
      page: this.index,
      x,
      y,
      width,
      height,
      ...(color !== undefined ? { color: tuple(color) } : {}),
      ...(borderColor !== undefined ? { borderColor: tuple(borderColor) } : {}),
      ...(borderWidth !== undefined ? { borderWidth } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
    };
    this.queue.pushRectangle(op);
  }

  /**
   * Draw an ellipse centered at `(x, y)` with horizontal radius `xScale` and
   * vertical radius `yScale`. Coordinates use the PDF convention: origin
   * bottom-left.
   */
  drawEllipse(options: DrawEllipseOptions): void {
    const { x, y, xScale, yScale, color, borderColor, borderWidth, opacity } = options;
    for (const [v, name] of [
      [x, "x"],
      [y, "y"],
      [xScale, "xScale"],
      [yScale, "yScale"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    if (xScale <= 0) throw new RangeError(`xScale must be > 0, got ${xScale}`);
    if (yScale <= 0) throw new RangeError(`yScale must be > 0, got ${yScale}`);
    validateBorderWidth(borderWidth);
    validateOpacity(opacity);
    const op: EllipseOp = {
      op: "ellipse",
      page: this.index,
      x,
      y,
      xScale,
      yScale,
      ...(color !== undefined ? { color: tuple(color) } : {}),
      ...(borderColor !== undefined ? { borderColor: tuple(borderColor) } : {}),
      ...(borderWidth !== undefined ? { borderWidth } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
    };
    this.queue.pushEllipse(op);
  }
}
