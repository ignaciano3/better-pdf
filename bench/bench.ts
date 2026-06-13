/**
 * Benchmarks better-pdf against pdf-lib on representative AcroForm work.
 *
 *   bun run bench                 # default 25 iterations
 *   BENCH_ITER=100 bun run bench  # more stable numbers
 *
 * Each scenario runs a full load -> mutate -> save cycle per iteration where
 * both libraries expose the same operation. better-pdf-only scenarios exercise
 * features pdf-lib does not directly match, such as visual signature image
 * stamping through this package's high-level API.
 */
import { readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { PdfDocument, PageSizes, rgb as bpRgb } from "../src/index.ts";
import { PDFDocument, StandardFonts, rgb as plRgb } from "pdf-lib";
import type { FieldInfo, FieldType } from "../src/forms/form.ts";

const ITER = Number(process.env.BENCH_ITER ?? 25);
const WARMUP = Number(process.env.BENCH_WARMUP ?? 3);

const fixtures = [
  {
    label: "small mixed form",
    path: join(
      import.meta.dir,
      "../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
    ),
  },
  {
    label: "medium dense form",
    path: join(import.meta.dir, "../tests/fixtures/Medicamentos/Modulo-de-Diabetes.pdf"),
  },
  {
    label: "large signature form",
    path: join(
      import.meta.dir,
      "../tests/fixtures/Discapacidad/Convenio-OSFATUN-Discapacidad-2022.pdf",
    ),
  },
];

const signatureImage = new Uint8Array(readFileSync(join(import.meta.dir, "../signature.jpg")));

// Minimal valid 1×1 RGBA PNG used by the "create + draw image" scenario.
const TINY_PNG = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
  0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
  0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00,
  0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
  0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00,
  0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
]);

let sink = 0;

type BenchFn = () => Promise<void>;

interface Scenario {
  name: string;
  better: BenchFn;
  pdflib?: BenchFn;
}

interface FixtureBench {
  label: string;
  file: string;
  bytes: Uint8Array;
  fields: FieldInfo[];
  textNames: string[];
  radioOps: { name: string; value: string }[];
  checkboxNames: string[];
  dropdownOps: { name: string; value: string }[];
  signatureNames: string[];
}

async function mean(fn: BenchFn): Promise<number> {
  for (let i = 0; i < WARMUP; i++) await fn();
  const t0 = performance.now();
  for (let i = 0; i < ITER; i++) await fn();
  return (performance.now() - t0) / ITER;
}

async function tryMean(fn: BenchFn): Promise<number | Error> {
  try {
    return await mean(fn);
  } catch (error) {
    return error instanceof Error ? error : new Error(String(error));
  }
}

function countByType(fields: FieldInfo[]): string {
  const counts = new Map<FieldType, number>();
  for (const field of fields) counts.set(field.type, (counts.get(field.type) ?? 0) + 1);
  return [...counts.entries()].map(([type, count]) => `${type}:${count}`).join(", ");
}

function remember(bytes: Uint8Array): void {
  sink ^= bytes.length;
}

async function canUsePdfLibText(bytes: Uint8Array, name: string): Promise<boolean> {
  try {
    (await PDFDocument.load(bytes)).getForm().getTextField(name);
    return true;
  } catch {
    return false;
  }
}

async function inspectFixture(label: string, path: string): Promise<FixtureBench> {
  const bytes = new Uint8Array(readFileSync(path));
  const form = (await PdfDocument.load(bytes)).getForm();
  const fields = form.getFields().filter((field) => !field.readOnly);
  const textNames: string[] = [];

  for (const field of fields.filter((field) => field.type === "text")) {
    if (await canUsePdfLibText(bytes, field.name)) textNames.push(field.name);
    if (textNames.length >= 24) break;
  }

  return {
    label,
    file: basename(path),
    bytes,
    fields,
    textNames,
    radioOps: fields
      .filter((field) => field.type === "radio" && field.states.length > 0)
      .slice(0, 4)
      .map((field) => ({ name: field.name, value: field.states[0]! })),
    checkboxNames: fields.filter((field) => field.type === "checkbox").slice(0, 12).map((f) => f.name),
    dropdownOps: fields
      .filter((field) => field.type === "dropdown" && field.options.length > 0)
      .slice(0, 4)
      .map((field) => ({ name: field.name, value: field.options[0]! })),
    signatureNames: fields.filter((field) => field.type === "signature").slice(0, 2).map((f) => f.name),
  };
}

