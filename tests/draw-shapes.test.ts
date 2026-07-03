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
      strokeWidth: 2,
      stroke: rgb(1, 0, 0),
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
      fill: rgb(0.9, 0.9, 0.9),
      stroke: rgb(0, 0, 0),
      strokeWidth: 1,
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
      radiusX: 100,
      radiusY: 40,
      fill: rgb(0, 0, 1),
    });
    // Inspect the raw content stream for the bezier operator, so opt out of
    // the default deflate compression.
    const out = await doc.save({ compress: false });
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
      fill: rgb(0.5, 0.5, 0.5),
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

  test("drawEllipse radiusX 0 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.getPage(0).drawEllipse({ x: 0, y: 0, radiusX: 0, radiusY: 10 }),
    ).toThrow(RangeError);
  });

  test("drawRectangle opacity 2 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.getPage(0).drawRectangle({ x: 0, y: 0, width: 10, height: 10, opacity: 2 }),
    ).toThrow(RangeError);
  });

  test("drawLine strokeWidth -1 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.getPage(0).drawLine({ start: { x: 0, y: 0 }, end: { x: 10, y: 10 }, strokeWidth: -1 }),
    ).toThrow(RangeError);
  });
});

describe("shapes compose with text", () => {
  test("shapes_compose_with_text", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const p = doc.getPage(0);
    p.drawText("T", { x: 10, y: 10, size: 10 });
    p.drawRectangle({ x: 20, y: 20, width: 30, height: 30, fill: rgb(0, 0, 0) });
    const out = await doc.save();
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain("(T) Tj");
    expect(s).toContain(" re");
  });
});

describe("dashed strokes", () => {
  test("rectangle border dash emits a dash op", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawRectangle({
      x: 20, y: 20, width: 100, height: 50,
      stroke: rgb(0, 0, 0), strokeWidth: 1, dash: [4, 2],
    });
    const s = new TextDecoder("latin1").decode(await doc.save());
    expect(s).toContain("[4 2] 0 d");
  });

  test("line dash with phase", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawLine({
      start: { x: 0, y: 0 }, end: { x: 100, y: 0 },
      stroke: rgb(0, 0, 0), dash: [6, 3], dashPhase: 1.5,
    });
    const s = new TextDecoder("latin1").decode(await doc.save());
    expect(s).toContain("[6 3] 1.5 d");
  });

  test("solid line emits no dash op", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawLine({ start: { x: 0, y: 0 }, end: { x: 100, y: 0 }, stroke: rgb(0, 0, 0) });
    const s = new TextDecoder("latin1").decode(await doc.save());
    expect(s).not.toContain(" d\n");
  });

  test("negative dash entry throws", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() => doc.getPage(0).drawLine({
      start: { x: 0, y: 0 }, end: { x: 10, y: 0 }, stroke: rgb(0, 0, 0), dash: [4, -1],
    })).toThrow(RangeError);
  });

  test("dash on loaded page round-trips", async () => {
    const { readFile } = await import("node:fs/promises");
    const src = await readFile(FICHA);
    const doc = await PdfDocument.load(new Uint8Array(src));
    doc.getPage(0).drawRectangle({
      x: 50, y: 50, width: 100, height: 50,
      stroke: rgb(0, 0, 0), strokeWidth: 1, dash: [5, 5],
    });
    const s = new TextDecoder("latin1").decode(await doc.save());
    expect(s).toContain("[5 5] 0 d");
  });
});
