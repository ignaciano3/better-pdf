import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.ts";
import { readFileSync } from "node:fs";

const FONT = new Uint8Array(readFileSync("tests/fixtures/fonts/NotoSans-Regular.subset.ttf"));

test("embed font and draw unicode text on a created page", async () => {
  const doc = await PdfDocument.create();
  const font = await doc.embedFont(FONT);
  const page = doc.addPage();
  page.drawText("Héllo 日本語", { x: 50, y: 700, size: 24, font });
  const bytes = await doc.save();
  expect(bytes.length).toBeGreaterThan(1000);
  // reload + sanity: page survives the round trip
  const reopened = await PdfDocument.load(bytes);
  expect(reopened.getPageCount()).toBe(1);
});

test("widthOfTextAtSize works for embedded fonts", async () => {
  const doc = await PdfDocument.create();
  const font = await doc.embedFont(FONT);
  const w = font.widthOfTextAtSize("Hello", 12);
  expect(w).toBeGreaterThan(0);
});
