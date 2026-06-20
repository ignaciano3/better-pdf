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

{
  // Larger object-stream file — stresses objstm decoding at higher object counts.
  // We load FICHA and embed the source page seven extra times as XObjects so that
  // the object graph is ~8x larger than the single-form file while the AcroForm
  // (and therefore all form fields, including beneficiario.apellidos_nombres) remains
  // intact. pdf-lib's embedPdf() adds independent XObjects per embedded copy, so
  // the resulting object table is substantially larger even if the number of ObjStm
  // streams themselves doesn't increase linearly.
  const doc = await PDFDocument.load(source);
  const pageSize = doc.getPage(0).getSize();
  for (let i = 0; i < 7; i++) {
    const embedded = await PDFDocument.load(source);
    const embPages = await doc.embedPdf(embedded, [0]);
    const embPage = embPages[0];
    if (!embPage) throw new Error("embedPdf returned no pages");
    const newPage = doc.addPage([pageSize.width, pageSize.height]);
    newPage.drawPage(embPage);
  }
  const bytes = await doc.save({ useObjectStreams: true, updateFieldAppearances: false });
  writeFileSync(join(OUT, "ficha-objstreams-big.pdf"), bytes);
  console.log(`ficha-objstreams-big.pdf: ${bytes.length} bytes`);
}

{
  // Minimal PDF with /Encrypt in the trailer — NOT genuinely encrypted.
  // This file exists solely to exercise EncryptedPdfError and trigger the
  // ENCRYPTED: prefix from the Rust core when it detects /Encrypt in the trailer.
  // Classic cross-reference layout so the trailer dict is plainly visible.
  const encryptObjNum = 1;
  const encryptObj =
    `${encryptObjNum} 0 obj\n<< /Filter /Standard /V 1 /R 2 >>\nendobj\n`;
  const bodyOffset = "%PDF-1.4\n".length;
  const encryptObjOffset = bodyOffset;
  const xrefOffset = bodyOffset + encryptObj.length;
  const xrefSection =
    `xref\n` +
    `0 2\n` +
    `0000000000 65535 f \n` +
    `${String(encryptObjOffset).padStart(10, "0")} 00000 n \n`;
  const trailer =
    `trailer\n` +
    `<< /Size 2 /Root 1 0 R /Encrypt ${encryptObjNum} 0 R >>\n` +
    `startxref\n` +
    `${xrefOffset}\n` +
    `%%EOF\n`;
  const content = `%PDF-1.4\n${encryptObj}${xrefSection}${trailer}`;
  writeFileSync(join(OUT, "encrypted-min.pdf"), content);
  console.log(`encrypted-min.pdf: ${content.length} bytes`);
}
