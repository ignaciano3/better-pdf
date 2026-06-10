/**
 * better-pdf playground — a scratch file for trying the library by hand.
 *
 * Run it with bun:
 *
 *   bun run play                                  # uses bundled fixtures
 *   bun run play path/to/your.pdf                 # uses your own PDF
 *   bun run play path/to/your.pdf signature.jpg   # also tests visual signature image
 *
 * It exercises the main load -> inspect -> fill -> flatten -> visual-signature
 * flow and writes scratch PDFs next to this file.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { generateFormTypes, PdfDocument } from "../src/index.ts";

const DEFAULT_FIXTURE = join(
  import.meta.dir,
  "../tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 1.pdf",
);
const SIGNATURE_FIXTURE = join(
  import.meta.dir,
  "../tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf",
);
const TINY_JPEG = new Uint8Array([
  0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00,
  0x02, 0x00, 0x03, 0x03, 0x00, 0xff, 0xd9,
]);

// First CLI arg is an optional path to your own PDF.
const inputPath = process.argv[2] ?? DEFAULT_FIXTURE;
const signatureImagePath = process.argv[3] ?? process.env.SIGNATURE_JPEG;
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

// --- Milestone 3 demo: fill the first writable text field and re-read it. ---
const firstText = fields.find((f) => f.type === "text" && !f.readOnly);
if (firstText) {
  doc.getForm().getTextField(firstText.name).setText("better-pdf was here");
  const filled = await doc.save();
  const filledPath = join(import.meta.dir, `filled-${basename(inputPath)}`);
  writeFileSync(filledPath, filled);
  const check = (await PdfDocument.load(filled)).getForm().getField(firstText.name);
  console.log(`\nFilled '${firstText.name}' → "${check?.value}"`);
  console.log(`(value now has a baked appearance — /NeedAppearances cleared)`);
  console.log(`Wrote:    ${filledPath} (${filled.length.toLocaleString()} bytes)`);

  // --- Milestone 5 demo: flatten that field so it becomes page graphics. ---
  doc.getForm().flattenField(firstText.name);
  const flat = await doc.save();
  const flatPath = join(import.meta.dir, `flat-${basename(inputPath)}`);
  writeFileSync(flatPath, flat);
  const stillThere = (await PdfDocument.load(flat)).getForm().getField(firstText.name);
  console.log(`Flattened '${firstText.name}' → field present after flatten: ${stillThere ? "yes" : "no"}`);
  console.log(`Wrote:    ${flatPath} (${flat.length.toLocaleString()} bytes)`);
}

// --- Milestone 6 demo: place a visual-only JPEG signature image. ---
const signatureInputPath = fields.some((f) => f.type === "signature") ? inputPath : SIGNATURE_FIXTURE;
const signatureOriginal = new Uint8Array(readFileSync(signatureInputPath));
const signatureDoc = await PdfDocument.load(signatureOriginal);
const signatureForm = signatureDoc.getForm();
const firstSignature = signatureForm.getFields().find((f) => f.type === "signature" && !f.readOnly);

if (firstSignature) {
  const signatureImage = signatureImagePath
    ? new Uint8Array(readFileSync(signatureImagePath))
    : TINY_JPEG;

  signatureForm.getSignature(firstSignature.name).setImage(signatureImage);
  const signed = await signatureDoc.save();
  const signedPath = join(import.meta.dir, `signed-${basename(signatureInputPath)}`);
  writeFileSync(signedPath, signed);

  const reloaded = await PdfDocument.load(signed);
  const stillSignature = reloaded.getForm().getField(firstSignature.name);
  console.log(`\nSigned '${firstSignature.name}' with ${signatureImagePath ?? "embedded tiny JPEG"}`);
  console.log(`(visual only — no cryptographic signature dictionary is created)`);
  console.log(`Field still present before flatten: ${stillSignature ? "yes" : "no"}`);
  console.log(`Wrote:    ${signedPath} (${signed.length.toLocaleString()} bytes)`);

  signatureForm.flattenField(firstSignature.name);
  const signedFlat = await signatureDoc.save();
  const signedFlatPath = join(import.meta.dir, `signed-flat-${basename(signatureInputPath)}`);
  writeFileSync(signedFlatPath, signedFlat);
  const flattenedSignature = (await PdfDocument.load(signedFlat)).getForm().getField(firstSignature.name);
  console.log(`Flattened signed field → field present after flatten: ${flattenedSignature ? "yes" : "no"}`);
  console.log(`Wrote:    ${signedFlatPath} (${signedFlat.length.toLocaleString()} bytes)`);
  if (!signatureImagePath) {
    console.log(`Tip: pass a JPEG path after the PDF path, or set SIGNATURE_JPEG=/path/to/signature.jpg`);
  }
}

// --- Milestone 11 demo: generate a typed module from the form's fields. ---
const formTypes = generateFormTypes(signatureForm.getFields(), { typeName: "MyForm" });
const typesPath = join(import.meta.dir, `types-${basename(inputPath, ".pdf")}.ts`);
writeFileSync(typesPath, formTypes);
console.log(`\nWrote:    ${typesPath} (${formTypes.split("\n").length} lines of types)`);
