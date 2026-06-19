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
  manipulate_pages,
  measure_text,
  measure_text_embedded,
  read_fields,
  read_pages,
  read_metadata,
  set_metadata,
  set_outline,
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

export function applyDrawOps(
  data: Uint8Array,
  opsJson: string,
  images: Uint8Array = new Uint8Array(),
  fonts: Uint8Array = new Uint8Array(),
  fontsJson = "[]",
): Uint8Array {
  return apply_draw_ops(data, opsJson, images, fonts, fontsJson);
}

export function createDocument(
  opsJson: string,
  images: Uint8Array = new Uint8Array(),
  fonts: Uint8Array = new Uint8Array(),
  fontsJson = "[]",
  fieldsJson = "[]",
): Uint8Array {
  return create_document(opsJson, images, fonts, fontsJson, fieldsJson);
}

export function imageInfo(data: Uint8Array): string {
  return image_info(data);
}

export function measureText(font: string, size: number, text: string): number {
  return measure_text(font, size, text);
}

export function measureTextEmbedded(font: Uint8Array, size: number, text: string): number {
  return measure_text_embedded(font, size, text);
}

export function readMetadata(data: Uint8Array): string {
  return read_metadata(data);
}

export function setMetadata(data: Uint8Array, metaJson: string): Uint8Array {
  return set_metadata(data, metaJson);
}

export function manipulatePages(
  docsBlob: Uint8Array,
  docsJson: string,
  planJson: string,
): Uint8Array {
  return manipulate_pages(docsBlob, docsJson, planJson);
}

export function setOutline(data: Uint8Array, json: string): Uint8Array {
  return set_outline(data, json);
}
