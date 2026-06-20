"use client";

import { useCallback, useRef, useState } from "react";

// initializeWasm + PdfDocument are loaded dynamically so they are never
// evaluated on the server (Next.js SSR would fail because it can't run WASM).
async function loadAndGenerate(): Promise<Uint8Array> {
  const { PdfDocument, initializeWasm, rgb, StandardFonts } = await import(
    "@ignaciano3/better-pdf/browser"
  );

  // The .wasm file is copied into public/ by the postinstall script (or
  // manually — see README).  Next.js serves public/ at the root path.
  await initializeWasm("/better_pdf_core_bg.wasm");

  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawText("hello from Next.js", {
    x: 50,
    y: 700,
    size: 24,
    font: StandardFonts.Helvetica,
    color: rgb(0, 0, 1),
  });

  return doc.save();
}

export default function Home() {
  const [status, setStatus] = useState("Click the button to generate a PDF.");
  const iframeRef = useRef<HTMLIFrameElement>(null);

  const generate = useCallback(async () => {
    setStatus("Generating…");
    try {
      const bytes = await loadAndGenerate();
      // Cast: TS 5.9 strict Uint8Array<ArrayBufferLike> is not assignable to BlobPart
      const blob = new Blob([bytes.buffer as ArrayBuffer], { type: "application/pdf" });
      const url = URL.createObjectURL(blob);
      if (iframeRef.current) iframeRef.current.src = url;
      setStatus(`Done — ${bytes.length} bytes`);
    } catch (err) {
      setStatus(`Error: ${err}`);
      console.error(err);
    }
  }, []);

  return (
    <main style={{ fontFamily: "sans-serif", padding: "1rem" }}>
      <h1>better-pdf — Next.js example</h1>
      <button onClick={generate} style={{ padding: "0.5rem 1rem", fontSize: "1rem" }}>
        Generate PDF
      </button>
      <p style={{ color: "#555" }}>{status}</p>
      <iframe
        ref={iframeRef}
        title="PDF preview"
        style={{ width: "100%", height: 600, border: "1px solid #ccc", marginTop: "1rem" }}
      />
    </main>
  );
}
