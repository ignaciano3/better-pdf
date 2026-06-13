import { describe, expect, test } from "bun:test";
import * as gen from "../src/generate/index.ts";

describe("generate entry", () => {
  test("exports the drawing surface", () => {
    expect(gen.PdfPage).toBeDefined();
    expect(gen.StandardFonts).toBeDefined();
    expect(gen.rgb).toBeDefined();
    expect(gen.grayscale).toBeDefined();
    expect(gen.PageOutOfRangeError).toBeDefined();
  });

  test("runtime-neutral: no PdfDocument or WASM bindings", () => {
    expect("PdfDocument" in gen).toBe(false);
    expect("initializeWasm" in gen).toBe(false);
  });
});
