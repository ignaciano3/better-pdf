// Browser import point for the generated WASM bindings.
// Built with `wasm-pack --target web`, so callers must initialize before use.
import initCore, {
  fill_fields,
  flatten_fields,
  read_fields,
  round_trip,
  type InitInput,
} from "../pkg-web/better_pdf_core.js";

let initPromise: Promise<void> | undefined;
let initialized = false;

export function initializeWasm(moduleOrPath?: InitInput | Promise<InitInput>): Promise<void> {
  if (!initPromise || moduleOrPath !== undefined) {
    const source =
      moduleOrPath ?? new URL("../pkg-web/better_pdf_core_bg.wasm", import.meta.url);
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

export function roundTrip(data: Uint8Array): Uint8Array {
  ensureInitialized();
  return round_trip(data);
}

export function readFields(data: Uint8Array): string {
  ensureInitialized();
  return read_fields(data);
}

export function fillFields(data: Uint8Array, opsJson: string, images: Uint8Array): Uint8Array {
  ensureInitialized();
  return fill_fields(data, opsJson, images);
}

export function flattenFields(data: Uint8Array, namesJson: string): Uint8Array {
  ensureInitialized();
  return flatten_fields(data, namesJson);
}
