// Generates derived test fixtures from the real corpus using pdf-lib (devDep):
//   ficha-objstreams.pdf — same form re-saved with object + xref streams (PDF 1.5 layout)
//   ficha-xfa.pdf        — same form with a stub /XFA entry (XFA-hybrid marker)
// Run with `bun run fixtures:generate`; outputs are committed.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { PDFDocument, PDFName, PDFString } from "pdf-lib";

const ROOT = join(import.meta.dir, "..");
const FICHA = join(ROOT, "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
const OUT = join(ROOT, "tests/fixtures/generated");
mkdirSync(OUT, { recursive: true });

const source = readFileSync(FICHA);

{
  const doc = await PDFDocument.load(source);
  if (doc.getForm().getFields().length === 0) throw new Error("source lost its fields");
  // updateFieldAppearances:false keeps the original /DA + /DR font wiring intact;
  // this fixture exists to test the file layout (object/xref streams), nothing else.
  const bytes = await doc.save({ useObjectStreams: true, updateFieldAppearances: false });
  writeFileSync(join(OUT, "ficha-objstreams.pdf"), bytes);
  console.log(`ficha-objstreams.pdf: ${bytes.length} bytes`);
}

{
  const doc = await PDFDocument.load(source);
  // Call getForm() first so pdf-lib populates its form cache. This also strips any
  // pre-existing XFA key (with a console.warn) — that's expected.
  const form = doc.getForm();
  // Re-inject the /XFA stub key directly onto the raw AcroForm dict.
  // We must pass updateFieldAppearances:false to save() so that pdf-lib's
  // appearance-update path does not call doc.getForm() again per field
  // (PDFTextField.updateAppearances → PDFField → doc.getForm()), which would
  // strip our key a second time before serialisation.
  form.acroForm.dict.set(PDFName.of("XFA"), PDFString.of("stub-xfa-packet"));
  const bytes = await doc.save({ useObjectStreams: false, updateFieldAppearances: false });
  writeFileSync(join(OUT, "ficha-xfa.pdf"), bytes);
  console.log(`ficha-xfa.pdf: ${bytes.length} bytes`);
}
