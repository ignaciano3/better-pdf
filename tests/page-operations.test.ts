import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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

test("merge preserves interactive form fields", async () => {
  const src = readFileSync(FIXTURE);
  const merged = await PdfDocument.merge([src, src]);
  const doc = await PdfDocument.load(merged);
  const fields = doc.getForm().getFields();
  // Two copies merged => roughly double the original field count, all present.
  const single = (await PdfDocument.load(src)).getForm().getFields();
  expect(single.length).toBeGreaterThan(0);
  expect(fields.length).toBeGreaterThanOrEqual(single.length);
});

test("merged form field names are unique (collisions renamed)", async () => {
  const src = readFileSync(FIXTURE);
  const merged = await PdfDocument.merge([src, src]);
  const doc = await PdfDocument.load(merged);
  const names = doc.getForm().getFields().map((f) => f.name);
  const unique = new Set(names);
  expect(unique.size).toBe(names.length);
});

// qpdf structural validation (mirrors tests/qpdf-validate.test.ts).
function qpdfSeverity(path: string): number {
  const r = Bun.spawnSync(["qpdf", "--check", path], { stdout: "ignore", stderr: "ignore" });
  if (r.exitCode === 0) return 0;
  if (r.exitCode === 3) return 1; // warnings only
  return 2;
}

let hasQpdf = false;
try {
  hasQpdf =
    Bun.spawnSync(["qpdf", "--version"], { stdout: "ignore", stderr: "ignore" }).exitCode === 0;
} catch {
  hasQpdf = false;
}

test.skipIf(!hasQpdf)("merged-with-form output is no worse than original under qpdf --check", async () => {
  const src = new Uint8Array(readFileSync(FIXTURE));
  const dir = mkdtempSync(join(tmpdir(), "better-pdf-pageops-qpdf-"));

  const origPath = join(dir, "orig.pdf");
  writeFileSync(origPath, src);
  const baseline = qpdfSeverity(origPath);

  const merged = await PdfDocument.merge([src, src]);
  const outPath = join(dir, "merged.pdf");
  writeFileSync(outPath, merged);

  expect(qpdfSeverity(outPath)).toBeLessThanOrEqual(baseline);
});
