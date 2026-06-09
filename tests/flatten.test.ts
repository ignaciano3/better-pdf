import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
const load = () => PdfDocument.load(new Uint8Array(readFileSync(FICHA)));

test("fill then flatten removes the field but keeps the document", async () => {
  const doc = await load();
  const form = doc.getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("FLAT");
  form.flattenField("beneficiario.apellidos_nombres");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  const names = reloaded.getForm().getFields().map((f) => f.name);
  expect(names).not.toContain("beneficiario.apellidos_nombres");
});

test("flatten() removes all fields", async () => {
  const doc = await load();
  doc.getForm().flatten();
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getForm().getFields().length).toBe(0);
});

test("flattenField on a missing field throws", async () => {
  const form = (await load()).getForm();
  expect(() => form.flattenField("nope.nope")).toThrow(/no such field/);
});
