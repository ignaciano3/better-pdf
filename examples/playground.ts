/**
 * better-pdf playground — a scratch file for trying the library by hand.
 *
 * Run it with bun:
 *
 *   bun run play                       # uses a bundled fixture
 *   bun run play path/to/your.pdf      # uses your own PDF
 *
 * What it currently does (Milestone 1): loads a PDF through the Rust/WASM core
 * and saves it back out, proving a byte-exact round-trip. As later milestones
 * land (read fields, fill, flatten, sign), extend this file to play with them.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const DEFAULT_FIXTURE = join(
  import.meta.dir,
  "../tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 1.pdf",
);

// First CLI arg is an optional path to your own PDF.
const inputPath = process.argv[2] ?? DEFAULT_FIXTURE;
const outputPath = join(import.meta.dir, `out-${basename(inputPath)}`);

const original = new Uint8Array(readFileSync(inputPath));
console.log(`Loaded:   ${inputPath} (${original.length.toLocaleString()} bytes)`);

const doc = await PdfDocument.load(original);
const saved = await doc.save();

const identical =
  saved.length === original.length &&
  Buffer.from(saved).equals(Buffer.from(original));

writeFileSync(outputPath, saved);
console.log(`Saved:    ${outputPath} (${saved.length.toLocaleString()} bytes)`);
console.log(`Round-trip byte-identical: ${identical ? "yes ✅" : "no ❌"}`);

const form = doc.getForm();
const fields = form.getFields();
console.log(`\nAcroForm fields: ${fields.length}`);
for (const f of fields.slice(0, 15)) {
  const extra =
    f.states.length ? ` states=${JSON.stringify(f.states)}` :
    f.options.length ? ` options=${JSON.stringify(f.options)}` : "";
  console.log(`  ${f.type.padEnd(10)} ${f.name}${extra}`);
}
if (fields.length > 15) console.log(`  ... and ${fields.length - 15} more`);
