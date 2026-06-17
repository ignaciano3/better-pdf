import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { readFileSync } from "node:fs";

const FIXTURE = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

test("set + get metadata round-trips on a loaded PDF", async () => {
  const doc = await PdfDocument.load(readFileSync(FIXTURE));
  doc.setTitle("Quarterly Report");
  doc.setAuthor("ACME");
  doc.setKeywords(["invoice", "2026"]);
  const bytes = await doc.save();
  const reopened = await PdfDocument.load(bytes);
  const meta = await reopened.getMetadata();
  expect(meta.title).toBe("Quarterly Report");
  expect(meta.author).toBe("ACME");
  expect(meta.keywords).toContain("invoice");
});

test("metadata on a created document", async () => {
  const doc = await PdfDocument.create();
  doc.setTitle("Generated");
  doc.addPage();
  const bytes = await doc.save();
  const meta = await (await PdfDocument.load(bytes)).getMetadata();
  expect(meta.title).toBe("Generated");
});
