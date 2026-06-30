// Browser import point for the generated WASM bindings.
// Built with `wasm-pack --target web`, so callers must initialize before use.
import initCore, { type InitInput } from "../../pkg-web/better_pdf_core.js";
import * as raw from "../../pkg-web/better_pdf_core.js";
import { makeBindings } from "./wasm-bindings.js";

let initPromise: Promise<void> | undefined;
let initialized = false;

export function initializeWasm(moduleOrPath?: InitInput | Promise<InitInput>): Promise<void> {
  if (!initPromise || moduleOrPath !== undefined) {
    const source =
      moduleOrPath ?? new URL("../../pkg-web/better_pdf_core_bg.wasm", import.meta.url);
    initPromise = initCore({ module_or_path: source }).then(() => {
      initialized = true;
    });
  }
  return initPromise;
}

function ensureInitialized(): void {
  if (!initialized) {
    throw new Error(
      "better-pdf browser WASM is not initialized; await PdfDocument.load() or initializeWasm() first.",
    );
  }
}

export const {
  decryptPdf,
  readFields,
  fillFields,
  flattenFields,
  readPages,
  applyDrawOps,
  applyAll,
  createDocument,
  imageInfo,
  measureText,
  measureTextEmbedded,
  readMetadata,
  setMetadata,
  manipulatePages,
  setOutline,
  insertPages,
} = makeBindings(raw, ensureInitialized);
