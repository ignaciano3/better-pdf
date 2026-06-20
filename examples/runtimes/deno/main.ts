import { PdfDocument, rgb, StandardFonts } from "npm:@ignaciano3/better-pdf";

// Create a new PDF document
const doc = await PdfDocument.create();

// Add an A4 page (595 x 842 pts)
const page = doc.addPage();

// Draw some text
page.drawText("Hello from better-pdf + Deno!", {
  x: 50,
  y: page.height - 100,
  size: 24,
  font: StandardFonts.Helvetica,
  color: rgb(0.6, 0.1, 0.8),
});

page.drawText("Generated with @ignaciano3/better-pdf", {
  x: 50,
  y: page.height - 140,
  size: 14,
  font: StandardFonts.HelveticaOblique,
  color: rgb(0.3, 0.3, 0.3),
});

// Save the document to a Uint8Array
const bytes = await doc.save();

// Write to disk using Deno.writeFile
await Deno.writeFile("out.pdf", bytes);

// Verify the output
const header = String.fromCharCode(...bytes.slice(0, 5));
console.log(`Bytes written : ${bytes.length}`);
console.log(`PDF header    : ${header}`);
console.log(`Starts with %PDF- : ${header === "%PDF-"}`);
console.log("Saved to out.pdf");
