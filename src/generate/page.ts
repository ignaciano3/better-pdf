import { StandardFonts } from "./fonts.js";
import { rgb, type Color } from "./color.js";
import type { DrawQueue, LineOp, RectangleOp, EllipseOp } from "./draw-queue.js";
import { PdfImage } from "./image.js";
import { PdfFont } from "./font.js";

/** Options for {@link PdfPage.drawText}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawTextOptions {
  x: number;
  y: number;
  /** Font size in points. Must be > 0. */
  size: number;
  /** One of the 14 standard fonts, or a {@link PdfFont} handle. Defaults to Helvetica. */
  font?: StandardFonts | PdfFont;
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
    const embeddedId =
      options.font instanceof PdfFont && options.font._fontId !== undefined
        ? options.font._fontId
        : undefined;
    const fontName =
      embeddedId !== undefined
        ? ""
        : options.font instanceof PdfFont
          ? options.font.name
          : (options.font ?? StandardFonts.Helvetica);
    this.queue.pushText(this.index, text, {
      x: options.x,
      y: options.y,
      size: options.size,
      font: fontName,
      color: options.color ?? rgb(0, 0, 0),
      lineHeight: options.lineHeight,
      ...(embeddedId !== undefined ? { fontId: embeddedId } : {}),
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
   * Set the rotation of the page. Must be a multiple of 90 (0, 90, 180, 270, etc.).
   * Works on both loaded and created documents.
   */
  setRotation(degrees: number): void {
    if (!Number.isFinite(degrees) || degrees % 90 !== 0) {
      throw new RangeError(`degrees must be a finite multiple of 90, got ${degrees}`);
    }
    this.queue.pushSetRotation(this.index, degrees);
  }

  /**
   * Set the media box of the page to the given coordinates.
   * `x1 > x0` and `y1 > y0` must hold. Works on both loaded and created documents.
   */
  setMediaBox(x0: number, y0: number, x1: number, y1: number): void {
    for (const [v, name] of [
      [x0, "x0"],
      [y0, "y0"],
      [x1, "x1"],
      [y1, "y1"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    if (x1 <= x0) throw new RangeError(`x1 must be > x0, got x0=${x0} x1=${x1}`);
    if (y1 <= y0) throw new RangeError(`y1 must be > y0, got y0=${y0} y1=${y1}`);
    this.queue.pushSetMediaBox(this.index, [x0, y0, x1, y1]);
  }

  /**
   * Convenience method to set the page size. Equivalent to `setMediaBox(0, 0, width, height)`.
   * Works on both loaded and created documents.
   */
  setSize(width: number, height: number): void {
    if (!Number.isFinite(width) || width <= 0) {
      throw new RangeError(`width must be > 0, got ${width}`);
    }
    if (!Number.isFinite(height) || height <= 0) {
      throw new RangeError(`height must be > 0, got ${height}`);
    }
    this.setMediaBox(0, 0, width, height);
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
