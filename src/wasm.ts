// Single import point for the generated WASM bindings on server runtimes
// (Node/Bun). Uses the `--target web` build: the binary is read from disk and
// instantiated synchronously, so this module keeps initializing on import.
import { readFileSync } from "node:fs";
import {
  initSync,
  fill_fields,
  flatten_fields,
  read_fields,
  round_trip,
} from "../pkg-web/better_pdf_core.js";

initSync({
  module: readFileSync(new URL("../pkg-web/better_pdf_core_bg.wasm", import.meta.url)),
});

export function roundTrip(data: Uint8Array): Uint8Array {
  return round_trip(data);
}

export function readFields(data: Uint8Array): string {
  return read_fields(data);
}

export function fillFields(data: Uint8Array, opsJson: string): Uint8Array {
  return fill_fields(data, opsJson);
}

export function flattenFields(data: Uint8Array, namesJson: string): Uint8Array {
  return flatten_fields(data, namesJson);
}
