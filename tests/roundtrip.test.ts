import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { PdfDocument } from "../src/index.ts";

const FIXTURE =
  "tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 1.pdf";

test("load then save returns byte-identical PDF", async () => {
  const original = new Uint8Array(readFileSync(FIXTURE));
  const doc = await PdfDocument.load(original);
  const out = await doc.save();
  expect(out).toBeInstanceOf(Uint8Array);
  expect(Buffer.from(out).equals(Buffer.from(original))).toBe(true);
});

test("load accepts an ArrayBuffer", async () => {
  const original = new Uint8Array(readFileSync(FIXTURE));
  const doc = await PdfDocument.load(original.buffer.slice(0));
  const out = await doc.save();
  expect(out.length).toBe(original.length);
});
