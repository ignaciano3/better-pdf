// Build artifact for manual Factur-X structure acceptance (Task 6, Step 4 — controller's job).
// Not part of the automated suite; run with: bun tests/scripts/gen-facturx-check.ts
import { writeFileSync } from "node:fs";
import { PdfDocument } from "../../src/index.ts";

const enc = new TextEncoder();

const FACTURX_XML = `<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100">
  <rsm:ExchangedDocument>
    <ram:ID xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100">INV-2026-0001</ram:ID>
  </rsm:ExchangedDocument>
</rsm:CrossIndustryInvoice>
`;

const doc = await PdfDocument.create();
doc.addPage();
doc.attach(enc.encode(FACTURX_XML), "factur-x.xml", {
  mimeType: "text/xml",
  description: "Factur-X invoice data",
  afRelationship: "Alternative",
});
const out = await doc.save();

const outPath =
  "/private/tmp/claude-501/-Users-ignacio-Documents-proyectos-better-pdf/27e20071-0db5-414e-ac6f-cd93274cef83/scratchpad/facturx-check.pdf";
writeFileSync(outPath, out);
console.log(`Wrote ${outPath} (${out.length} bytes)`);
