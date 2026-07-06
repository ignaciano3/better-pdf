// Build artifact for manual visual acceptance (Task 6, Step 3 — controller's job).
// Not part of the automated suite; run with: bun tests/scripts/gen-cjk-visual-check.ts
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../../src/index.ts";

const NOTO_JP = new Uint8Array(
  readFileSync(join(import.meta.dir, "../fixtures/fonts/NotoSansJP-Regular.subset.ttf")),
);
const FICHA = join(import.meta.dir, "../fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
const font = await doc.embedFont(NOTO_JP);
doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("山田太郎", { font });
doc.getForm().flatten();
const out = await doc.save();

const outPath =
  "/private/tmp/claude-501/-Users-ignacio-Documents-proyectos-better-pdf/27e20071-0db5-414e-ac6f-cd93274cef83/scratchpad/cjk-visual-check.pdf";
writeFileSync(outPath, out);
console.log(`Wrote ${outPath} (${out.length} bytes)`);
