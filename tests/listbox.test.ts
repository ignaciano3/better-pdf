import { test, expect } from "bun:test";
import { PdfListBox, FillQueue } from "../src/fields.ts";
import { InvalidOptionError } from "../src/errors.ts";
import type { FieldInfo } from "../src/form.ts";

function listboxInfo(): FieldInfo {
  return {
    name: "preferencias.idioma",
    type: "listbox",
    value: null,
    states: [],
    options: ["ES", "EN", "PT"],
    readOnly: false,
    required: false,
    exported: true,
    maxLength: null,
    widgets: [],
  };
}

test("PdfListBox.select queues a valid option", () => {
  const queue = new FillQueue();
  new PdfListBox(listboxInfo(), queue).select("EN");
  expect(queue.length).toBe(1);
  expect(JSON.parse(queue.toPayload().opsJson)).toEqual([
    { name: "preferencias.idioma", value: "EN" },
  ]);
});

test("PdfListBox.select rejects an unknown option", () => {
  const queue = new FillQueue();
  const lb = new PdfListBox(listboxInfo(), queue);
  expect(() => lb.select("DE")).toThrow(InvalidOptionError);
  expect(queue.length).toBe(0);
});

test("PdfListBox.options exposes the field's option values", () => {
  const lb = new PdfListBox(listboxInfo(), new FillQueue());
  expect(lb.options).toEqual(["ES", "EN", "PT"]);
});
