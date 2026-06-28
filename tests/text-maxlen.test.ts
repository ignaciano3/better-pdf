import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfTextField, FillQueue } from "../src/forms/fields.ts";
import { MaxLengthExceededError } from "../src/core/errors.ts";
import { PdfDocument } from "../src/index.ts";
import type { FieldInfo } from "../src/forms/form.ts";

function textInfo(maxLength: number | null): FieldInfo {
  return {
    name: "applicant.code",
    type: "text",
    value: null,
    defaultValue: null,
    states: [],
    options: [],
    readOnly: false,
    required: false,
    exported: true,
    maxLength,
    multiSelect: false,
    password: false,
    multiline: false,
    comb: false,
    editable: false,
    align: "left",
    tooltip: null,
    widgets: [],
  };
}

test("setText accepts text up to maxLength", () => {
  const queue = new FillQueue();
  new PdfTextField(textInfo(5), queue).setText("ABCDE");
  expect(queue.length).toBe(1);
});

test("setText throws MaxLengthExceededError past maxLength", () => {
  const queue = new FillQueue();
  const field = new PdfTextField(textInfo(5), queue);
  let err: unknown;
  try {
    field.setText("ABCDEF");
  } catch (e) {
    err = e;
  }
  expect(err).toBeInstanceOf(MaxLengthExceededError);
  const e = err as MaxLengthExceededError;
  expect(e.maxLength).toBe(5);
  expect(e.actualLength).toBe(6);
  expect(queue.length).toBe(0);
});

test("setText is unconstrained when maxLength is null", () => {
  const queue = new FillQueue();
  new PdfTextField(textInfo(null), queue).setText("x".repeat(1000));
  expect(queue.length).toBe(1);
});

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

test("real fields expose exported (boolean) and maxLength (number | null)", async () => {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  for (const f of doc.getForm().getFields()) {
    expect(typeof f.exported).toBe("boolean");
    expect(f.maxLength === null || typeof f.maxLength === "number").toBe(true);
  }
});
