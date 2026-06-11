// Independent validation: qpdf re-parses our output. Severity must not get
// worse than the original fixture (0 = clean, 1 = warnings, 2 = errors).
// Skips when qpdf is not installed (CI installs it; locally: dnf/apt install qpdf).
import { test, expect } from "bun:test";
import { readFileSync, writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

function qpdfSeverity(path: string): number {
  const r = Bun.spawnSync(["qpdf", "--check", path], { stdout: "ignore", stderr: "ignore" });
  if (r.exitCode === 0) return 0;
  if (r.exitCode === 3) return 1; // warnings only
  return 2;
}

let hasQpdf = false;
try {
  hasQpdf = Bun.spawnSync(["qpdf", "--version"], { stdout: "ignore", stderr: "ignore" })
    .exitCode === 0;
} catch {
  hasQpdf = false;
}

const FIXTURES = [
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
  "fixtures/Medicamentos/Modulo-de-Diabetes.pdf",
  "fixtures/Discapacidad/Convenio-OSFATUN-Discapacidad-2022.pdf",
  "fixtures/generated/ficha-objstreams.pdf",
];

const dir = mkdtempSync(join(tmpdir(), "better-pdf-qpdf-"));
const slug = (rel: string) => rel.replace(/[/ ]/g, "_");

for (const rel of FIXTURES) {
  test.skipIf(!hasQpdf)(`qpdf --check: fill+flatten of ${rel} is no worse than the original`, async () => {
    const original = new Uint8Array(readFileSync(join(import.meta.dir, rel)));
    const originalPath = join(dir, `orig-${slug(rel)}`);
    writeFileSync(originalPath, original);
    const baseline = qpdfSeverity(originalPath);

    const doc = await PdfDocument.load(original);
    const form = doc.getForm();
    const text = form.getFields().find((f) => f.type === "text");
    if (!text) throw new Error(`fixture ${rel} has no text field`);
    form.getTextField(text.name).setText("QPDF CHECK");
    form.flatten();
    const out = await doc.save();
    const outPath = join(dir, `out-${slug(rel)}`);
    writeFileSync(outPath, out);

    expect(qpdfSeverity(outPath)).toBeLessThanOrEqual(baseline);
  });
}
