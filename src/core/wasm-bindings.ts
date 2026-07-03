// Shared wrapper layer over the generated wasm-bindgen exports. Both the Node
// (wasm.ts) and browser (wasm-browser.ts) entry points build their CoreWasm
// from this factory; the only per-runtime differences are how the module is
// initialized and an optional pre-call `guard` (the browser checks that the
// async WASM init has completed). Keeping the 16 wrappers here means the two
// files can't drift, and a new core function is added in exactly one place.
import type { CoreWasm } from "./document.js";

/** The snake_case functions exported by the generated `better_pdf_core.js`. */
export interface RawBindings {
  decrypt_pdf(data: Uint8Array, password: string): Uint8Array;
  read_fields(data: Uint8Array): string;
  fill_fields(data: Uint8Array, opsJson: string, images: Uint8Array, compress: boolean): Uint8Array;
  flatten_fields(data: Uint8Array, namesJson: string, compress: boolean): Uint8Array;
  read_pages(data: Uint8Array): string;
  apply_draw_ops(
    data: Uint8Array,
    opsJson: string,
    images: Uint8Array,
    fonts: Uint8Array,
    fontsJson: string,
    compress: boolean,
  ): Uint8Array;
  apply_all(
    data: Uint8Array,
    planJson: string,
    fillImages: Uint8Array,
    drawImages: Uint8Array,
    fonts: Uint8Array,
    compress: boolean,
  ): Uint8Array;
  create_document(
    opsJson: string,
    images: Uint8Array,
    fonts: Uint8Array,
    fontsJson: string,
    fieldsJson: string,
    compress: boolean,
  ): Uint8Array;
  image_info(data: Uint8Array): string;
  measure_text(font: string, size: number, text: string): number;
  measure_text_embedded(font: Uint8Array, size: number, text: string): number;
  read_metadata(data: Uint8Array): string;
  set_metadata(data: Uint8Array, metaJson: string, compress: boolean): Uint8Array;
  manipulate_pages(
    docsBlob: Uint8Array,
    docsJson: string,
    planJson: string,
    compress: boolean,
  ): Uint8Array;
  set_outline(data: Uint8Array, json: string, compress: boolean): Uint8Array;
  insert_pages(data: Uint8Array, opsJson: string, compress: boolean): Uint8Array;
  inject_fields(
    data: Uint8Array,
    fieldsJson: string,
    fonts: Uint8Array,
    fontsJson: string,
    compress: boolean,
  ): Uint8Array;
}

const EMPTY = new Uint8Array();

/**
 * Build the {@link CoreWasm} surface from the raw generated bindings. `guard`
 * runs before every call (default no-op); the browser build passes a function
 * that throws if the async WASM init hasn't completed yet.
 */
export function makeBindings(raw: RawBindings, guard: () => void = () => {}): CoreWasm {
  return {
    decryptPdf: (data, password) => (guard(), raw.decrypt_pdf(data, password)),
    readFields: (data) => (guard(), raw.read_fields(data)),
    fillFields: (data, opsJson, images, compress = true) =>
      (guard(), raw.fill_fields(data, opsJson, images, compress)),
    flattenFields: (data, namesJson, compress = true) =>
      (guard(), raw.flatten_fields(data, namesJson, compress)),
    readPages: (data) => (guard(), raw.read_pages(data)),
    applyDrawOps: (data, opsJson, images = EMPTY, fonts = EMPTY, fontsJson = "[]", compress = true) =>
      (guard(), raw.apply_draw_ops(data, opsJson, images, fonts, fontsJson, compress)),
    applyAll: (
      data,
      planJson,
      fillImages = EMPTY,
      drawImages = EMPTY,
      fonts = EMPTY,
      compress = true,
    ) => (guard(), raw.apply_all(data, planJson, fillImages, drawImages, fonts, compress)),
    createDocument: (
      opsJson,
      images = EMPTY,
      fonts = EMPTY,
      fontsJson = "[]",
      fieldsJson = "[]",
      compress = true,
    ) => (guard(), raw.create_document(opsJson, images, fonts, fontsJson, fieldsJson, compress)),
    imageInfo: (data) => (guard(), raw.image_info(data)),
    measureText: (font, size, text) => (guard(), raw.measure_text(font, size, text)),
    measureTextEmbedded: (font, size, text) =>
      (guard(), raw.measure_text_embedded(font, size, text)),
    readMetadata: (data) => (guard(), raw.read_metadata(data)),
    setMetadata: (data, metaJson, compress = true) =>
      (guard(), raw.set_metadata(data, metaJson, compress)),
    manipulatePages: (docsBlob, docsJson, planJson, compress = true) =>
      (guard(), raw.manipulate_pages(docsBlob, docsJson, planJson, compress)),
    setOutline: (data, json, compress = true) => (guard(), raw.set_outline(data, json, compress)),
    insertPages: (data, opsJson, compress = true) =>
      (guard(), raw.insert_pages(data, opsJson, compress)),
    injectFields: (data, fieldsJson, fonts = EMPTY, fontsJson = "[]", compress = true) =>
      (guard(), raw.inject_fields(data, fieldsJson, fonts, fontsJson, compress)),
  };
}
