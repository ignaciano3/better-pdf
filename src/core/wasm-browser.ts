// Browser import point for the generated WASM bindings.
// Built with `wasm-pack --target web`, so callers must initialize before use.
import initCore, {
  apply_draw_ops,
  create_document,
  decrypt_pdf,
  fill_fields,
  flatten_fields,
  image_info,
  insert_pages,
  manipulate_pages,
  measure_text,
  measure_text_embedded,
  read_fields,
  read_pages,
  read_metadata,
  set_metadata,
  set_outline,
  type InitInput,
} from "../../pkg-web/better_pdf_core.js";

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

export function decryptPdf(data: Uint8Array, password: string): Uint8Array {
  ensureInitialized();
  return decrypt_pdf(data, password);
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

export function readPages(data: Uint8Array): string {
  ensureInitialized();
  return read_pages(data);
}

export function applyDrawOps(
  data: Uint8Array,
  opsJson: string,
  images: Uint8Array = new Uint8Array(),
  fonts: Uint8Array = new Uint8Array(),
  fontsJson = "[]",
): Uint8Array {
  ensureInitialized();
  return apply_draw_ops(data, opsJson, images, fonts, fontsJson);
}

export function createDocument(
  opsJson: string,
  images: Uint8Array = new Uint8Array(),
  fonts: Uint8Array = new Uint8Array(),
  fontsJson = "[]",
  fieldsJson = "[]",
): Uint8Array {
  ensureInitialized();
  return create_document(opsJson, images, fonts, fontsJson, fieldsJson);
}

export function imageInfo(data: Uint8Array): string {
  ensureInitialized();
  return image_info(data);
}

export function measureText(font: string, size: number, text: string): number {
  ensureInitialized();
  return measure_text(font, size, text);
}

export function measureTextEmbedded(font: Uint8Array, size: number, text: string): number {
  ensureInitialized();
  return measure_text_embedded(font, size, text);
}

export function readMetadata(data: Uint8Array): string {
  ensureInitialized();
  return read_metadata(data);
}

export function setMetadata(data: Uint8Array, metaJson: string): Uint8Array {
  ensureInitialized();
  return set_metadata(data, metaJson);
}

export function manipulatePages(
  docsBlob: Uint8Array,
  docsJson: string,
  planJson: string,
): Uint8Array {
  ensureInitialized();
  return manipulate_pages(docsBlob, docsJson, planJson);
}

export function setOutline(data: Uint8Array, json: string): Uint8Array {
  ensureInitialized();
  return set_outline(data, json);
}

export function insertPages(data: Uint8Array, opsJson: string): Uint8Array {
  ensureInitialized();
  return insert_pages(data, opsJson);
}
