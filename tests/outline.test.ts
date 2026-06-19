import { describe, expect, test } from "bun:test";
import { PdfDocument, PageSizes } from "../src/index.ts";
import type { OutlineItem } from "../src/index.ts";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

const ITEMS: OutlineItem[] = [
  { title: "A", page: 0 },
  { title: "B", page: 0, children: [{ title: "B.1", page: 0 }] },
];

describe("setOutline — created doc", () => {
  test("setOutline then save/reload valid, page count preserved", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.setOutline(ITEMS);
    const out = await doc.save();
    expect(new TextDecoder("latin1").decode(out).slice(0, 5)).toBe("%PDF-");
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });
});

describe("setOutline — loaded doc", () => {
  test("setOutline on fixture then save/reload valid, page count preserved", async () => {
    const bytes = new Uint8Array(await Bun.file(FIXTURE).arrayBuffer());
    const doc = await PdfDocument.load(bytes);
    const pageCount = doc.getPageCount();
    doc.setOutline(ITEMS);
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(pageCount);
  });
});
