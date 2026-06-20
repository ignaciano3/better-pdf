import {
  PdfDocument,
  initializeWasm,
  rgb,
  StandardFonts,
} from "@ignaciano3/better-pdf/browser";

// webpack 5: `new URL(specifier, import.meta.url)` emits the asset and
// resolves to its output URL at runtime — no loader plugin required.
const wasmUrl = new URL(
  "@ignaciano3/better-pdf/wasm",
  import.meta.url
);

let initialized = false;

async function generatePdf() {
  if (!initialized) {
    await initializeWasm(wasmUrl.href);
    initialized = true;
  }

  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from webpack", {
    x: 50,
    y: 700,
    size: 24,
    font: StandardFonts.Helvetica,
    color: rgb(0, 0, 1),
  });

  return doc.save();
}

document.getElementById("generate").addEventListener("click", async () => {
  const status = document.getElementById("status");
  const preview = document.getElementById("preview");

  status.textContent = "Generating…";
  try {
    const bytes = await generatePdf();
    const blob = new Blob([bytes], { type: "application/pdf" });
    const url = URL.createObjectURL(blob);
    preview.src = url;
    status.textContent = `Done — ${bytes.length} bytes`;
  } catch (err) {
    status.textContent = `Error: ${err}`;
    console.error(err);
  }
});
