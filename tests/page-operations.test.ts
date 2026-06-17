import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { readFileSync } from "node:fs";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("merge doubles the page count", async () => {
  const bytes = readFileSync(FIXTURE);
  const n = (await PdfDocument.load(bytes)).getPageCount();
  const merged = await PdfDocument.merge([bytes, bytes]);
  const out = await PdfDocument.load(merged);
  expect(out.getPageCount()).toBe(2 * n);
});

test("copyPages extracts the first page", async () => {
  const bytes = readFileSync(FIXTURE);
  const doc = await PdfDocument.load(bytes);
  const onePage = await doc.copyPages([0]);
  expect((await PdfDocument.load(onePage)).getPageCount()).toBe(1);
});

test("splitPages yields one PDF per page", async () => {
  const bytes = readFileSync(FIXTURE);
  const doc = await PdfDocument.load(bytes);
  const n = doc.getPageCount();
  const parts = await doc.splitPages();
  expect(parts.length).toBe(n);
  expect((await PdfDocument.load(parts[0]!)).getPageCount()).toBe(1);
});
