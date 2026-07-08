use wasm_bindgen::prelude::*;

mod appearance;
mod apply;
mod attach;
mod compress;
pub mod create;
mod doc_io;
mod draw;
mod embed;
mod fill;
mod flatten;
mod font_metrics;
mod fonts;
mod forms;
mod inject;
mod metadata;
mod outline;
mod pageops;
mod pages;
mod pagetree;

/// Read the AcroForm fields of a PDF, returned as a JSON array string.
#[wasm_bindgen]
pub fn read_fields(data: &[u8]) -> Result<String, JsError> {
    forms::read_fields_json(data).map_err(|e| JsError::new(&e))
}

/// Apply fill ops (JSON array of {name, value | imageOffset+imageLength}) to a
/// PDF and return new bytes. `images` is the concatenated image blob the
/// offsets index into.
#[wasm_bindgen]
pub fn fill_fields(
    data: &[u8],
    ops_json: &str,
    images: &[u8],
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    fill::fill_fields_json(data, ops_json, images, compress).map_err(|e| JsError::new(&e))
}

/// Flatten the named fields (JSON array of names) and return new PDF bytes.
#[wasm_bindgen]
pub fn flatten_fields(
    data: &[u8],
    names_json: &str,
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    flatten::flatten_fields_json(data, names_json, compress).map_err(|e| JsError::new(&e))
}

/// Read the pages of a PDF, returned as a JSON array of `{index, width, height, rotation}`.
#[wasm_bindgen]
pub fn read_pages(data: &[u8]) -> Result<String, JsError> {
    pages::read_pages_json(data).map_err(|e| JsError::new(&e))
}

/// Apply draw ops (JSON array of text/image commands) to an existing PDF and
/// return new bytes (incremental save). `images` is the concatenated image blob
/// that Image ops index into via imageOffset / imageLength.
#[wasm_bindgen]
pub fn apply_draw_ops(
    data: &[u8],
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    draw::apply_draw_ops_json(data, ops_json, images, fonts, fonts_json, compress)
        .map_err(|e| JsError::new(&e))
}

/// Apply fill, flatten, draw, metadata, and outline operations to a loaded PDF
/// in a single parse → mutate → serialize pass. `plan_json` is an object with
/// optional `fill`, `flatten`, `draw` ({ ops, fonts }), `metadata`, and
/// `outline` keys. `fill_images` / `draw_images` / `fonts` are the binary blobs
/// referenced by fill ops, draw ops, and embedded fonts respectively (pass
/// `&[]` for none). Page-structure ops are NOT handled here.
#[wasm_bindgen]
pub fn apply_all(
    data: &[u8],
    plan_json: &str,
    fill_images: &[u8],
    draw_images: &[u8],
    fonts: &[u8],
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    apply::apply_all_json(data, plan_json, fill_images, draw_images, fonts, compress)
        .map_err(|e| JsError::new(&e))
}

/// Attach embedded files (JSON array of {name, mimeType?, description?,
/// creationDate?, modificationDate?, afRelationship?, offset, length}) to a
/// PDF; `blob` is the concatenated file bytes the offsets index into.
/// Returns new bytes (incremental update).
#[wasm_bindgen]
pub fn attach_files(
    data: &[u8],
    ops_json: &str,
    blob: &[u8],
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    attach::attach_files_json(data, ops_json, blob, compress).map_err(|e| JsError::new(&e))
}

/// Read every /EmbeddedFiles attachment. Returns a packed buffer:
/// `[u32 LE json_len][json][concatenated file bytes]`, where the JSON is an
/// array of metadata objects whose `offset`/`length` index the bytes section.
#[wasm_bindgen]
pub fn read_attachments(data: &[u8]) -> Result<Vec<u8>, JsError> {
    attach::read_attachments_packed(data).map_err(|e| JsError::new(&e))
}

