import {
  PdfDocument,
  initializeWasm,
  rgb,
  StandardFonts,
} from "@ignaciano3/better-pdf/browser";
// Vite resolves `?url` imports to the final asset URL (works in dev + build).
import wasmUrl from "@ignaciano3/better-pdf/wasm?url";

let initialized = false;

async function generatePdf(): Promise<Uint8Array> {
  if (!initialized) {
    await initializeWasm(wasmUrl);
    initialized = true;
  }

  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from vite", {
    x: 50,
    y: 700,
    size: 24,
    font: StandardFonts.Helvetica,
    color: rgb(0, 0, 1),
  });

  return doc.save();
}

document.getElementById("generate")!.addEventListener("click", async () => {
  const status = document.getElementById("status")!;
  const preview = document.getElementById("preview") as HTMLIFrameElement;

  status.textContent = "Generating…";
  try {
    const bytes = await generatePdf();
    // Cast: TS 5.9 strict Uint8Array<ArrayBufferLike> is not assignable to BlobPart
    const blob = new Blob([bytes.buffer as ArrayBuffer], { type: "application/pdf" });
    const url = URL.createObjectURL(blob);
    preview.src = url;
    status.textContent = `Done — ${bytes.length} bytes`;
  } catch (err) {
    status.textContent = `Error: ${err}`;
    console.error(err);
  }
});
