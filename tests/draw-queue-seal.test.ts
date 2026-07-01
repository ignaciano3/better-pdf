import { describe, expect, test } from "bun:test";
import { DrawQueue } from "../src/generate/draw-queue.ts";
import { FormSealedError } from "../src/index.ts";

describe("DrawQueue.seal", () => {
  test("push after seal throws FormSealedError", () => {
    const q = new DrawQueue();
    q.pushAddPage(100, 200); // ok before seal
    q.seal();
    expect(() =>
      q.pushText(0, "x", { x: 0, y: 0, size: 12, font: "Helvetica", color: { red: 0, green: 0, blue: 0 } }),
    ).toThrow(FormSealedError);
    expect(() => q.pushAddPage(1, 1)).toThrow(FormSealedError);
  });

  test("serialization still works after seal", () => {
    const q = new DrawQueue();
    q.pushAddPage(100, 200);
    q.seal();
    const payload = q.toCreatePayload();
    expect(payload.opsJson).toContain("addPage");
  });
});
