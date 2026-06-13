import type { Color } from "./color.js";

/** @internal Wire format consumed by the Rust core's apply_draw_ops. */
export type DrawOp = {
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

export type AddPageOp = { op: "addPage"; width: number; height: number };

/** @internal */
export class DrawQueue {
  private readonly ops: DrawOp[] = [];
  private readonly pageOps: AddPageOp[] = [];

  get length(): number {
    return this.ops.length;
  }

  pushText(
    page: number,
    text: string,
    opts: { x: number; y: number; size: number; font: string; color: Color; lineHeight?: number },
  ): void {
    this.ops.push({
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

  toJson(): string {
    return JSON.stringify(this.ops);
  }

  /** Ops for create_document: addPage ops first, then all text ops. */
  toCreateJson(): string {
    return JSON.stringify([...this.pageOps, ...this.ops]);
  }
}
