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

/** @internal */
export class DrawQueue {
  private readonly ops: DrawOp[] = [];

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

  toJson(): string {
    return JSON.stringify(this.ops);
  }
}
