import { StandardFonts } from "./fonts.js";
import { rgb, colorToTuple, type Color } from "./color.js";
import type { DrawQueue, LineOp, RectangleOp, EllipseOp, LinkOp, PathOp } from "./draw-queue.js";
import { PdfImage, kImageBytes } from "./image.js";
import { EmbeddedPdfPage, kEmbeddedBytes } from "./embedded-page.js";
import { PdfFont, kFontId } from "./font.js";
import { InvalidRotationError } from "../core/errors.js";
import { parseSvgPath, type Segment } from "./svg-path.js";

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
  /** Rotation angle in degrees (counter-clockwise). Must be finite. */
  rotate?: number;
  /** Opacity 0..1. Default 1 (fully opaque). */
  opacity?: number;
  /**
   * Maximum line width in PDF points. When set, text is word-wrapped to fit:
   * `\n` are kept as hard breaks, and a word wider than `maxWidth` overflows
   * onto its own line. Must be a positive finite number.
   */
  maxWidth?: number;
}

/** Options for {@link PdfPage.drawLine}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawLineOptions {
  start: { x: number; y: number };
  end: { x: number; y: number };
  /** Stroke width in points. Default 1. */
  strokeWidth?: number;
  /** Stroke color. Default black. */
  stroke?: Color;
  /** Opacity 0..1. Default 1 (opaque). */
  opacity?: number;
  /**
   * Dash pattern: alternating on/off segment lengths in points (e.g. `[4, 2]`).
   * Omit or pass `[]` for a solid stroke.
   */
  dash?: number[];
  /** Distance into the dash pattern at which to start. Default 0. */
  dashPhase?: number;
}

/** Options for {@link PdfPage.drawRectangle}. `(x, y)` is the lower-left corner. */
export interface DrawRectangleOptions {
  x: number;
  y: number;
  width: number;
  height: number;
  /** Fill color. Omit for no fill. */
  fill?: Color;
  /** Stroke (border) color. Omit for no border. */
  stroke?: Color;
  /** Stroke (border) width in points. Default 1 when stroke is set. */
  strokeWidth?: number;
  /** Opacity 0..1. Default 1. */
  opacity?: number;
  /** Border dash pattern in points (e.g. `[4, 2]`). Omit for a solid border. */
  dash?: number[];
  /** Distance into the dash pattern at which to start. Default 0. */
  dashPhase?: number;
}

/** Options for {@link PdfPage.drawLink}. `(x, y)` is the lower-left corner. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawLinkOptions {
  x: number;
  y: number;
  width: number;
  height: number;
  /** URI target. Exactly one of `url` or `goToPage` must be provided. */
  url?: string;
  /** Zero-based page index to navigate to within the document. Exactly one of `url` or `goToPage` must be provided. */
  goToPage?: number;
}

/** Options for {@link PdfPage.drawSvgPath}. */
export interface DrawSvgPathOptions {
  /** Fill color. Omit for no fill. */
  fill?: Color;
  /** Stroke color. Omit for no stroke. */
  stroke?: Color;
  /** Stroke width in points. Must be >= 0. */
  strokeWidth?: number;
  /** Opacity 0..1. Default 1 (opaque). */
  opacity?: number;
  /** Stroke dash pattern in points (e.g. `[4, 2]`). Omit for a solid stroke. */
  dash?: number[];
  /** Distance into the dash pattern at which to start. Default 0. */
  dashPhase?: number;
}

/** Options for {@link PdfPage.drawPolygon}. */
export interface DrawPolygonOptions {
  /** Fill color. Omit for no fill. */
  fill?: Color;
  /** Stroke color. Omit for no stroke. */
  stroke?: Color;
  /** Stroke width in points. Must be >= 0. */
  strokeWidth?: number;
  /** Opacity 0..1. Default 1 (opaque). */
  opacity?: number;
  /** Stroke dash pattern in points (e.g. `[4, 2]`). Omit for a solid stroke. */
  dash?: number[];
  /** Distance into the dash pattern at which to start. Default 0. */
  dashPhase?: number;
  /** Whether to close the polygon by appending a Z segment back to the first point. Default `true`. Pass `closed: false` for an open polyline. */
  closed?: boolean;
}

/** Options for {@link PdfPage.drawEllipse}. `(x, y)` is the center. */
export interface DrawEllipseOptions {
  x: number;
  y: number;
  /** Horizontal radius in points. */
  radiusX: number;
  /** Vertical radius in points. */
  radiusY: number;
  /** Fill color. Omit for no fill. */
  fill?: Color;
  /** Stroke (border) color. Omit for no border. */
  stroke?: Color;
  /** Stroke (border) width in points. Default 1 when stroke is set. */
  strokeWidth?: number;
  /** Opacity 0..1. Default 1. */
  opacity?: number;
  /** Border dash pattern in points (e.g. `[4, 2]`). Omit for a solid border. */
  dash?: number[];
  /** Distance into the dash pattern at which to start. Default 0. */
  dashPhase?: number;
}