function scenarioList(fixture: FixtureBench): Scenario[] {
  const scenarios: Scenario[] = [
    {
      name: "load + save unchanged",
      better: async () => {
        remember(await (await PdfDocument.load(fixture.bytes)).save());
      },
      pdflib: async () => {
        remember(await (await PDFDocument.load(fixture.bytes)).save());
      },
    },
    {
      name: "load + read fields",
      better: async () => {
        sink ^= (await PdfDocument.load(fixture.bytes)).getForm().getFields().length;
      },
      pdflib: async () => {
        sink ^= (await PDFDocument.load(fixture.bytes)).getForm().getFields().length;
      },
    },
  ];

  if (fixture.textNames.length > 0) {
    scenarios.push({
      name: `fill ${fixture.textNames.length} text fields + save`,
      better: async () => {
        const doc = await PdfDocument.load(fixture.bytes);
        const form = doc.getForm();
        for (const name of fixture.textNames) form.getTextField(name).setText("X");
        remember(await doc.save());
      },
      pdflib: async () => {
        const doc = await PDFDocument.load(fixture.bytes);
        const form = doc.getForm();
        for (const name of fixture.textNames) form.getTextField(name).setText("X");
        remember(await doc.save());
      },
    });
  }

  if (
    fixture.radioOps.length > 0 ||
    fixture.checkboxNames.length > 0 ||
    fixture.dropdownOps.length > 0
  ) {
    const opCount =
      fixture.radioOps.length + fixture.checkboxNames.length + fixture.dropdownOps.length;
    scenarios.push({
      name: `fill ${opCount} choice fields + save`,
      better: async () => {
        const doc = await PdfDocument.load(fixture.bytes);
        const form = doc.getForm();
        for (const op of fixture.radioOps) form.getRadioGroup(op.name).select(op.value);
        for (const name of fixture.checkboxNames) form.getCheckBox(name).check();
        for (const op of fixture.dropdownOps) form.getDropdown(op.name).select(op.value);
        remember(await doc.save());
      },
      pdflib: async () => {
        const doc = await PDFDocument.load(fixture.bytes);
        const form = doc.getForm();
        for (const op of fixture.radioOps) form.getRadioGroup(op.name).select(op.value);
        for (const name of fixture.checkboxNames) form.getCheckBox(name).check();
        for (const op of fixture.dropdownOps) form.getDropdown(op.name).select(op.value);
        remember(await doc.save());
      },
    });
  }

  if (fixture.signatureNames.length > 0) {
    scenarios.push({
      name: `stamp ${fixture.signatureNames.length} signature image(s) + save`,
      better: async () => {
        const doc = await PdfDocument.load(fixture.bytes);
        const form = doc.getForm();
        for (const name of fixture.signatureNames) form.getSignature(name).setImage(signatureImage);
        remember(await doc.save());
      },
    });

    scenarios.push({
      name: "stamp first signature + flatten it",
      better: async () => {
        const doc = await PdfDocument.load(fixture.bytes);
        const form = doc.getForm();
        const name = fixture.signatureNames[0]!;
        form.getSignature(name).setImage(signatureImage);
        form.flattenField(name);
        remember(await doc.save());
      },
    });
  }

  scenarios.push({
    name: "flatten all + save",
    better: async () => {
      const doc = await PdfDocument.load(fixture.bytes);
      doc.getForm().flatten();
      remember(await doc.save());
    },
    pdflib: async () => {
      const doc = await PDFDocument.load(fixture.bytes);
      doc.getForm().flatten();
      remember(await doc.save());
    },
  });

  return scenarios;
}

console.log(`Iterations: ${ITER} (after ${WARMUP} warmup)\n`);

for (const fixtureInfo of fixtures) {
  const fixture = await inspectFixture(fixtureInfo.label, fixtureInfo.path);
  console.log(`### ${fixture.label}`);
  console.log(
    `${fixture.file} (${fixture.bytes.length.toLocaleString()} bytes, ${fixture.fields.length} fields: ${countByType(fixture.fields)})\n`,
  );
  console.log("| Scenario | better-pdf | pdf-lib | speedup |");
  console.log("| --- | ---: | ---: | ---: |");

  for (const scenario of scenarioList(fixture)) {
    const better = await tryMean(scenario.better);
    if (better instanceof Error) {
      console.log(`| ${scenario.name} | error: ${better.message} | n/a | n/a |`);
      continue;
    }

    if (!scenario.pdflib) {
      console.log(`| ${scenario.name} | ${better.toFixed(2)} ms | n/a | n/a |`);
      continue;
    }

    const pdflib = await tryMean(scenario.pdflib);
    if (pdflib instanceof Error) {
      console.log(`| ${scenario.name} | ${better.toFixed(2)} ms | error: ${pdflib.message} | n/a |`);
      continue;
    }

    console.log(
      `| ${scenario.name} | ${better.toFixed(2)} ms | ${pdflib.toFixed(2)} ms | ${(pdflib / better).toFixed(1)}x |`,
    );
  }

  console.log("");
}

