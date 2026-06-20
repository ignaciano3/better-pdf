import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

// A hand-written schema standing in for a generated `…Fields` const, with names
// that exist in the FICHA fixture so the typed accessors reach the real runtime.
const schema = {
  "beneficiario.apellidos_nombres": { type: "text", readOnly: false, value: "", states: [] as const, options: [] as const, multiSelect: false },
  "beneficiario.estado_civil": { type: "dropdown", readOnly: false, value: "Soltero", states: [] as const, options: ["Soltero", "Casado"] as const, multiSelect: false },
} as const;

function load() {
  return PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
}

test("typed getForm<S>() drives the same runtime as the untyped form", async () => {
  const doc = await load();
  const form = doc.getForm<typeof schema>();
  form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  form.getDropdown("beneficiario.estado_civil").select("Casado");

  const reloaded = await PdfDocument.load(await doc.save());
  const read = reloaded.getForm();
  expect(read.getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
  expect(read.getField("beneficiario.estado_civil")?.value).toBe("Casado");
});
