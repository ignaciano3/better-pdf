import { describe, expect, test } from "bun:test";
import { PdfDocument, StandardFonts } from "../src/index.ts";

describe("text measurement", () => {
  test("widthOfTextAtSize positive and scales with size", async () => {
    const d = await PdfDocument.create();
    const font = d.getFont(StandardFonts.Helvetica);
    const w12 = font.widthOfTextAtSize("Hello", 12);
    expect(w12).toBeGreaterThan(0);
    const w24 = font.widthOfTextAtSize("Hello", 24);
    expect(Math.abs(w24 - 2 * w12)).toBeLessThan(0.01);
  });
  test("empty string is zero width", async () => {
    const d = await PdfDocument.create();
    expect(d.getFont(StandardFonts.Courier).widthOfTextAtSize("", 12)).toBe(0);
  });
  test("Courier is monospaced", async () => {
    const d = await PdfDocument.create();
    const f = d.getFont(StandardFonts.Courier);
    expect(f.widthOfTextAtSize("MM", 10)).toBeCloseTo(2 * f.widthOfTextAtSize("M", 10), 5);
  });
  test("widthOfTextAtSize rejects non-positive size", async () => {
    const d = await PdfDocument.create();
    const f = d.getFont(StandardFonts.Helvetica);
    expect(() => f.widthOfTextAtSize("x", 0)).toThrow(RangeError);
  });
  test("getFont works on a loaded document too", async () => {
    const bytes = new Uint8Array(await Bun.file("tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf").arrayBuffer());
    const loaded = await PdfDocument.load(bytes);
    expect(loaded.getFont(StandardFonts.Helvetica).widthOfTextAtSize("Hi", 12)).toBeGreaterThan(0);
  });
  test("a PdfFont can be passed to drawText", async () => {
    const d = await PdfDocument.create();
    const font = d.getFont(StandardFonts.TimesBold);
    d.addPage().drawText("Times", { x: 10, y: 10, size: 12, font });
    const out = await d.save();
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain("(Times) Tj");
    expect(s).toContain("Times-Bold");
  });
});
