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

export type AddPageOp = { op: "addPage"; width: number; height: number };

type ImageEntry = {
  kind: "image";
  bytes: Uint8Array;
  op: Omit<ImageOp, "imageOffset" | "imageLength">;
};

/** @internal */
export class DrawQueue {
  private readonly drawOps: Array<TextOp | ImageEntry> = [];
  private readonly pageOps: AddPageOp[] = [];

  get length(): number {
    return this.drawOps.length;
  }

  pushText(
    page: number,
    text: string,
    opts: { x: number; y: number; size: number; font: string; color: Color; lineHeight?: number },
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
    });
  }

  pushAddPage(width: number, height: number): void {
    this.pageOps.push({ op: "addPage", width, height });
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

  private buildDrawOps(): { ops: (TextOp | ImageOp)[]; images: Uint8Array } {
    const chunks: Uint8Array[] = [];
    let offset = 0;
    const ops: (TextOp | ImageOp)[] = [];
    for (const entry of this.drawOps) {
      if ("kind" in entry) {
        const len = entry.bytes.length;
        ops.push({ ...entry.op, imageOffset: offset, imageLength: len });
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

  toDrawPayload(): { opsJson: string; images: Uint8Array } {
    const { ops, images } = this.buildDrawOps();
    return { opsJson: JSON.stringify(ops), images };
  }

  toCreatePayload(): { opsJson: string; images: Uint8Array } {
    const { ops, images } = this.buildDrawOps();
    return { opsJson: JSON.stringify([...this.pageOps, ...ops]), images };
  }
}