/** Options for {@link PdfPage.drawImage}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawImageOptions {
  x: number;
  y: number;
  /** Width in PDF points. Defaults to the image's intrinsic pixel width. */
  width?: number;
  /** Height in PDF points. Defaults to the image's intrinsic pixel height. */
  height?: number;
  /** Constant opacity 0..1 applied to the whole image. Default 1 (opaque). */
  opacity?: number;
  /** Rotation in degrees, counter-clockwise about `(x, y)`. Default 0. */
  rotate?: number;
  /** Horizontal skew in degrees. Default 0. */
  xSkew?: number;
  /** Vertical skew in degrees. Default 0. */
  ySkew?: number;
}

/** Options for {@link PdfPage.drawPage}. Coordinates use the PDF convention: origin bottom-left. */
export interface DrawPageOptions {
  x: number;
  y: number;
  /** Width in PDF points. Defaults to the embedded page's intrinsic width. */
  width?: number;
  /** Height in PDF points. Defaults to the embedded page's intrinsic height. */
  height?: number;
  /** Constant opacity 0..1 applied to the whole page. Default 1 (opaque). */
  opacity?: number;
  /** Rotation in degrees, counter-clockwise about `(x, y)`. Default 0. */
  rotate?: number;
  /** Horizontal skew in degrees. Default 0. */
  xSkew?: number;
  /** Vertical skew in degrees. Default 0. */
  ySkew?: number;
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

function validateFinite(v: number | undefined, name: string): void {
  if (v !== undefined && !Number.isFinite(v)) {
    throw new RangeError(`${name} must be a finite number, got ${v}`);
  }
}

/** Validate a dash pattern: every entry finite and >= 0; phase finite. */
function validateDash(dash: number[] | undefined, dashPhase: number | undefined): void {
  if (dash !== undefined) {
    for (const v of dash) {
      if (!Number.isFinite(v) || v < 0) {
        throw new RangeError(`dash entries must be finite and >= 0, got ${v}`);
      }
    }
  }
  validateFinite(dashPhase, "dashPhase");
}

/**
 * A page of a PdfDocument. Drawing methods queue operations that are
 * applied when the document is saved.
 */
export class PdfPage {
  /**
   * Stable slot id used to resolve this page's final index at save time.
   * Loaded pages use their original index; appended pages use a negative
   * sentinel; created pages reuse `index`. Draw ops carry this, not `index`,
   * so a later insert/remove/move re-targets draws onto the right page.
   * @internal
   */
  private readonly _slot: number;

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
    slot?: number,
  ) {
    this._slot = slot ?? index;
  }

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
    if (options.rotate !== undefined && !Number.isFinite(options.rotate)) {
      throw new RangeError(`rotate must be a finite number, got ${options.rotate}`);
    }
    validateOpacity(options.opacity);
    if (options.maxWidth !== undefined && (!Number.isFinite(options.maxWidth) || options.maxWidth <= 0)) {
      throw new RangeError(`maxWidth must be > 0, got ${options.maxWidth}`);
    }
    const embeddedId =
      options.font instanceof PdfFont && options.font[kFontId] !== undefined
        ? options.font[kFontId]
        : undefined;
    const fontName =
      embeddedId !== undefined
        ? ""
        : options.font instanceof PdfFont
          ? options.font.name
          : (options.font ?? StandardFonts.Helvetica);
    this.queue.pushText(this._slot, text, {
      x: options.x,
      y: options.y,
      size: options.size,
      font: fontName,
      color: options.color ?? rgb(0, 0, 0),
      lineHeight: options.lineHeight,
      ...(embeddedId !== undefined ? { fontId: embeddedId } : {}),
      ...(options.rotate !== undefined ? { rotate: options.rotate } : {}),
      ...(options.opacity !== undefined ? { opacity: options.opacity } : {}),
      ...(options.maxWidth !== undefined ? { maxWidth: options.maxWidth } : {}),
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
    validateOpacity(options.opacity);
    validateFinite(options.rotate, "rotate");
    validateFinite(options.xSkew, "xSkew");
    validateFinite(options.ySkew, "ySkew");
    this.queue.pushImage(this._slot, image[kImageBytes], {
      x: options.x,
      y: options.y,
      width,
      height,
      ...(options.opacity !== undefined ? { opacity: options.opacity } : {}),
      ...(options.rotate !== undefined ? { rotate: options.rotate } : {}),
      ...(options.xSkew !== undefined ? { xSkew: options.xSkew } : {}),
      ...(options.ySkew !== undefined ? { ySkew: options.ySkew } : {}),
    });
  }

  /**
   * Draw an embedded PDF page on this page at `(x, y)` (bottom-left corner,
   * origin bottom-left). The page must first be embedded via `doc.embedPdfPage()`.
   */
  drawPage(embedded: EmbeddedPdfPage, options: DrawPageOptions): void {
    const width = options.width ?? embedded.width;
    const height = options.height ?? embedded.height;
    for (const [v, name] of [
      [options.x, "x"],
      [options.y, "y"],
      [width, "width"],
      [height, "height"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    if (width <= 0 || height <= 0) throw new RangeError("width and height must be > 0");
    validateOpacity(options.opacity);
    validateFinite(options.rotate, "rotate");
    validateFinite(options.xSkew, "xSkew");
    validateFinite(options.ySkew, "ySkew");
    this.queue.pushPage(this._slot, embedded[kEmbeddedBytes], {
      x: options.x,
      y: options.y,
      width,
      height,
      srcPage: embedded.srcPage,
      ...(options.opacity !== undefined ? { opacity: options.opacity } : {}),
      ...(options.rotate !== undefined ? { rotate: options.rotate } : {}),
      ...(options.xSkew !== undefined ? { xSkew: options.xSkew } : {}),
      ...(options.ySkew !== undefined ? { ySkew: options.ySkew } : {}),
    });
  }

  /**
   * Draw a straight line from `start` to `end`. Coordinates use the PDF
   * convention: origin bottom-left.
   */
  drawLine(options: DrawLineOptions): void {
    const { start, end, strokeWidth, stroke, opacity, dash, dashPhase } = options;
    for (const [v, name] of [
      [start.x, "start.x"],
      [start.y, "start.y"],
      [end.x, "end.x"],
      [end.y, "end.y"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    validateBorderWidth(strokeWidth, "strokeWidth");
    validateOpacity(opacity);
    validateDash(dash, dashPhase);
    const op: LineOp = {
      op: "line",
      page: this._slot,
      x1: start.x,
      y1: start.y,
      x2: end.x,
      y2: end.y,
      ...(strokeWidth !== undefined ? { thickness: strokeWidth } : {}),
      ...(stroke !== undefined ? { color: colorToTuple(stroke) } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
      ...(dash !== undefined ? { dash } : {}),
      ...(dashPhase !== undefined ? { dashPhase } : {}),
    };
    this.queue.pushLine(op);
  }

  /**
   * Draw a rectangle with optional fill and border. `(x, y)` is the lower-left
   * corner. Coordinates use the PDF convention: origin bottom-left.
   */
  drawRectangle(options: DrawRectangleOptions): void {
    const { x, y, width, height, fill, stroke, strokeWidth, opacity, dash, dashPhase } = options;
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
    validateBorderWidth(strokeWidth, "strokeWidth");
    validateOpacity(opacity);
    validateDash(dash, dashPhase);
    const op: RectangleOp = {
      op: "rectangle",
      page: this._slot,
      x,
      y,
      width,
      height,
      ...(fill !== undefined ? { color: colorToTuple(fill) } : {}),
      ...(stroke !== undefined ? { borderColor: colorToTuple(stroke) } : {}),
      ...(strokeWidth !== undefined ? { borderWidth: strokeWidth } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
      ...(dash !== undefined ? { dash } : {}),
      ...(dashPhase !== undefined ? { dashPhase } : {}),
    };
    this.queue.pushRectangle(op);
  }

  /**
   * Set the rotation of the page. Must be a multiple of 90 (0, 90, 180, 270, etc.).
   * Works on both loaded and created documents.
   */
  setRotation(degrees: number): void {
    if (!Number.isFinite(degrees) || degrees % 90 !== 0) {
      throw new InvalidRotationError(degrees);
    }
    this.queue.pushSetRotation(this._slot, degrees);
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
    this.queue.pushSetMediaBox(this._slot, [x0, y0, x1, y1]);
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
   * Draw an ellipse centered at `(x, y)` with horizontal radius `radiusX` and
   * vertical radius `radiusY`. Coordinates use the PDF convention: origin
   * bottom-left.
   */
  drawEllipse(options: DrawEllipseOptions): void {
    const { x, y, radiusX, radiusY, fill, stroke, strokeWidth, opacity, dash, dashPhase } = options;
    for (const [v, name] of [
      [x, "x"],
      [y, "y"],
      [radiusX, "radiusX"],
      [radiusY, "radiusY"],
    ] as const) {
      if (!Number.isFinite(v)) throw new RangeError(`${name} must be a finite number`);
    }
    if (radiusX <= 0) throw new RangeError(`radiusX must be > 0, got ${radiusX}`);
    if (radiusY <= 0) throw new RangeError(`radiusY must be > 0, got ${radiusY}`);
    validateBorderWidth(strokeWidth, "strokeWidth");
    validateOpacity(opacity);
    validateDash(dash, dashPhase);
    const op: EllipseOp = {
      op: "ellipse",
      page: this._slot,
      x,
      y,
      xScale: radiusX,
      yScale: radiusY,
      ...(fill !== undefined ? { color: colorToTuple(fill) } : {}),
      ...(stroke !== undefined ? { borderColor: colorToTuple(stroke) } : {}),
      ...(strokeWidth !== undefined ? { borderWidth: strokeWidth } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
      ...(dash !== undefined ? { dash } : {}),
      ...(dashPhase !== undefined ? { dashPhase } : {}),
    };
    this.queue.pushEllipse(op);
  }

  /**
   * Draw a vector path described by an SVG path `d` string. Supports M/L/H/V/C/S/Q/T/Z
   * commands; arc (A) commands throw an error. Coordinates use the PDF convention:
   * origin bottom-left.
   *
   * @param d - SVG path data string.
   * @param opts - Optional fill, stroke, strokeWidth, and opacity.
   */
  drawSvgPath(d: string, opts: DrawSvgPathOptions = {}): void {
    const { fill, stroke, strokeWidth, opacity, dash, dashPhase } = opts;
    validateOpacity(opacity);
    validateBorderWidth(strokeWidth, "strokeWidth");
    validateDash(dash, dashPhase);
    const segments = parseSvgPath(d);
    const op: PathOp = {
      op: "path",
      page: this._slot,
      segments,
      ...(fill !== undefined ? { fill: colorToTuple(fill) } : {}),
      ...(stroke !== undefined ? { stroke: colorToTuple(stroke) } : {}),
      ...(strokeWidth !== undefined ? { strokeWidth } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
      ...(dash !== undefined ? { dash } : {}),
      ...(dashPhase !== undefined ? { dashPhase } : {}),
    };
    this.queue.pushPath(op);
  }

  /**
   * Draw a polygon from an array of points. Coordinates use the PDF convention:
   * origin bottom-left.
   *
   * @param points - At least 2 points, each `{x, y}`.
   * @param opts - Optional fill, stroke, strokeWidth, opacity, and closed flag.
   */
  drawPolygon(points: { x: number; y: number }[], opts: DrawPolygonOptions = {}): void {
    if (!Array.isArray(points) || points.length < 2) {
      throw new RangeError(`drawPolygon requires at least 2 points, got ${Array.isArray(points) ? points.length : 0}`);
    }
    for (let i = 0; i < points.length; i++) {
      const p = points[i]!;
      if (!Number.isFinite(p.x) || !Number.isFinite(p.y)) {
        throw new RangeError(`points[${i}] coordinates must be finite numbers`);
      }
    }
    const { fill, stroke, strokeWidth, opacity, dash, dashPhase } = opts;
    const closed = opts.closed ?? true;
    validateOpacity(opacity);
    validateBorderWidth(strokeWidth, "strokeWidth");
    validateDash(dash, dashPhase);
    const segments: Segment[] = [
      { t: "m", x: points[0]!.x, y: points[0]!.y },
      ...points.slice(1).map((p) => ({ t: "l" as const, x: p.x, y: p.y })),
      ...(closed ? [{ t: "z" as const }] : []),
    ];
    const op: PathOp = {
      op: "path",
      page: this._slot,
      segments,
      ...(fill !== undefined ? { fill: colorToTuple(fill) } : {}),
      ...(stroke !== undefined ? { stroke: colorToTuple(stroke) } : {}),
      ...(strokeWidth !== undefined ? { strokeWidth } : {}),
      ...(opacity !== undefined ? { opacity } : {}),
      ...(dash !== undefined ? { dash } : {}),
      ...(dashPhase !== undefined ? { dashPhase } : {}),
    };
    this.queue.pushPath(op);
  }

  /**
   * Add a link annotation over a rectangular region on the page. Coordinates
   * use the PDF convention: origin bottom-left. Exactly one of `url` or
   * `goToPage` must be provided.
   */
  drawLink(options: DrawLinkOptions): void {
    const { x, y, width, height, url, goToPage } = options;
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
    if (url === undefined && goToPage === undefined) {
      throw new Error("drawLink requires exactly one of `url` or `goToPage`");
    }
    if (url !== undefined && goToPage !== undefined) {
      throw new Error("drawLink requires exactly one of `url` or `goToPage`");
    }
    const rect: [number, number, number, number] = [x, y, x + width, y + height];
    const op: LinkOp = {
      op: "link",
      page: this._slot,
      rect,
      ...(url !== undefined ? { uri: url } : {}),
      ...(goToPage !== undefined ? { goToPage } : {}),
    };
    this.queue.pushLink(op);
  }
}
