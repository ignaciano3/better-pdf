import type { Color } from "./color.js";

/** @internal Wire format consumed by the Rust core's apply_draw_ops. */
export type TextOp = {
  op: "text";
  page: number;
  x: number;
  y: number;
  size: number;
  font: string;
  color: [number, number, number];
  text: string;
  lineHeight?: number;
  fontId?: number;
};

export type ImageOp = {
  op: "image";
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
  imageOffset: number;
  imageLength: number;
};

export type PageOp = {
  op: "page";
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
  imageOffset: number;
  imageLength: number;
  srcPage: number;
};

export type LineOp = {
  op: "line";
  page: number;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  thickness?: number;
  color?: [number, number, number];
  opacity?: number;
};

export type RectangleOp = {
  op: "rectangle";
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
  color?: [number, number, number];
  borderColor?: [number, number, number];
  borderWidth?: number;
  opacity?: number;
};

export type EllipseOp = {
  op: "ellipse";
  page: number;
  x: number;
  y: number;
  xScale: number;
  yScale: number;
  color?: [number, number, number];
  borderColor?: [number, number, number];
  borderWidth?: number;
  opacity?: number;
};

export type SetRotationOp = { op: "setRotation"; page: number; degrees: number };

export type SetMediaBoxOp = { op: "setMediaBox"; page: number; box: [number, number, number, number] };

export type LinkOp = {
  op: "link";
  page: number;
  rect: [number, number, number, number];
  uri?: string;
  goToPage?: number;
};

export type AddPageOp = { op: "addPage"; width: number; height: number };

type ImageEntry = {
  kind: "image";
  bytes: Uint8Array;
  op: Omit<ImageOp, "imageOffset" | "imageLength">;
};

type PageEntry = {
  kind: "page";
  bytes: Uint8Array;
  op: Omit<PageOp, "imageOffset" | "imageLength">;
};

type FontEntry = { bytes: Uint8Array; subset: boolean };

/** @internal */
export class DrawQueue {
  private readonly drawOps: Array<TextOp | ImageEntry | PageEntry | LineOp | RectangleOp | EllipseOp | SetRotationOp | SetMediaBoxOp | LinkOp> = [];
  private readonly pageOps: AddPageOp[] = [];
  private readonly fonts: FontEntry[] = [];
  private metadataOp: Record<string, string> | undefined = undefined;

  get length(): number {
    return this.drawOps.length;
  }

  /** Register an embedded font's bytes; returns its index for use as `fontId` on text ops. */
  registerFont(bytes: Uint8Array, subset: boolean): number {
    const id = this.fonts.length;
    this.fonts.push({ bytes, subset });
    return id;
  }

  pushText(
    page: number,
    text: string,
    opts: {
      x: number;
      y: number;
      size: number;
      font: string;
      color: Color;
      lineHeight?: number;
      fontId?: number;
    },
  ): void {
    this.drawOps.push({
      op: "text",
      page,
      x: opts.x,
      y: opts.y,
      size: opts.size,
      font: opts.font,
      color: [opts.color.red, opts.color.green, opts.color.blue],
      text,
      ...(opts.lineHeight !== undefined ? { lineHeight: opts.lineHeight } : {}),
      ...(opts.fontId !== undefined ? { fontId: opts.fontId } : {}),
    });
  }

  pushAddPage(width: number, height: number): void {
    this.pageOps.push({ op: "addPage", width, height });
  }

  pushMetadata(meta: Record<string, string>): void {
    this.metadataOp = meta;
  }

  pushImage(
    page: number,
    bytes: Uint8Array,
    opts: { x: number; y: number; width: number; height: number },
  ): void {
    this.drawOps.push({
      kind: "image",
      bytes,
      op: { op: "image", page, x: opts.x, y: opts.y, width: opts.width, height: opts.height },
    });
  }

  pushPage(
    page: number,
    bytes: Uint8Array,
    opts: { x: number; y: number; width: number; height: number; srcPage: number },
  ): void {
    this.drawOps.push({
      kind: "page",
      bytes,
      op: { op: "page", page, x: opts.x, y: opts.y, width: opts.width, height: opts.height, srcPage: opts.srcPage },
    });
  }

  pushLine(op: LineOp): void {
    this.drawOps.push(op);
  }

  pushRectangle(op: RectangleOp): void {
    this.drawOps.push(op);
  }

  pushEllipse(op: EllipseOp): void {
    this.drawOps.push(op);
  }

  pushSetRotation(page: number, degrees: number): void {
    this.drawOps.push({ op: "setRotation", page, degrees });
  }

  pushSetMediaBox(page: number, box: [number, number, number, number]): void {
    this.drawOps.push({ op: "setMediaBox", page, box });
  }

  pushLink(op: LinkOp): void {
    this.drawOps.push(op);
  }

  private buildDrawOps(): { ops: (TextOp | ImageOp | PageOp | LineOp | RectangleOp | EllipseOp | SetRotationOp | SetMediaBoxOp | LinkOp)[]; images: Uint8Array } {
    const chunks: Uint8Array[] = [];
    let offset = 0;
    const ops: (TextOp | ImageOp | PageOp | LineOp | RectangleOp | EllipseOp | SetRotationOp | SetMediaBoxOp | LinkOp)[] = [];
    for (const entry of this.drawOps) {
      if ("kind" in entry) {
        const len = entry.bytes.length;
        ops.push({ ...entry.op, imageOffset: offset, imageLength: len } as ImageOp | PageOp);
        chunks.push(entry.bytes);
        offset += len;
      } else {
        ops.push(entry);
      }
    }
    const images = new Uint8Array(offset);
    let pos = 0;
    for (const c of chunks) {
      images.set(c, pos);
      pos += c.length;
    }
    return { ops, images };
  }

  private buildFonts(): { fonts: Uint8Array; fontsJson: string } {
    const chunks: Uint8Array[] = [];
    let offset = 0;
    const table = this.fonts.map((f) => {
      const entry = { offset, length: f.bytes.length, subset: f.subset };
      chunks.push(f.bytes);
      offset += f.bytes.length;
      return entry;
    });
    const fonts = new Uint8Array(offset);
    let pos = 0;
    for (const c of chunks) {
      fonts.set(c, pos);
      pos += c.length;
    }
    return { fonts, fontsJson: JSON.stringify(table) };
  }

  toDrawPayload(): { opsJson: string; images: Uint8Array; fonts: Uint8Array; fontsJson: string } {
    const { ops, images } = this.buildDrawOps();
    const { fonts, fontsJson } = this.buildFonts();
    return { opsJson: JSON.stringify(ops), images, fonts, fontsJson };
  }

  toCreatePayload(): { opsJson: string; images: Uint8Array; fonts: Uint8Array; fontsJson: string } {
    const { ops, images } = this.buildDrawOps();
    const { fonts, fontsJson } = this.buildFonts();
    const metaOps = this.metadataOp ? [{ op: "metadata", ...this.metadataOp }] : [];
    return { opsJson: JSON.stringify([...metaOps, ...this.pageOps, ...ops]), images, fonts, fontsJson };
  }
}
