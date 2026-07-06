import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument, MissingGlyphError, FieldTypeError, PdfError, StandardFonts } from "../src/index.ts";

const NOTO = new Uint8Array(readFileSync(join(import.meta.dir, "fixtures/fonts/NotoSans-Regular.subset.ttf")));
const FICHA_PERSONAL = new Uint8Array(
  readFileSync(join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf")),
);

test("setText with embedded font combined with a page-structure op throws a clear error", async () => {
  const doc = await PdfDocument.load(FICHA_PERSONAL);
  const font = await doc.embedFont(NOTO);
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("Juan Perez", { font });
  doc.removePage(doc.getPageCount() - 1);
  await expect(doc.save()).rejects.toThrow(/cannot be combined with page-structure/);
});

test("setText rejects a standard-14 PdfFont handle", async () => {
  const doc = await PdfDocument.create();
  doc.addPage();
  doc.createForm().addTextField("n", { page: 0, x: 10, y: 10, width: 200, height: 20 });
  const form = doc.getForm();
  const helv = doc.getFont(StandardFonts.Helvetica);
  expect(() => form.getTextField("n").setText("x", { font: helv })).toThrow(PdfError);
});

test("setText with font on a comb field throws FieldTypeError", async () => {
  const doc = await PdfDocument.create();
  doc.addPage();
  doc.createForm().addTextField("c", { page: 0, x: 10, y: 10, width: 200, height: 20, comb: true, maxLength: 4 });
  const font = await doc.embedFont(NOTO);
  expect(() => doc.getForm().getTextField("c").setText("ab", { font })).toThrow(FieldTypeError);
});

test("missing glyph surfaces as MissingGlyphError at save", async () => {
  const doc = await PdfDocument.create();
  doc.addPage();
  doc.createForm().addTextField("n", { page: 0, x: 10, y: 10, width: 200, height: 20 });
  const out = await doc.save();

  // Reload so embedFont() and setText({ font }) share the same document
  // instance's font registry (a created doc's getForm() seals it, so the
  // font used to fill can't be registered after the fact on that instance).
  const reloaded = await PdfDocument.load(out);
  const font = await reloaded.embedFont(NOTO); // Latin-only subset fixture
  reloaded.getForm().getTextField("n").setText("日本語", { font });
  await expect(reloaded.save()).rejects.toThrow(MissingGlyphError);
});

test("drawText throws MissingGlyphError by default and skips with onMissingGlyph", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  const font = await doc.embedFont(NOTO);
  page.drawText("日本語", { x: 10, y: 10, size: 12, font });
  await expect(doc.save()).rejects.toThrow(MissingGlyphError);

  const doc2 = await PdfDocument.create();
  const page2 = doc2.addPage();
  const font2 = await doc2.embedFont(NOTO);
  page2.drawText("日本語", { x: 10, y: 10, size: 12, font: font2, onMissingGlyph: "skip" });
  await doc2.save(); // must not throw
});
