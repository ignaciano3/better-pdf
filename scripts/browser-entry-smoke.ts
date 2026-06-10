import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../dist/index.browser.js";

const fixture = join(
  import.meta.dir,
  "../tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf",
);
const bytes = new Uint8Array(readFileSync(fixture));
const doc = await PdfDocument.load(bytes);
const fields = doc.getForm().getFields();

if (fields.length === 0) {
  throw new Error("browser entry loaded no fields");
}

console.log(`browser entry loaded ${fields.length} fields`);
