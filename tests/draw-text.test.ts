import { describe, expect, test } from "bun:test";
import { PdfDocument, rgb, StandardFonts } from "../src/index.ts";

const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

async function load() {
  const bytes = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
  return PdfDocument.load(bytes);
}

describe("drawText", () => {
  test("save appends incremental update containing the text", async () => {
    const doc = await load();
    const original = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
    doc.getPage(0).drawText("STAMPED", { x: 50, y: 700, size: 24 });
    const out = await doc.save();
    expect(out.length).toBeGreaterThan(original.length);
    expect(out.slice(0, original.length)).toEqual(original);
    expect(new TextDecoder("latin1").decode(out)).toContain("(STAMPED) Tj");
  });

  test("no draw ops -> save returns copy of original", async () => {
    const doc = await load();
    doc.getPage(0); // page access alone must not dirty the doc
    const out = await doc.save();
    const original = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
    expect(out).toEqual(original);
  });

  test("draw options: font, color, multiline", async () => {
    const doc = await load();
    doc.getPage(0).drawText("line1\nline2", {
      x: 40, y: 650, size: 12,
      font: StandardFonts.TimesRoman,
      color: rgb(1, 0, 0),
      lineHeight: 14,
    });
    const out = await doc.save();
    const s = new TextDecoder("latin1").decode(out);
    expect(s).toContain("(line1) Tj");
    expect(s).toContain("(line2) Tj");
    expect(s).toContain("Times-Roman");
  });

  test("composes with form fill in one save", async () => {
    const doc = await load();
    const firstText = doc.getForm().getFields().find((f) => f.type === "text")!;
    doc.getForm().getTextField(firstText.name).setText("VALUE");
    doc.getPage(0).drawText("STAMP", { x: 30, y: 30, size: 10 });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    const field = reloaded.getForm().getFields().find((f) => f.name === firstText.name)!;
    expect(field.value).toBe("VALUE");
    expect(new TextDecoder("latin1").decode(out)).toContain("(STAMP) Tj");
  });

  test("output still parses as a PDF with same page count", async () => {
    const doc = await load();
    const before = doc.getPageCount();
    doc.getPage(0).drawText("x", { x: 10, y: 10, size: 8 });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(before);
  });

  test("invalid options throw before save", async () => {
    const doc = await load();
    const page = doc.getPage(0);
    expect(() => page.drawText("x", { x: 0, y: 0, size: 0 })).toThrow();
    expect(() => page.drawText("x", { x: 0, y: 0, size: -3 })).toThrow();
  });
});
