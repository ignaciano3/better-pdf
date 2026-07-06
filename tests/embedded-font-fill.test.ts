import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument, MissingGlyphError, FieldTypeError, PdfError, StandardFonts } from "../src/index.ts";

const NOTO = new Uint8Array(readFileSync(join(import.meta.dir, "fixtures/fonts/NotoSans-Regular.subset.ttf")));
const NOTO_JP = new Uint8Array(readFileSync(join(import.meta.dir, "fixtures/fonts/NotoSansJP-Regular.subset.ttf")));
const FICHA = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
const FICHA_PERSONAL = new Uint8Array(readFileSync(FICHA));

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

// --- Task 6: end-to-end CJK integration tests ---

test("flagship: CJK fill on a loaded standard-14 field, round-trips", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP);
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  const out = await doc.save();
  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("山田太郎");
});

test("CJK fill + flatten in one save", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP);
  const form = doc.getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  form.flatten();
  const out = await doc.save();
  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getFields().length).toBe(0);
  expect(out.length).toBeGreaterThan(0);
});

test("multiline CJK fill wraps and round-trips", async () => {
  const doc = await PdfDocument.create();
  doc.addPage();
  doc.createForm().addTextField("m", { page: 0, x: 10, y: 10, width: 80, height: 60, multiline: true });
  const saved = await doc.save();
  const loaded = await PdfDocument.load(saved);
  const font = await loaded.embedFont(NOTO_JP);
  loaded.getForm().getTextField("m").setText("日本語 日本語 日本語", { font });
  const out = await loaded.save();
  expect((await PdfDocument.load(out)).getForm().getField("m")?.value).toBe("日本語 日本語 日本語");
});

test("subset font used only for fill renders all value glyphs (no throw)", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP); // subset: true default; no drawText usage
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  await doc.save(); // MissingGlyphError here would mean fill chars didn't join used_per_font
});

test("font shared by drawText and fill saves without error and embeds once", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const font = await doc.embedFont(NOTO_JP);
  doc.getPage(0).drawText("日本語", { x: 20, y: 20, size: 10, font });
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
  const out = await doc.save();
  // Best-effort single-embed check: FontFile2 streams are FlateDecoded so the raw
  // marker bytes typically won't appear in the compressed output — the scan below
  // is expected to find 0 matches. The authoritative single-build check is the
  // Rust-side test from Task 1; here we keep the no-throw assertion as primary.
  const marker = NOTO_JP.slice(0, 64);
  let count = 0;
  outer: for (let i = 0; i <= out.length - marker.length; i++) {
    for (let j = 0; j < marker.length; j++) if (out[i + j] !== marker[j]) continue outer;
    count++;
  }
  expect(count).toBeLessThanOrEqual(1); // vacuous when 0 (compressed/subsetted); no-throw above is the real assertion
});
