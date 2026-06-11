import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
const DIABETES = join(import.meta.dir, "fixtures/Medicamentos/Modulo-de-Diabetes.pdf");

const load = (p: string) => PdfDocument.load(new Uint8Array(readFileSync(p)));

test("setText is reflected in getField().value before save", async () => {
  const form = (await load(FICHA)).getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  expect(form.getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
});

test("radio select and dropdown select update value", async () => {
  const form = (await load(FICHA)).getForm();
  form.getRadioGroup("beneficiario.tipo_beneficiario").select("Titular");
  form.getDropdown("beneficiario.estado_civil").select("Casado");
  expect(form.getField("beneficiario.tipo_beneficiario")?.value).toBe("Titular");
  expect(form.getField("beneficiario.estado_civil")?.value).toBe("Casado");
});

test("check()/uncheck() update value to the on-state / Off", async () => {
  const form = (await load(DIABETES)).getForm();
  const box = form.getFields().find((f) => f.type === "checkbox");
  if (!box) throw new Error("fixture has no checkbox");
  form.getCheckBox(box.name).check();
  expect(form.getField(box.name)?.value).toBe(box.states[0]!);
  form.getCheckBox(box.name).uncheck();
  expect(form.getField(box.name)?.value).toBe("Off");
});
