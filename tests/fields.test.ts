import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

async function form() {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  return doc.getForm();
}

test("getFields returns all fields with names and types", async () => {
  const fields = (await form()).getFields();
  expect(fields.length).toBe(30);
  expect(fields[0]!.name).toBe("beneficiario.apellidos_nombres");
  expect(fields[0]!.type).toBe("text");
});

test("radio field exposes its export states", async () => {
  const f = (await form()).getField("beneficiario.tipo_beneficiario");
  expect(f?.type).toBe("radio");
  expect(f?.states).toEqual(expect.arrayContaining(["Titular", "Familiar"]));
});

test("dropdown field exposes its options", async () => {
  const f = (await form()).getField("beneficiario.estado_civil");
  expect(f?.type).toBe("dropdown");
  expect(f?.options).toEqual(expect.arrayContaining(["Soltero"]));
});

test("getField returns undefined for an unknown name", async () => {
  expect((await form()).getField("does.not.exist")).toBeUndefined();
});
