/**
 * better-pdf playground — a guided tour of the library.
 *
 * Run it with bun:
 *
 *   bun run play                                  # uses bundled fixtures
 *   bun run play path/to/your.pdf                 # inspect / fill your own PDF
 *   bun run play path/to/your.pdf signature.jpg   # also test a signature image
 *
 * Every section is self-contained and prints what it does. Scratch PDFs are
 * written next to this file so you can open them and see the result.
 *
 * There are two mental models in better-pdf:
 *
 *   Existing PDF:  load(bytes) → getForm()/getPage() → queue edits → save()
 *   New PDF:       create()    → addPage()           → page.draw*() → save()
 *
 * In both cases nothing touches the bytes until `save()`; edits are queued and
 * applied in one pass, so calling save() twice with the same queue is stable.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import {
  generateFormTypes,
  grayscale,
  PageSizes,
  PdfDocument,
  rgb,
  StandardFonts,
} from "../src/index.ts";

// --- Fixtures & helpers ------------------------------------------------------

const FORM_FIXTURE = join(
  import.meta.dir,
  "../tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 1.pdf",
);
const SIGNATURE_FIXTURE = join(
  import.meta.dir,
  "../tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf",
);

// Minimal valid images, just to show the embed → draw flow end to end.
const TINY_JPEG = new Uint8Array([
  0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00,
  0x02, 0x00, 0x03, 0x03, 0x00, 0xff, 0xd9,
]);
const TINY_PNG = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
  0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
  0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
  0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
  0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
]);

// CLI: [2] optional path to your own PDF, [3] optional signature image path.
const inputPath = process.argv[2] ?? FORM_FIXTURE;
const signatureImagePath = process.argv[3] ?? process.env.SIGNATURE_JPEG;

function heading(title: string): void {
  console.log(`\n${"─".repeat(64)}\n${title}\n${"─".repeat(64)}`);
}

/** Save bytes next to this file and log it. Returns the full path. */
function save(name: string, bytes: Uint8Array): string {
  const path = join(import.meta.dir, name);
  writeFileSync(path, bytes);
  console.log(`  wrote ${name} (${bytes.length.toLocaleString()} bytes)`);
  return path;
}

// --- 1. Load and inspect an existing PDF ------------------------------------

heading("1. Load an existing PDF and read its AcroForm");

const original = new Uint8Array(readFileSync(inputPath));
console.log(`  loaded ${basename(inputPath)} (${original.length.toLocaleString()} bytes)`);

const doc = await PdfDocument.load(original);

// save() with no queued edits returns a byte-identical copy of the input.
const roundTrip = await doc.save();
const identical = Buffer.from(roundTrip).equals(Buffer.from(original));
console.log(`  no-op save is byte-identical: ${identical ? "yes ✅" : "no ❌"}`);

const fields = doc.getForm().getFields();
console.log(`  AcroForm fields: ${fields.length}`);
for (const f of fields.slice(0, 8)) {
  const extra =
    f.states.length ? ` states=${JSON.stringify(f.states)}` :
    f.options.length ? ` options=${JSON.stringify(f.options)}` : "";
  console.log(`    ${f.type.padEnd(10)} ${f.name}${extra}`);
}
if (fields.length > 8) console.log(`    … and ${fields.length - 8} more`);

// --- 2. Fill and flatten form fields ----------------------------------------

heading("2. Fill a field, then flatten it");

const firstText = fields.find((f) => f.type === "text" && !f.readOnly);
if (firstText) {
  // Edits are queued on the form, then baked into the PDF by save().
  doc.getForm().getTextField(firstText.name).setText("better-pdf was here");
  const filled = await doc.save();
  const reread = (await PdfDocument.load(filled)).getForm().getField(firstText.name);
  console.log(`  filled '${firstText.name}' → "${reread?.value}"`);
  save(`filled-${basename(inputPath)}`, filled);

  // Flattening turns the field into static page graphics (no more widget).
  doc.getForm().flattenField(firstText.name);
  save(`flat-${basename(inputPath)}`, await doc.save());
} else {
  console.log("  (no writable text field in this PDF — skipping)");
}

// --- 3. Place a visual signature image ---------------------------------------

heading("3. Stamp a visual signature image into a signature field");

const sigPath = fields.some((f) => f.type === "signature") ? inputPath : SIGNATURE_FIXTURE;
const sigDoc = await PdfDocument.load(new Uint8Array(readFileSync(sigPath)));
const sigField = sigDoc.getForm().getFields().find((f) => f.type === "signature" && !f.readOnly);

if (sigField) {
  const image = signatureImagePath ? new Uint8Array(readFileSync(signatureImagePath)) : TINY_JPEG;
  // Visual only — no cryptographic signature dictionary is created.
  sigDoc.getForm().getSignature(sigField.name).setImage(image);
  save(`signed-${basename(sigPath)}`, await sigDoc.save());
  console.log(`  signed '${sigField.name}' with ${signatureImagePath ?? "an embedded tiny JPEG"}`);
  if (!signatureImagePath) {
    console.log("  tip: pass a JPEG path after the PDF, or set SIGNATURE_JPEG=…");
  }
} else {
  console.log("  (no signature field available — skipping)");
}

