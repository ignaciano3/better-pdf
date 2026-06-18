import { describe, it, expect } from "bun:test";
import { readFileSync } from "fs";
import { PdfDocument, EmbeddedPdfPage } from "../src/index.js";

const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

const TINY_PNG = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
  0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
  0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
  0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
]);

describe("embedPdfPage + drawPage", () => {
  it("created target: embed page 0 from fixture, draw into new doc, save round-trips", async () => {
    const src = readFileSync(FICHA);
    const doc = await PdfDocument.create();
    const e = await doc.embedPdfPage(new Uint8Array(src), 0);
    expect(e).toBeInstanceOf(EmbeddedPdfPage);
    expect(e.width).toBeGreaterThan(0);
    expect(e.height).toBeGreaterThan(0);
    expect(e.srcPage).toBe(0);
    const p = doc.addPage();
    p.drawPage(e, { x: 0, y: 0, width: 300, height: 400 });
    const out = await doc.save();
    const re = await PdfDocument.load(out);
    expect(re.getPageCount()).toBe(1);
  });

  it("loaded target: embed fixture page into loaded doc, save, reload", async () => {
    const srcBytes = new Uint8Array(readFileSync(FICHA));
    const doc = await PdfDocument.load(srcBytes);
    const originalCount = doc.getPageCount();
    const e = await doc.embedPdfPage(srcBytes, 0);
    expect(e.width).toBeGreaterThan(0);
    expect(e.height).toBeGreaterThan(0);
    doc.getPage(0).drawPage(e, { x: 0, y: 0, width: 200, height: 250 });
    const out = await doc.save();
    const re = await PdfDocument.load(out);
    expect(re.getPageCount()).toBe(originalCount);
  });

  it("mix sanity: embed a page AND drawImage in same doc → save → reload valid", async () => {
    const srcBytes = new Uint8Array(readFileSync(FICHA));
    const doc = await PdfDocument.create();
    const e = await doc.embedPdfPage(srcBytes, 0);
    const img = await doc.embedPng(TINY_PNG);
    const p = doc.addPage();
    p.drawPage(e, { x: 0, y: 0, width: 200, height: 200 });
    p.drawImage(img, { x: 210, y: 0, width: 50, height: 50 });
    const out = await doc.save();
    const re = await PdfDocument.load(out);
    expect(re.getPageCount()).toBe(1);
  });

  it("drawPage defaults width/height to intrinsic", async () => {
    const srcBytes = new Uint8Array(readFileSync(FICHA));
    const doc = await PdfDocument.create();
    const e = await doc.embedPdfPage(srcBytes, 0);
    const p = doc.addPage();
    // No width/height — should not throw
    expect(() => p.drawPage(e, { x: 0, y: 0 })).not.toThrow();
  });

  it("drawPage validates bad dimensions", async () => {
    const srcBytes = new Uint8Array(readFileSync(FICHA));
    const doc = await PdfDocument.create();
    const e = await doc.embedPdfPage(srcBytes, 0);
    const p = doc.addPage();
    expect(() => p.drawPage(e, { x: 0, y: 0, width: 0, height: 100 })).toThrow(RangeError);
    expect(() => p.drawPage(e, { x: 0, y: 0, width: 100, height: -1 })).toThrow(RangeError);
    expect(() => p.drawPage(e, { x: NaN, y: 0, width: 100, height: 100 })).toThrow(RangeError);
  });

  it("embedPdfPage out of range throws", async () => {
    const srcBytes = new Uint8Array(readFileSync(FICHA));
    const doc = await PdfDocument.create();
    await expect(doc.embedPdfPage(srcBytes, 9999)).rejects.toThrow();
  });
});
