/**
 * Cloudflare Workers example — @ignaciano3/better-pdf
 *
 * WHY the browser entry must be used:
 *   The default package entry ("@ignaciano3/better-pdf") auto-initialises the
 *   WASM binary via `readFileSync`, which requires `node:fs`.  Cloudflare
 *   Workers have no `node:fs` (and no Node.js compatibility for it), so the
 *   default entry fails at startup.
 *
 *   The "/browser" entry exports all the same public API but skips the
 *   `readFileSync` self-init, leaving WASM initialisation to the caller.
 *
 * WHY the wasm is imported as a module (not fetched at runtime):
 *   Cloudflare Workers do not allow fetching arbitrary binaries at runtime;
 *   the WASM file must be bundled as a WebAssembly.Module binding.  When
 *   wrangler (esbuild under the hood) sees `import wasmModule from "…/wasm"`,
 *   it compiles the referenced .wasm into a WebAssembly.Module and injects it
 *   as a module-level binding — no fetch, no filesystem access required.
 *
 *   The "./wasm" subpath export in @ignaciano3/better-pdf resolves to the
 *   raw .wasm file, which wrangler's CompiledWasm rule then picks up.
 */

// Use the browser entry: no node:fs dependency.
import { PdfDocument, initializeWasm, StandardFonts } from "@ignaciano3/better-pdf/browser";

// wrangler / esbuild compiles this .wasm import into a WebAssembly.Module
// binding at bundle time — no runtime fetch or filesystem access needed.
import wasmModule from "@ignaciano3/better-pdf/wasm";

export default {
  async fetch(): Promise<Response> {
    // Pass the pre-compiled Module directly; initializeWasm() accepts both
    // a WebAssembly.Module and a fetch-compatible Response/URL.
    await initializeWasm(wasmModule);

    const doc = await PdfDocument.create();
    const page = doc.addPage();
    page.drawText("hello from a worker", {
      x: 50,
      y: 700,
      size: 24,
      font: StandardFonts.Helvetica,
    });
    const bytes = await doc.save();

    return new Response(bytes, {
      headers: { "content-type": "application/pdf" },
    });
  },
};