// --- 4. Generate a brand-new PDF from scratch -------------------------------

heading("4. Build a PDF from scratch: text, fonts, shapes, images");

// create() → addPage() gives you blank pages to draw on. Coordinates are in
// points with the origin at the bottom-left, y growing upward.
const made = await PdfDocument.create();
const page = made.addPage(PageSizes.A4); // also accepts a [width, height] tuple

// getFont returns a handle you can measure with and pass to drawText.
const titleFont = made.getFont(StandardFonts.HelveticaBold);
const title = "better-pdf — generated from scratch";
page.drawText(title, { x: 56, y: 780, size: 18, font: titleFont, color: rgb(0.1, 0.1, 0.4) });

// widthOfTextAtSize lets you lay out relative to measured text.
page.drawLine({
  start: { x: 56, y: 772 },
  end: { x: 56 + titleFont.widthOfTextAtSize(title, 18), y: 772 },
  strokeWidth: 1.5,
  stroke: rgb(0.1, 0.1, 0.4),
});

// Multiline text via "\n" + lineHeight. Default font is Helvetica.
page.drawText("Standard-14 fonts, WinAnsi text.\nUse \\n + lineHeight for multiple lines.", {
  x: 56, y: 732, size: 12, color: grayscale(0.2), lineHeight: 16,
});

// Vector shapes: fill color, border, and opacity (via an ExtGState).
page.drawRectangle({
  x: 56, y: 600, width: 220, height: 90,
  fill: rgb(0.85, 0.9, 1), stroke: rgb(0.1, 0.1, 0.4), strokeWidth: 1, opacity: 0.8,
});
page.drawEllipse({ x: 360, y: 645, xScale: 70, yScale: 40, fill: rgb(1, 0.6, 0.2) });

// Embed an image once, then draw it (here the same logo could be drawn N times).
const logo = await made.embedPng(TINY_PNG);
page.drawImage(logo, { x: 56, y: 520, width: 48, height: 48 });

// Add as many pages as you like.
made.addPage(PageSizes.Letter).drawText("Page 2 — Letter size", { x: 56, y: 720, size: 14 });

const madeBytes = await made.save();
console.log(`  built a ${(await PdfDocument.load(madeBytes)).getPageCount()}-page PDF`);
save("generated.pdf", madeBytes);

// --- 5. Draw on the pages of an existing PDF --------------------------------

heading("5. Stamp text onto an existing PDF's page");

// The same page.draw* API works on loaded documents, applied as an
// incremental update on top of the original content.
const stamp = await PdfDocument.load(original);
stamp.getPage(0).drawText("DRAFT", {
  x: 220, y: 400, size: 64, font: StandardFonts.HelveticaBold, color: rgb(1, 0, 0),
});
save(`stamped-${basename(inputPath)}`, await stamp.save());

// --- 6. Generate a typed module from a form's fields ------------------------

heading("6. Generate a TypeScript module typed to this form");

const formTypes = generateFormTypes(doc.getForm().getFields(), { typeName: "MyForm" });
save(`types-${basename(inputPath, ".pdf")}.ts`, new TextEncoder().encode(formTypes));
console.log(`  ${formTypes.split("\n").length} lines — import it and pass to getForm<typeof …>()`);

// --- 7. Generate a fillable form on a new PDF -------------------------------

heading("7. Build a brand-new fillable AcroForm");

// createForm() is only available on documents made with create(). Each add*
// call is chainable and accumulates the typed field-name schema.
const formDoc = await PdfDocument.create();
formDoc.addPage(PageSizes.A4);

const builder = formDoc
  .createForm()
  .addTextField("applicant.name", {
    page: 0, x: 56, y: 740, width: 240, height: 22,
    value: "GARCIA, IGNACIO",
    border: { color: rgb(0.1, 0.1, 0.4), width: 1 },
  })
  .addCheckBox("applicant.agree", {
    page: 0, x: 56, y: 700, size: 14, checked: true,
  });

console.log(`  declared fields: ${JSON.stringify(builder.getFieldNames())}`);

const formBytes = await formDoc.save();
save("generated-form.pdf", formBytes);

// Reload it: the saved document is a normal fillable AcroForm.
const reloaded = await PdfDocument.load(formBytes);
const reloadedFields = reloaded.getForm().getFields();
console.log(`  reloaded form has ${reloadedFields.length} field(s):`);
for (const f of reloadedFields) {
  console.log(`    ${f.type.padEnd(10)} ${f.name} = ${JSON.stringify(f.value)}`);
}

console.log("\nDone. Open the files above to see the results.");