// ---------------------------------------------------------------------------
// PDF generation scenarios (no fixture required)
// ---------------------------------------------------------------------------

const smallFixtureBytes = new Uint8Array(
  readFileSync(fixtures[0]!.path),
);

const generationScenarios: Scenario[] = [
  {
    name: "create + draw text",
    better: async () => {
      const doc = await PdfDocument.create();
      const page = doc.addPage(PageSizes.A4);
      for (let i = 0; i < 20; i++) {
        page.drawText(`Line ${i + 1}`, { x: 50, y: 800 - i * 20, size: 12 });
      }
      remember(await doc.save());
    },
    pdflib: async () => {
      const doc = await PDFDocument.create();
      const font = await doc.embedFont(StandardFonts.Helvetica);
      const page = doc.addPage([595.28, 841.89]);
      for (let i = 0; i < 20; i++) {
        page.drawText(`Line ${i + 1}`, { x: 50, y: 800 - i * 20, size: 12, font });
      }
      remember(new Uint8Array(await doc.save()));
    },
  },
  {
    name: "stamp text on existing",
    better: async () => {
      const doc = await PdfDocument.load(smallFixtureBytes);
      const page = doc.getPage(0);
      for (let i = 0; i < 5; i++) {
        page.drawText("STAMP", { x: 50, y: 50 + i * 20, size: 14 });
      }
      remember(await doc.save());
    },
    pdflib: async () => {
      const doc = await PDFDocument.load(smallFixtureBytes);
      const font = await doc.embedFont(StandardFonts.Helvetica);
      const page = doc.getPages()[0]!;
      for (let i = 0; i < 5; i++) {
        page.drawText("STAMP", { x: 50, y: 50 + i * 20, size: 14, font });
      }
      remember(new Uint8Array(await doc.save()));
    },
  },
  {
    name: "create + draw image",
    better: async () => {
      const doc = await PdfDocument.create();
      const page = doc.addPage(PageSizes.A4);
      const img = await doc.embedPng(TINY_PNG);
      page.drawImage(img, { x: 50, y: 50, width: 100, height: 100 });
      remember(await doc.save());
    },
    pdflib: async () => {
      const doc = await PDFDocument.create();
      const page = doc.addPage([595.28, 841.89]);
      const img = await doc.embedPng(TINY_PNG);
      page.drawImage(img, { x: 50, y: 50, width: 100, height: 100 });
      remember(new Uint8Array(await doc.save()));
    },
  },
  {
    name: "create + vector shapes",
    better: async () => {
      const doc = await PdfDocument.create();
      const page = doc.addPage(PageSizes.A4);
      for (let i = 0; i < 4; i++) {
        page.drawRectangle({
          x: 50 + i * 60,
          y: 600,
          width: 50,
          height: 40,
          color: bpRgb(0.8, 0.2, 0.2),
          borderColor: bpRgb(0, 0, 0),
        });
      }
      for (let i = 0; i < 3; i++) {
        page.drawLine({
          start: { x: 50, y: 550 - i * 20 },
          end: { x: 300, y: 550 - i * 20 },
          thickness: 1,
          color: bpRgb(0, 0, 0.8),
        });
      }
      for (let i = 0; i < 3; i++) {
        page.drawEllipse({
          x: 100 + i * 80,
          y: 480,
          xScale: 30,
          yScale: 20,
          color: bpRgb(0.2, 0.7, 0.2),
          borderColor: bpRgb(0, 0, 0),
        });
      }
      remember(await doc.save());
    },
    // no pdf-lib equivalent — marked better-pdf-only (no pdflib field)
  },
];

console.log("### PDF generation");
console.log("(no fixture — create or stamp from scratch)\n");
console.log("| Scenario | better-pdf | pdf-lib | speedup |");
console.log("| --- | ---: | ---: | ---: |");

for (const scenario of generationScenarios) {
  const better = await tryMean(scenario.better);
  if (better instanceof Error) {
    console.log(`| ${scenario.name} | error: ${better.message} | n/a | n/a |`);
    continue;
  }

  if (!scenario.pdflib) {
    console.log(`| ${scenario.name} | ${better.toFixed(2)} ms | n/a | n/a |`);
    continue;
  }

  const pdflib = await tryMean(scenario.pdflib);
  if (pdflib instanceof Error) {
    console.log(
      `| ${scenario.name} | ${better.toFixed(2)} ms | error: ${pdflib.message} | n/a |`,
    );
    continue;
  }

  console.log(
    `| ${scenario.name} | ${better.toFixed(2)} ms | ${pdflib.toFixed(2)} ms | ${(pdflib / better).toFixed(1)}x |`,
  );
}

console.log("");

if (sink === Number.MIN_SAFE_INTEGER) console.log("ignore", sink);
