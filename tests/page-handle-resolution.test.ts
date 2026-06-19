import { expect, test } from "bun:test";
import * as pdfjs from "pdfjs-dist/legacy/build/pdf.mjs";
import { PdfDocument, PageSizes, rgb } from "../src/index.js";

/** Build a known N-page loaded doc, each page stamped "ORIGn". */
async function makeLoadedDoc(n: number): Promise<Uint8Array> {
  const doc = await PdfDocument.create();
  for (let i = 0; i < n; i++) {
    const p = doc.addPage(PageSizes.A4);
    p.drawText(`ORIG${i}`, { x: 50, y: 700, size: 24, color: rgb(0, 0, 0) });
  }
  return doc.save();
}

/** Extract text of one page (zero-based) via pdfjs. */
async function pageText(bytes: Uint8Array, index: number): Promise<string> {
  const d = await pdfjs.getDocument({ data: bytes.slice() }).promise;
  const page = await d.getPage(index + 1); // pdfjs is 1-based
  const content = await page.getTextContent();
  return content.items.map((it) => ("str" in it ? it.str : "")).join(" ");
}

test("draw on appended handle lands on the final page even after a later insertPage", async () => {
  const base = await makeLoadedDoc(3);
  const doc = await PdfDocument.load(base);
  const appended = doc.addPage(PageSizes.A4);
  appended.drawText("APPENDED", { x: 50, y: 700, size: 24, color: rgb(0, 0, 0) });
  doc.insertPage(0, PageSizes.A4); // shifts everything right by one
  const out = await doc.save();

  // Final order: [blank, ORIG0, ORIG1, ORIG2, APPENDED] => 5 pages, APPENDED last.
  const re = await PdfDocument.load(out);
  expect(re.getPageCount()).toBe(5);
  expect(await pageText(out, 4)).toContain("APPENDED");
  // The inserted blank is at 0; appended text must NOT be on the page that was
  // its frozen index (3, which is ORIG2 in the final doc).
  expect(await pageText(out, 3)).toContain("ORIG2");
  expect(await pageText(out, 3)).not.toContain("APPENDED");
});

test("draw on a loaded handle follows the page when it is moved", async () => {
  const base = await makeLoadedDoc(3);
  const doc = await PdfDocument.load(base);
  const p0 = doc.getPage(0);
  p0.drawText("TRACK", { x: 50, y: 650, size: 24, color: rgb(0, 0, 0) });
  doc.movePage(0, 2); // ORIG0 (with TRACK) moves to the last slot
  const out = await doc.save();

  // Final order: [ORIG1, ORIG2, ORIG0+TRACK]
  expect(await pageText(out, 2)).toContain("TRACK");
  expect(await pageText(out, 2)).toContain("ORIG0");
  expect(await pageText(out, 0)).not.toContain("TRACK");
});

test("drawing on a page that is then removed rejects at save", async () => {
  const base = await makeLoadedDoc(3);
  const doc = await PdfDocument.load(base);
  const p1 = doc.getPage(1);
  p1.drawText("DOOMED", { x: 50, y: 650, size: 24, color: rgb(0, 0, 0) });
  doc.removePage(1);
  await expect(doc.save()).rejects.toThrow();
});

test("insertPage / removePage / movePage validate indices eagerly", async () => {
  const base = await makeLoadedDoc(2);
  const doc = await PdfDocument.load(base);
  expect(() => doc.insertPage(5, PageSizes.A4)).toThrow();
  expect(() => doc.insertPage(-1, PageSizes.A4)).toThrow();
  expect(() => doc.removePage(2)).toThrow();
  expect(() => doc.movePage(0, 9)).toThrow();
  expect(() => doc.movePage(-1, 0)).toThrow();
});
