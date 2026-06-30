// Single import point for the generated WASM bindings on server runtimes
// (Node/Bun). Uses the `--target web` build: the binary is read from disk and
// instantiated synchronously, so this module keeps initializing on import.
import { readFileSync } from "node:fs";
import * as raw from "../../pkg-web/better_pdf_core.js";
import { makeBindings } from "./wasm-bindings.js";

raw.initSync({
  module: readFileSync(new URL("../../pkg-web/better_pdf_core_bg.wasm", import.meta.url)),
});

// No guard needed: the module is initialized synchronously above before any
// of these are called.
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
} = makeBindings(raw);