/// Build a new PDF document from scratch using a JSON array of create ops
/// (addPage, text, image, etc.) and return the PDF bytes. `images` is the
/// concatenated image blob that Image ops index into via imageOffset / imageLength.
/// `fonts` is the concatenated font blob that embedded-font text ops index into
/// via `fonts_json` descriptors (pass `&[]` / "[]" for none).
/// `fields_json` is a JSON array of field definitions (pass "[]" for none).
/// `compress` deflates generated streams (default-on at the TS layer);
/// `object_streams` packs non-stream objects into PDF object streams for
/// smaller output (default-off).
#[wasm_bindgen]
pub fn create_document(
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    fields_json: &str,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, JsError> {
    create::create_document_json(
        ops_json, images, fonts, fonts_json, fields_json, compress, object_streams,
    )
    .map_err(|e| JsError::new(&e))
}

/// Inject new AcroForm fields (JSON array of field defs, same schema as
/// create_document's fields_json) into a loaded PDF; returns new bytes.
/// `fonts` / `fonts_json` carry embedded fonts referenced by fields.
#[wasm_bindgen]
pub fn inject_fields(
    data: &[u8],
    fields_json: &str,
    fonts: &[u8],
    fonts_json: &str,
    compress: bool,
) -> Result<Vec<u8>, JsError> {
    inject::inject_fields_json(data, fields_json, fonts, fonts_json, compress)
        .map_err(|e| JsError::new(&e))
}

/// Assemble a new PDF from an ordered page selection across source PDFs.
/// `docs_blob` is the concatenated bytes of every source PDF; `docs_json` is a
/// JSON array of `{offset,length}` slicing it into documents; `plan_json` is a
/// JSON array of `{doc,page}` (0-based) giving the ordered output pages.
#[wasm_bindgen]
pub fn manipulate_pages(
    docs_blob: &[u8],
    docs_json: &str,
    plan_json: &str,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, JsError> {
    pageops::manipulate_pages_json(docs_blob, docs_json, plan_json, compress, object_streams)
        .map_err(|e| JsError::new(&e))
}

/// Width in points of `text` in standard-14 `font` at `size`.
/// Incrementally append/insert/remove/move blank pages on a loaded PDF.
#[wasm_bindgen]
pub fn insert_pages(data: &[u8], ops_json: &str, compress: bool) -> Result<Vec<u8>, JsError> {
    pagetree::insert_pages_json(data, ops_json, compress).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen]
pub fn measure_text(font: &str, size: f32, text: &str) -> Result<f32, JsError> {
    appearance::measure_text_width(font, size, text).map_err(|e| JsError::new(&e))
}

/// Width in points of `text` in an embedded font at `size`.
#[wasm_bindgen]
pub fn measure_text_embedded(font: &[u8], size: f32, text: &str) -> Result<f32, JsError> {
    fonts::measure_embedded(font, size, text).map_err(|e| JsError::new(&e))
}

/// Return JSON `{"width":W,"height":H}` (intrinsic pixels) for a JPEG/PNG, or error.
#[wasm_bindgen]
pub fn image_info(data: &[u8]) -> Result<String, JsError> {
    appearance::signature_image(data)
        .map(|img| {
            let i = img.info();
            format!("{{\"width\":{},\"height\":{}}}", i.width, i.height)
        })
        .map_err(|e| JsError::new(&e))
}

/// Read the document Info dictionary as a JSON object.
#[wasm_bindgen]
pub fn read_metadata(data: &[u8]) -> Result<String, JsError> {
    metadata::read_metadata_json(data).map_err(|e| JsError::new(&e))
}

/// Set Info-dictionary metadata; returns new PDF bytes (incremental update).
#[wasm_bindgen]
pub fn set_metadata(data: &[u8], meta_json: &str, compress: bool) -> Result<Vec<u8>, JsError> {
    metadata::set_metadata_json(data, meta_json, compress).map_err(|e| JsError::new(&e))
}

/// Set the document outline (bookmarks) from a JSON array of outline items;
/// returns new PDF bytes (incremental update).
#[wasm_bindgen]
pub fn set_outline(data: &[u8], json: &str, compress: bool) -> Result<Vec<u8>, JsError> {
    outline::set_outline_json(data, json, compress).map_err(|e| JsError::new(&e))
}

/// Decrypt an encrypted PDF with `password` (empty string for the common
/// owner-locked case) and return plaintext bytes. Unencrypted input is returned
/// unchanged. Errors start with `PASSWORD:` (bad/missing password) or
/// `ENCRYPTED:` (unsupported scheme).
#[wasm_bindgen]
pub fn decrypt_pdf(data: &[u8], password: &str) -> Result<Vec<u8>, JsError> {
    doc_io::decrypt_pdf(data, password).map_err(|e| JsError::new(&e))
}

/// Internal re-exports for the fuzz targets in `fuzz/`. Not a public API.
#[doc(hidden)]
pub mod fuzz_api {
    pub use crate::appearance::{parse_da, signature_image};
    pub use crate::create::create_document_json;
    pub use crate::draw::apply_draw_ops_json;
    pub use crate::fill::fill_fields_json;
    pub use crate::forms::read_fields_json;
    pub use crate::metadata::{read_metadata_json, set_metadata_json};
    pub use crate::outline::set_outline_json;
    pub use crate::pageops::manipulate_pages_json;
    pub use crate::pagetree::insert_pages_json;
}
