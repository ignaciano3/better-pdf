import { test, expect } from "bun:test";
import { PdfListBox, FillQueue } from "../src/forms/fields.ts";
import { InvalidOptionError, MultiSelectError } from "../src/core/errors.ts";
import { PdfDocument } from "../src/index.ts";
import type { FieldInfo } from "../src/forms/form.ts";

function listboxInfo(multiSelect = false): FieldInfo {
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
    multiSelect,
    multiline: false,
    comb: false,
    editable: false,
    align: "left",
    tooltip: null,
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

test("PdfListBox.selectMultiple queues a values op on a multi-select list box", () => {
  const queue = new FillQueue();
  new PdfListBox(listboxInfo(true), queue).selectMultiple(["ES", "PT"]);
  expect(queue.length).toBe(1);
  expect(JSON.parse(queue.toPayload().opsJson)).toEqual([
    { name: "preferencias.idioma", values: ["ES", "PT"] },
  ]);
});

test("PdfListBox.selectMultiple rejects an unknown option", () => {
  const lb = new PdfListBox(listboxInfo(true), new FillQueue());
  expect(() => lb.selectMultiple(["ES", "DE"])).toThrow(InvalidOptionError);
});

test("PdfListBox.selectMultiple throws on a single-select list box", () => {
  const lb = new PdfListBox(listboxInfo(false), new FillQueue());
  expect(() => lb.selectMultiple(["ES", "PT"])).toThrow(MultiSelectError);
});

test("PdfListBox.selectMultiple deduplicates values preserving first-seen order", () => {
  const queue = new FillQueue();
  new PdfListBox(listboxInfo(true), queue).selectMultiple(["ES", "ES", "PT"]);
  expect(queue.length).toBe(1);
  expect(JSON.parse(queue.toPayload().opsJson)).toEqual([
    { name: "preferencias.idioma", values: ["ES", "PT"] },
  ]);
});

test("selectMultiple round-trips both values", async () => {
  const bytes = new Uint8Array(
    await Bun.file("tests/fixtures/generated/ficha-multiselect-listbox.pdf").arrayBuffer(),
  );
  const doc = await PdfDocument.load(bytes);
  const form = doc.getForm();
  form.getListBox("beneficiario.estado_civil").selectMultiple(["Casado", "Viudo"]);
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  const field = reloaded.getForm().getField("beneficiario.estado_civil");
  // Multi-value /V is reported by the reader as a comma-joined string.
  expect(field?.value ?? "").toContain("Casado");
  expect(field?.value ?? "").toContain("Viudo");
});
