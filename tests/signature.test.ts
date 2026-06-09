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
