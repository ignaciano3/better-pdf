import { describe, expect, test } from "bun:test";
import { PdfDocument, PageSizes } from "../src/index.ts";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

describe("drawText rotate/opacity — created doc", () => {
  test("rotate round-trips (created doc saves/reloads valid)", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4).drawText("Rotated", { x: 100, y: 400, size: 14, rotate: 45 });
    const out = await doc.save();
    expect(new TextDecoder("latin1").decode(out).slice(0, 5)).toBe("%PDF-");
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });

  test("opacity round-trips (created doc saves/reloads valid)", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4).drawText("Faded", { x: 100, y: 400, size: 14, opacity: 0.3 });
    const out = await doc.save();
    expect(new TextDecoder("latin1").decode(out).slice(0, 5)).toBe("%PDF-");
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });
});

describe("drawText rotate/opacity — loaded doc", () => {
  test("rotate on loaded doc saves/reloads valid", async () => {
    const bytes = new Uint8Array(await Bun.file(FIXTURE).arrayBuffer());
    const doc = await PdfDocument.load(bytes);
    const pageCount = doc.getPageCount();
    doc.getPage(0).drawText("Angled", { x: 50, y: 700, size: 12, rotate: 30 });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(pageCount);
  });
});

describe("drawText validation", () => {
  test("opacity out of range throws RangeError", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage(PageSizes.A4);
    expect(() => page.drawText("x", { x: 10, y: 10, size: 12, opacity: 1.5 })).toThrow(RangeError);
    expect(() => page.drawText("x", { x: 10, y: 10, size: 12, opacity: -0.1 })).toThrow(RangeError);
  });

  test("non-finite rotate throws RangeError", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage(PageSizes.A4);
    expect(() => page.drawText("x", { x: 10, y: 10, size: 12, rotate: NaN })).toThrow(RangeError);
    expect(() => page.drawText("x", { x: 10, y: 10, size: 12, rotate: Infinity })).toThrow(RangeError);
  });
});
