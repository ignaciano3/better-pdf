import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

function load() {
  return PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
}

test("fills a text field and reads it back after save", async () => {
  const doc = await load();
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
});

test("fills an accented text field without flattening and reads it back after save", async () => {
  const doc = await load();
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("Juan Pérez");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("Juan Pérez");
});

test("selects a radio option and reads it back", async () => {
  const doc = await load();
  doc.getForm().getRadioGroup("beneficiario.tipo_beneficiario").select("Titular");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.tipo_beneficiario")?.value).toBe("Titular");
});

test("selects a dropdown option and reads it back", async () => {
  const doc = await load();
  doc.getForm().getDropdown("beneficiario.estado_civil").select("Casado");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.estado_civil")?.value).toBe("Casado");
});

test("save with no pending ops returns a byte-identical round-trip", async () => {
  const original = new Uint8Array(readFileSync(FICHA));
  const doc = await PdfDocument.load(original);
  const out = await doc.save();
  expect(Buffer.from(out).equals(Buffer.from(original))).toBe(true);
});

test("wrong-type access throws", async () => {
  const form = (await load()).getForm();
  expect(() => form.getRadioGroup("beneficiario.apellidos_nombres")).toThrow(/not a radio/);
});

test("invalid radio option throws before save", async () => {
  const form = (await load()).getForm();
  expect(() => form.getRadioGroup("beneficiario.tipo_beneficiario").select("Nope")).toThrow();
});
