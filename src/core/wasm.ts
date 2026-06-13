// Single import point for the generated WASM bindings on server runtimes
// (Node/Bun). Uses the `--target web` build: the binary is read from disk and
// instantiated synchronously, so this module keeps initializing on import.
import { readFileSync } from "node:fs";
import {
  initSync,
  apply_draw_ops,
  create_document,
  fill_fields,
  flatten_fields,
  image_info,
  read_fields,
  read_pages,
} from "../../pkg-web/better_pdf_core.js";

initSync({
  module: readFileSync(new URL("../../pkg-web/better_pdf_core_bg.wasm", import.meta.url)),
});

export function readFields(data: Uint8Array): string {
  return read_fields(data);
}

export function fillFields(data: Uint8Array, opsJson: string, images: Uint8Array): Uint8Array {
  return fill_fields(data, opsJson, images);
}

export function flattenFields(data: Uint8Array, namesJson: string): Uint8Array {
  return flatten_fields(data, namesJson);
}

export function readPages(data: Uint8Array): string {
  return read_pages(data);
}

export function applyDrawOps(data: Uint8Array, opsJson: string): Uint8Array {
  return apply_draw_ops(data, opsJson);
}

export function createDocument(opsJson: string, images: Uint8Array = new Uint8Array()): Uint8Array {
  return create_document(opsJson, images);
}

export function imageInfo(data: Uint8Array): string {
  return image_info(data);
}
