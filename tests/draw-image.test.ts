import { describe, it, expect } from "bun:test";
import { PdfDocument, InvalidImageError, PdfImage, rgb } from "../src/index.js";

const TINY_PNG = new Uint8Array([
  0x89,0x50,0x4e,0x47,0x0d,0x0a,0x1a,0x0a,0x00,0x00,0x00,0x0d,0x49,0x48,0x44,0x52,
  0x00,0x00,0x00,0x01,0x00,0x00,0x00,0x01,0x08,0x06,0x00,0x00,0x00,0x1f,0x15,0xc4,
  0x89,0x00,0x00,0x00,0x0d,0x49,0x44,0x41,0x54,0x78,0xda,0x63,0xf8,0xcf,0xc0,0xf0,
  0x1f,0x00,0x05,0x00,0x01,0xff,0x89,0x99,0x3d,0x1d,0x00,0x00,0x00,0x00,0x49,0x45,
  0x4e,0x44,0xae,0x42,0x60,0x82,
]);
const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

describe("PdfImage", () => {
  it("embedPng returns intrinsic size 1x1", async () => {
    const doc = await PdfDocument.create();
    const img = await doc.embedPng(TINY_PNG);
    expect(img.width).toBe(1);
    expect(img.height).toBe(1);
    expect(img).toBeInstanceOf(PdfImage);
  });

  it("embed rejects non-image bytes", async () => {
    const doc = await PdfDocument.create();
    await expect(doc.embedPng(new Uint8Array([1,2,3]))).rejects.toThrow(InvalidImageError);
  });

  it("drawImage on loaded page round-trips", async () => {
    const { readFile } = await import("fs/promises");
    const src = await readFile(FICHA);
    const doc = await PdfDocument.load(new Uint8Array(src));
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80 });
    const out = await doc.save();
    expect(out.length).toBeGreaterThan(src.length);
    const doc2 = await PdfDocument.load(out);
    expect(doc2.getPageCount()).toBe(doc.getPageCount());
    // check content stream has image draw op "Do"
    const str = Array.from(out).map(b => String.fromCharCode(b)).join("");
    expect(str).toContain(" Do");
  });

  it("drawImage on created page", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 10, y: 10, width: 50, height: 50 });
    const out = await doc.save();
    const doc2 = await PdfDocument.load(out);
    expect(doc2.getPageCount()).toBe(1);
    const str = Array.from(out).map(b => String.fromCharCode(b)).join("");
    expect(str).toContain(" Do");
  });

  it("drawImage default size uses intrinsic", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    // No width/height — should not throw
    expect(() => doc.getPage(0).drawImage(img, { x: 0, y: 0 })).not.toThrow();
  });

  it("drawImage validates width 0 throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    expect(() => doc.getPage(0).drawImage(img, { x: 0, y: 0, width: 0, height: 50 })).toThrow(RangeError);
  });

  it("drawImage validates non-finite x throws RangeError", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    expect(() => doc.getPage(0).drawImage(img, { x: NaN, y: 0, width: 50, height: 50 })).toThrow(RangeError);
  });

  it("scale helper", async () => {
    const doc = await PdfDocument.create();
    const img = await doc.embedPng(TINY_PNG);
    expect(img.scale(0.5)).toEqual({ width: 0.5, height: 0.5 });
  });

  it("text+image order preserved on same page", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawText("hello", { x: 10, y: 700, size: 12, color: rgb(0, 0, 0) });
    doc.getPage(0).drawImage(img, { x: 10, y: 600, width: 50, height: 50 });
    const out = await doc.save();
    const str = Array.from(out).map(b => String.fromCharCode(b)).join("");
    expect(str).toContain("hello");
    expect(str).toContain(" Do");
  });

  it("drawImage opacity emits an ExtGState gs op on a created page", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 10, y: 10, width: 50, height: 50, opacity: 0.5 });
    const out = await doc.save();
    const str = Array.from(out).map(b => String.fromCharCode(b)).join("");
    expect(str).toContain(" gs");
    expect(str).toContain("/ca 0.5");
  });

  it("drawImage opacity round-trips on a loaded page", async () => {
    const { readFile } = await import("fs/promises");
    const src = await readFile(FICHA);
    const doc = await PdfDocument.load(new Uint8Array(src));
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80, opacity: 0.3 });
    const out = await doc.save();
    const str = Array.from(out).map(b => String.fromCharCode(b)).join("");
    expect(str).toContain(" gs");
    expect(str).toContain("/ca 0.3");
  });

  it("drawImage without opacity emits no gs op", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 10, y: 10, width: 50, height: 50 });
    const out = await doc.save();
    const str = Array.from(out).map(b => String.fromCharCode(b)).join("");
    expect(str).not.toContain(" gs");
  });

  it("drawImage rejects opacity out of range", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    expect(() => doc.getPage(0).drawImage(img, { x: 0, y: 0, width: 10, height: 10, opacity: 1.5 }))
      .toThrow(RangeError);
  });

  it("drawImage rotate emits a rotation matrix", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80, rotate: 90 });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("0 1 -1 0 0 0 cm");
    expect(str).toContain("1 0 0 1 50 50 cm");
  });

  it("drawImage with no transform uses a single combined cm", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80 });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("100 0 0 80 50 50 cm");
  });

  it("drawImage flipX emits a horizontal mirror inside the same box", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80, flipX: true });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("1 0 0 1 50 50 cm");
    expect(str).toContain("-1 0 0 1 100 0 cm");
    expect(str).toContain("100 0 0 80 0 0 cm");
  });

  it("drawImage flipY emits a vertical mirror inside the same box", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80, flipY: true });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("1 0 0 -1 0 80 cm");
  });

  it("drawImage flipX+flipY mirrors both axes", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 0, y: 0, width: 10, height: 20, flipX: true, flipY: true });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("-1 0 0 -1 10 20 cm");
  });

  it("drawImage flip combines with rotate", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 5, y: 5, width: 10, height: 20, rotate: 90, flipX: true });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("0 1 -1 0 0 0 cm");
    expect(str).toContain("-1 0 0 1 10 0 cm");
  });

  it("drawImage flip false keeps the collapsed single cm", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80, flipX: false, flipY: false });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("100 0 0 80 50 50 cm");
  });

  it("drawImage flipX on a loaded page round-trips", async () => {
    const { readFile } = await import("fs/promises");
    const src = await readFile(FICHA);
    const doc = await PdfDocument.load(new Uint8Array(src));
    const img = await doc.embedPng(TINY_PNG);
    doc.getPage(0).drawImage(img, { x: 50, y: 50, width: 100, height: 80, flipX: true });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("-1 0 0 1 100 0 cm");
    const doc2 = await PdfDocument.load(out);
    expect(doc2.getPageCount()).toBe(doc.getPageCount());
  });

  it("drawImage rejects non-finite rotate", async () => {
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    const img = await doc.embedPng(TINY_PNG);
    expect(() => doc.getPage(0).drawImage(img, { x: 0, y: 0, width: 10, height: 10, rotate: Infinity }))
      .toThrow(RangeError);
  });
});
