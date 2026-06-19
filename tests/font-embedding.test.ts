import { expect, test } from "bun:test";
import { PdfDocument, PdfError, PageSizes } from "../src/index.ts";
import { readFileSync } from "node:fs";
import * as pdfjs from "pdfjs-dist/legacy/build/pdf.mjs";

const FONT = new Uint8Array(readFileSync("tests/fixtures/fonts/NotoSans-Regular.subset.ttf"));
const FIXTURE = new Uint8Array(
  readFileSync("tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf"),
);

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

// (a) No-glyph char doesn't panic: NotoSans-Regular.subset lacks emoji / full CJK
test("drawing chars not in the font does not throw (renders .notdef)", async () => {
  const doc = await PdfDocument.create();
  const font = await doc.embedFont(FONT);
  const page = doc.addPage();
  // "🎉" is outside the BMP and NotoSans-Regular.subset has no emoji glyphs.
  // The core should silently substitute .notdef or skip — it must NOT panic.
  page.drawText("Hello 🎉 World", { x: 50, y: 700, size: 18, font });
  const bytes = await doc.save();
  expect(bytes.length).toBeGreaterThan(1000);
  const reopened = await PdfDocument.load(bytes);
  expect(reopened.getPageCount()).toBe(1);
});

// (b) Bad font bytes → save() rejects with a PdfError-family error
test("embedFont with bad bytes causes save() to throw a PdfError", async () => {
  const doc = await PdfDocument.create();
  const badFont = await doc.embedFont(new Uint8Array([1, 2, 3]));
  const page = doc.addPage();
  page.drawText("Hello", { x: 50, y: 700, size: 12, font: badFont });
  await expect(doc.save()).rejects.toBeInstanceOf(PdfError);
});

// (c) subset:false produces a larger file than subset:true for the same text
test("subset:false output is larger than subset:true for same text", async () => {
  async function buildDoc(subset: boolean): Promise<Uint8Array> {
    const doc = await PdfDocument.create();
    const font = await doc.embedFont(FONT, { subset });
    const page = doc.addPage();
    page.drawText("Hello", { x: 50, y: 700, size: 18, font });
    return doc.save();
  }

  const [full, sub] = await Promise.all([buildDoc(false), buildDoc(true)]);
  // Full font (NotoSans subset TTF ~556 KB) should be meaningfully larger
  // than the glyph-subset for a 5-char ASCII string.
  expect(full.length).toBeGreaterThan(sub.length + 10_000);
});

// (d) Embedded-font text on a LOADED PDF (apply_draw_ops path)
test("embedded font text on a loaded PDF round-trips correctly", async () => {
  const doc = await PdfDocument.load(FIXTURE);
  const originalPageCount = doc.getPageCount();
  const font = await doc.embedFont(FONT);
  const page = doc.getPage(0);
  // Draw near the bottom-left to avoid overlapping form fields
  page.drawText("Héllo", { x: 50, y: 30, size: 12, font });
  const bytes = await doc.save();
  expect(bytes.length).toBeGreaterThan(1000);
  const reopened = await PdfDocument.load(bytes);
  expect(reopened.getPageCount()).toBe(originalPageCount);
});

test("maxWidth wraps text rendered with an embedded font", async () => {
  const doc = await PdfDocument.create();
  const fontBytes = readFileSync("tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
  const font = await doc.embedFont(fontBytes);
  const page = doc.addPage(PageSizes.A4);
  page.drawText("the quick brown fox jumps over the lazy dog", {
    x: 40,
    y: 700,
    size: 12,
    font,
    maxWidth: 80,
  });
  const out = await doc.save();
  const s = new TextDecoder("latin1").decode(out);
  // Embedded text renders as hex <....> Tj; wrapping yields more than one.
  const tjCount = (s.match(/> Tj/g) ?? []).length;
  expect(tjCount).toBeGreaterThan(1);
});

// Render/visual verification: pdfjs parses the embedded-font PDF and extracts text
test("pdfjs can extract embedded-font text content (render verification)", async () => {
  const doc = await PdfDocument.create();
  const font = await doc.embedFont(FONT);
  const page = doc.addPage();
  page.drawText("Héllo Unicode", { x: 50, y: 700, size: 24, font });
  const bytes = await doc.save();

  // Use pdfjs-dist legacy build (no DOM required) to parse and extract text
  const pdfjsDoc = await pdfjs.getDocument({ data: bytes }).promise;
  expect(pdfjsDoc.numPages).toBe(1);
  const pdfjsPage = await pdfjsDoc.getPage(1);
  const content = await pdfjsPage.getTextContent();
  const extracted = content.items.map((it) => ("str" in it ? it.str : "")).join(" ");
  // pdfjs should recover the Unicode string from the embedded Type0 font
  expect(extracted).toContain("Héllo Unicode");
});
