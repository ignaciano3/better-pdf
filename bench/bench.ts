/**
 * Benchmarks better-pdf against pdf-lib on the same operations and fixture.
 *
 *   bun run bench                 # default 50 iterations
 *   BENCH_ITER=200 bun run bench  # more iterations
 *
 * Each scenario runs a full load -> mutate -> save cycle per iteration so the
 * two libraries are compared on identical, end-to-end work.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";
import { PDFDocument } from "pdf-lib";

const FIXTURE = join(
  import.meta.dir,
  "../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);
const bytes = new Uint8Array(readFileSync(FIXTURE));

const ITER = Number(process.env.BENCH_ITER ?? 50);
const WARMUP = 5;

async function mean(fn: () => Promise<void>): Promise<number> {
  for (let i = 0; i < WARMUP; i++) await fn();
  const t0 = performance.now();
  for (let i = 0; i < ITER; i++) await fn();
  return (performance.now() - t0) / ITER;
}

// Pick text fields both libraries agree are writable text fields.
const probe = await PdfDocument.load(bytes);
const candidates = probe
  .getForm()
  .getFields()
  .filter((f) => f.type === "text" && !f.readOnly)
  .map((f) => f.name);

const pdflibForm = (await PDFDocument.load(bytes)).getForm();
const textNames = candidates
  .filter((name) => {
    try {
      pdflibForm.getTextField(name);
      return true;
    } catch {
      return false;
    }
  })
  .slice(0, 10);

interface Scenario {
  name: string;
  better: () => Promise<void>;
  pdflib: () => Promise<void>;
}

const scenarios: Scenario[] = [
  {
    name: "load + read fields",
    better: async () => {
      (await PdfDocument.load(bytes)).getForm().getFields();
    },
    pdflib: async () => {
      (await PDFDocument.load(bytes)).getForm().getFields();
    },
  },
  {
    name: `fill ${textNames.length} text fields + save`,
    better: async () => {
      const doc = await PdfDocument.load(bytes);
      const form = doc.getForm();
      for (const name of textNames) form.getTextField(name).setText("GARCIA");
      await doc.save();
    },
    pdflib: async () => {
      const doc = await PDFDocument.load(bytes);
      const form = doc.getForm();
      for (const name of textNames) form.getTextField(name).setText("GARCIA");
      await doc.save();
    },
  },
  {
    name: "flatten all + save",
    better: async () => {
      const doc = await PdfDocument.load(bytes);
      doc.getForm().flatten();
      await doc.save();
    },
    pdflib: async () => {
      const doc = await PDFDocument.load(bytes);
      doc.getForm().flatten();
      await doc.save();
    },
  },
];

console.log(`Fixture: Form.-D.P.-2.4.1-Ficha-personal.pdf (${bytes.length.toLocaleString()} bytes)`);
console.log(`Iterations: ${ITER} (after ${WARMUP} warmup)\n`);
console.log("| Scenario | better-pdf | pdf-lib | speedup |");
console.log("| --- | ---: | ---: | ---: |");

for (const s of scenarios) {
  const b = await mean(s.better);
  const p = await mean(s.pdflib);
  const speedup = `${(p / b).toFixed(1)}×`;
  console.log(
    `| ${s.name} | ${b.toFixed(2)} ms | ${p.toFixed(2)} ms | ${speedup} |`,
  );
}
