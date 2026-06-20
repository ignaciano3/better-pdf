// Generates tests/fixtures/generated/ficha-objstreams-updated.pdf
//
// This fixture is produced by our OWN core's incremental save so the output
// has a base xref-stream (from ficha-objstreams.pdf) plus an appended update
// section with /Prev — i.e. two `startxref` occurrences.
//
// Run AFTER `bun run build`:
//   bun run scripts/make-objstream-update-fixture.ts
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const ROOT = join(import.meta.dir, "..");
const BASE = join(ROOT, "tests/fixtures/generated/ficha-objstreams.pdf");
const OUT = join(ROOT, "tests/fixtures/generated/ficha-objstreams-updated.pdf");

const doc = await PdfDocument.load(new Uint8Array(readFileSync(BASE)));
doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("TEST_UPDATE");
const bytes = await doc.save();
writeFileSync(OUT, bytes);
console.log(`ficha-objstreams-updated.pdf: ${bytes.length} bytes`);
