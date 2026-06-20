import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FIXTURE = join(import.meta.dir, "fixtures/generated/ficha-objstreams.pdf");
const FIXTURE_BIG = join(import.meta.dir, "fixtures/generated/ficha-objstreams-big.pdf");
const FIXTURE_UPDATED = join(import.meta.dir, "fixtures/generated/ficha-objstreams-updated.pdf");

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

// --- larger-objstm fixture (higher object count; stresses ObjStm decoding) ---

test("fills and reloads a larger-objstm PDF", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FIXTURE_BIG)));
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("GARCIA_BIG");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA_BIG");
});

test("flattens a larger-objstm PDF", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FIXTURE_BIG)));
  const form = doc.getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA_BIG");
  form.flattenField("beneficiario.apellidos_nombres");
  const out = await doc.save();

  const names = (await PdfDocument.load(out)).getForm().getFields().map((f) => f.name);
  expect(names).not.toContain("beneficiario.apellidos_nombres");
});

// --- incremental-over-xref-stream fixture (base xref-stream + our own /Prev update) ---

test("fills and reloads an incremental-update-over-xref-stream PDF", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FIXTURE_UPDATED)));
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("GARCIA_UPDATED");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe(
    "GARCIA_UPDATED",
  );
});

test("flattens an incremental-update-over-xref-stream PDF", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FIXTURE_UPDATED)));
  const form = doc.getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA_UPDATED");
  form.flattenField("beneficiario.apellidos_nombres");
  const out = await doc.save();

  const names = (await PdfDocument.load(out)).getForm().getFields().map((f) => f.name);
  expect(names).not.toContain("beneficiario.apellidos_nombres");
});
