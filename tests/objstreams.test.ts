import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FIXTURE = join(import.meta.dir, "fixtures/generated/ficha-objstreams.pdf");

test("fills and reloads an xref-stream PDF", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FIXTURE)));
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
});

test("flattens an xref-stream PDF", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FIXTURE)));
  const form = doc.getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  form.flattenField("beneficiario.apellidos_nombres");
  const out = await doc.save();

  const names = (await PdfDocument.load(out)).getForm().getFields().map((f) => f.name);
  expect(names).not.toContain("beneficiario.apellidos_nombres");
});
