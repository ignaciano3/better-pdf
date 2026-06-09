// Single import point for the generated WASM bindings.
// Built with `wasm-pack --target nodejs`, so the module initializes synchronously on import.
// Later milestones may add a browser target behind this same module boundary.
import * as core from "../pkg/better_pdf_core.js";

export function roundTrip(data: Uint8Array): Uint8Array {
  return core.round_trip(data);
}

export function readFields(data: Uint8Array): string {
  return core.read_fields(data);
}
