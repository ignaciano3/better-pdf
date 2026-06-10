import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

test("fields report required flag, page index, and rect", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  const field = doc
    .getForm()
    .getFields()
    .find((f) => f.type === "text")!;

  expect(typeof field.required).toBe("boolean");
  expect(field.widgets.length).toBeGreaterThan(0);

  const w = field.widgets[0]!;
  expect(Number.isInteger(w.page)).toBe(true);
  expect(w.page).toBeGreaterThanOrEqual(0);
  expect(w.rect).toHaveLength(4);
  expect(w.rect.every((n) => typeof n === "number")).toBe(true);
});
