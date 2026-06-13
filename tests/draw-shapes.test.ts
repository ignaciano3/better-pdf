import { describe, expect, test } from "bun:test";
import { PdfDocument, rgb, PageSizes } from "../src/index.ts";

const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

describe("drawLine", () => {
  test("line_on_created_page", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawLine({
      start: { x: 50, y: 100 },
      end: { x: 250, y: 100 },
      thickness: 2,
      color: rgb(1, 0, 0),
    });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain(" l");
    expect(s).toContain("S");
  });
});

describe("drawRectangle", () => {
  test("rectangle_on_loaded_page", async () => {
    const original = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
    const doc = await PdfDocument.load(original);
    doc.getPage(0).drawRectangle({
      x: 50,
      y: 100,
      width: 200,
      height: 80,
      color: rgb(0.9, 0.9, 0.9),
      borderColor: rgb(0, 0, 0),
      borderWidth: 1,
    });
    const out = await doc.save();
    expect(out.length).toBeGreaterThan(original.length);
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(doc.getPageCount());
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain(" re");
  });
});

describe("drawEllipse", () => {
  test("ellipse_on_created_page", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawEllipse({
      x: 150,
      y: 140,
      xScale: 100,
      yScale: 40,
      color: rgb(0, 0, 1),
    });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain(" c");
  });
});

describe("opacity", () => {
  test("opacity_round_trips", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawRectangle({
      x: 10,
      y: 10,
      width: 100,
      height: 50,
      color: rgb(0.5, 0.5, 0.5),
      opacity: 0.5,
    });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain(" gs");
  });
});

describe("shapes validation", () => {
  test("drawRectangle width 0 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.getPage(0).drawRectangle({ x: 0, y: 0, width: 0, height: 10 }),
    ).toThrow(RangeError);
  });

  test("drawEllipse xScale 0 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.getPage(0).drawEllipse({ x: 0, y: 0, xScale: 0, yScale: 10 }),
    ).toThrow(RangeError);
  });

  test("drawRectangle opacity 2 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.getPage(0).drawRectangle({ x: 0, y: 0, width: 10, height: 10, opacity: 2 }),
    ).toThrow(RangeError);
  });

  test("drawLine thickness -1 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.getPage(0).drawLine({ start: { x: 0, y: 0 }, end: { x: 10, y: 10 }, thickness: -1 }),
    ).toThrow(RangeError);
  });
});

describe("shapes compose with text", () => {
  test("shapes_compose_with_text", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const p = doc.getPage(0);
    p.drawText("T", { x: 10, y: 10, size: 10 });
    p.drawRectangle({ x: 20, y: 20, width: 30, height: 30, color: rgb(0, 0, 0) });
    const out = await doc.save();
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain("(T) Tj");
    expect(s).toContain(" re");
  });
});
