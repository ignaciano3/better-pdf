import { describe, expect, test } from "bun:test";
import { PdfDocument } from "../src/index.ts";

describe("stream compression", () => {
  test("create: compressed output is smaller and still valid", async () => {
    async function build(compress: boolean) {
      const doc = await PdfDocument.create();
      const page = doc.addPage();
      // repetitive text compresses well
      for (let i = 0; i < 200; i++) {
        page.drawText("The quick brown fox jumps over the lazy dog.", {
          x: 50,
          y: 700 - (i % 40) * 15,
          size: 10,
        });
      }
      return doc.save({ compress });
    }
    const compressed = await build(true);
    const raw = await build(false);
    expect(compressed.length).toBeLessThan(raw.length);
    // Both remain valid PDFs.
    expect(new TextDecoder().decode(compressed.slice(0, 5))).toBe("%PDF-");
    expect(new TextDecoder().decode(raw.slice(0, 5))).toBe("%PDF-");
  });

  test("default is compressed", async () => {
    async function build(options?: { compress: boolean }) {
      const doc = await PdfDocument.create();
      const page = doc.addPage();
      for (let i = 0; i < 200; i++) {
        page.drawText("compress me compress me compress me", { x: 40, y: 60 + i, size: 8 });
      }
      return options ? doc.save(options) : doc.save();
    }
    const dflt = await build();
    const raw = await build({ compress: false });
    expect(dflt.length).toBeLessThan(raw.length);
  });

  test("round-trip: compressed load-path draw is reloadable", async () => {
    const seed = await PdfDocument.create();
    seed.addPage();
    const base = await seed.save();

    const doc = await PdfDocument.load(base);
    doc.getPage(0).drawText("stamped", { x: 50, y: 50, size: 12 });
    const out = await doc.save({ compress: true });
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });
});

describe("object streams", () => {
  async function manyPages(objectStreams: boolean) {
    const doc = await PdfDocument.create();
    for (let p = 0; p < 30; p++) {
      const page = doc.addPage();
      page.drawText("page " + p, { x: 40, y: 700, size: 12 });
    }
    return doc.save({ objectStreams });
  }

  test("create: objectStreams shrinks a multi-page doc and reloads", async () => {
    const packed = await manyPages(true);
    const plain = await manyPages(false);
    expect(packed.length).toBeLessThan(plain.length);
    expect(new TextDecoder().decode(packed.slice(0, 5))).toBe("%PDF-");
    const reloaded = await PdfDocument.load(packed);
    expect(reloaded.getPageCount()).toBe(30);
  });

  test("default is off (byte-identical to no option)", async () => {
    const a = await manyPages(false);
    const doc = await PdfDocument.create();
    for (let p = 0; p < 30; p++) doc.addPage().drawText("page " + p, { x: 40, y: 700, size: 12 });
    const b = await doc.save();
    expect(b.length).toBe(a.length);
  });

  test("merge: objectStreams shrinks and reloads", async () => {
    const base = await (async () => {
      const d = await PdfDocument.create();
      for (let p = 0; p < 10; p++) d.addPage().drawText("x", { x: 10, y: 10, size: 8 });
      return d.save();
    })();
    const packed = await PdfDocument.merge([base, base], { objectStreams: true });
    const plain = await PdfDocument.merge([base, base]);
    expect(packed.length).toBeLessThan(plain.length);
    const reloaded = await PdfDocument.load(packed);
    expect(reloaded.getPageCount()).toBe(20);
  });

  test("objectStreams is a no-op on loaded-document (incremental) save", async () => {
    const seed = await PdfDocument.create();
    seed.addPage();
    const base = await seed.save();
    async function stamp(objectStreams: boolean) {
      const doc = await PdfDocument.load(base);
      doc.getPage(0).drawText("stamp", { x: 50, y: 50, size: 12 });
      return doc.save({ objectStreams });
    }
    const withFlag = await stamp(true);
    const without = await stamp(false);
    expect(withFlag.length).toBe(without.length);
  });
});
