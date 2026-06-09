import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const ANEXO = join(import.meta.dir, "fixtures/Discapacidad/Anexo-3-sssalud.pdf");
const FICHA = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
const tinyJpeg = new Uint8Array([
  0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00,
  0x02, 0x00, 0x03, 0x03, 0x00, 0xff, 0xd9,
]);
const tinyPng = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
  0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
  0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00,
  0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
  0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00,
  0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
]);

const load = (path: string) => PdfDocument.load(new Uint8Array(readFileSync(path)));

test("sets a visual signature image and keeps the PDF loadable", async () => {
  const doc = await load(ANEXO);
  const form = doc.getForm();
  form.getSignature("firma.titular").setImage(tinyJpeg);

  const out = await doc.save();
  expect(out.length).toBeGreaterThan(readFileSync(ANEXO).length);

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("firma.titular")?.type).toBe("signature");
});

test("sets a visual signature from PNG bytes", async () => {
  const doc = await load(ANEXO);
  const form = doc.getForm();
  form.getSignature("firma.titular").setImage(tinyPng);

  const out = await doc.save();
  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("firma.titular")?.type).toBe("signature");
});

test("visual signature can be flattened after setting the image", async () => {
  const doc = await load(ANEXO);
  const form = doc.getForm();
  form.getSignature("firma.titular").setImage(tinyJpeg);
  form.flattenField("firma.titular");

  const out = await doc.save();
  const names = (await PdfDocument.load(out)).getForm().getFields().map((f) => f.name);
  expect(names).not.toContain("firma.titular");
});

test("getSignature on a non-signature field throws", async () => {
  const form = (await load(FICHA)).getForm();
  expect(() => form.getSignature("beneficiario.apellidos_nombres")).toThrow(/not a signature/);
});
