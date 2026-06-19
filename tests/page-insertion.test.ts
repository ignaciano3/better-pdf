import { expect, test } from "bun:test";
import { PdfDocument, PageSizes, rgb } from "../src/index.js";
import { readFileSync } from "node:fs";

const FIXTURE =
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("append a page to a loaded doc and draw on it", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  const n = doc.getPageCount();
  const page = doc.addPage(PageSizes.A4); // works on loaded now
  page.drawText("Appended", { x: 50, y: 700, size: 24, color: rgb(0, 0, 0) });
  const out = await doc.save();
  const re = await PdfDocument.load(out);
  expect(re.getPageCount()).toBe(n + 1);
});

test("insertPage / removePage change count", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  const n = doc.getPageCount();
  doc.insertPage(0, PageSizes.A4);
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(n + 1);

  const doc2 = await PdfDocument.load(readFileSync(FIXTURE));
  doc2.removePage(0);
  const out2 = await doc2.save();
  expect((await PdfDocument.load(out2)).getPageCount()).toBe(n - 1);
});
