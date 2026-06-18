import { expect, test } from "bun:test";
import { PdfDocument, InvalidRotationError } from "../src/index.js";
import { readFileSync } from "node:fs";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("rotate a loaded page persists", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  doc.getPage(0).setRotation(90);
  const out = await doc.save();
  const reopened = await PdfDocument.load(out);
  expect(reopened.getPage(0).rotation).toBe(90);
});

test("resize a created page", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.setSize(200, 300);
  const out = await doc.save();
  const reopened = await PdfDocument.load(out);
  const p = reopened.getPage(0);
  expect(Math.round(p.width)).toBe(200);
  expect(Math.round(p.height)).toBe(300);
});

test("setRotation rejects non-multiple of 90", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  expect(() => doc.getPage(0).setRotation(45)).toThrow(InvalidRotationError);
});
