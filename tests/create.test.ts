import { describe, expect, test } from "bun:test";
import { PdfDocument, PageSizes, StandardFonts, rgb } from "../src/index.ts";

describe("create", () => {
  test("create empty doc with one page", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage(PageSizes.A4);
    expect(page.index).toBe(0);
    expect(Math.round(page.width)).toBe(595);
    expect(Math.round(page.height)).toBe(842);
    expect(doc.getPageCount()).toBe(1);
    const out = await doc.save();
    expect(new TextDecoder("latin1").decode(out).slice(0, 5)).toBe("%PDF-");
  });

  test("default page size is A4", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage();
    expect(Math.round(page.width)).toBe(595);
    expect(Math.round(page.height)).toBe(842);
  });

  test("custom size tuple", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage([200, 300]);
    expect(page.width).toBe(200);
    expect(page.height).toBe(300);
  });

  test("draw text on created page round-trips", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.Letter).drawText("Hello PDF", {
      x: 72, y: 700, size: 18, font: StandardFonts.HelveticaBold, color: rgb(0, 0, 0),
    });
    const out = await doc.save();
    expect(new TextDecoder("latin1").decode(out)).toContain("(Hello PDF) Tj");
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });

  test("multiple pages", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.addPage(PageSizes.A4);
    doc.getPage(0).drawText("p0", { x: 10, y: 10, size: 10 });
    doc.getPage(1).drawText("p1", { x: 10, y: 10, size: 10 });
    expect(doc.getPageCount()).toBe(2);
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(2);
  });

  test("addPage on a loaded doc returns a drawable page", async () => {
    const bytes = new Uint8Array(
      await Bun.file("tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf").arrayBuffer(),
    );
    const doc = await PdfDocument.load(bytes);
    const n = doc.getPageCount();
    const page = doc.addPage(PageSizes.A4);
    expect(page).toBeDefined();
    expect(doc.getPageCount()).toBe(n + 1);
  });

  test("save with no pages throws", async () => {
    const doc = await PdfDocument.create();
    await expect(doc.save()).rejects.toThrow();
  });
});

describe("create mode guards", () => {
  // getForm() on a created document now materializes the document instead of
  // throwing — see tests/created-form-getform.test.ts for coverage.
  test("insertPage on a created doc throws PdfError", async () => {
    const { PdfError } = await import("../src/index.ts");
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() => doc.insertPage(0)).toThrow(PdfError);
  });
});
