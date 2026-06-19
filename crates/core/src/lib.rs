use wasm_bindgen::prelude::*;

mod appearance;
pub mod create;
mod fonts;
mod draw;
mod embed;
mod fill;
mod flatten;
mod font_metrics;
mod forms;
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
pub fn fill_fields(data: &[u8], ops_json: &str, images: &[u8]) -> Result<Vec<u8>, JsError> {
    fill::fill_fields_json(data, ops_json, images).map_err(|e| JsError::new(&e))
}

/// Flatten the named fields (JSON array of names) and return new PDF bytes.
#[wasm_bindgen]
pub fn flatten_fields(data: &[u8], names_json: &str) -> Result<Vec<u8>, JsError> {
    flatten::flatten_fields_json(data, names_json).map_err(|e| JsError::new(&e))
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
) -> Result<Vec<u8>, JsError> {
    draw::apply_draw_ops_json(data, ops_json, images, fonts, fonts_json).map_err(|e| JsError::new(&e))
}

/// Build a new PDF document from scratch using a JSON array of create ops
/// (addPage, text, image, etc.) and return the PDF bytes. `images` is the
/// concatenated image blob that Image ops index into via imageOffset / imageLength.
/// `fonts` is the concatenated font blob that embedded-font text ops index into
/// via `fonts_json` descriptors (pass `&[]` / "[]" for none).
/// `fields_json` is a JSON array of field definitions (pass "[]" for none).
#[wasm_bindgen]
pub fn create_document(
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    fields_json: &str,
) -> Result<Vec<u8>, JsError> {
    create::create_document_json(ops_json, images, fonts, fonts_json, fields_json)
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
) -> Result<Vec<u8>, JsError> {
    pageops::manipulate_pages_json(docs_blob, docs_json, plan_json).map_err(|e| JsError::new(&e))
}

/// Width in points of `text` in standard-14 `font` at `size`.
/// Incrementally append/insert/remove/move blank pages on a loaded PDF.
#[wasm_bindgen]
pub fn insert_pages(data: &[u8], ops_json: &str) -> Result<Vec<u8>, JsError> {
    pagetree::insert_pages_json(data, ops_json).map_err(|e| JsError::new(&e))
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
pub fn set_metadata(data: &[u8], meta_json: &str) -> Result<Vec<u8>, JsError> {
    metadata::set_metadata_json(data, meta_json).map_err(|e| JsError::new(&e))
}

/// Set the document outline (bookmarks) from a JSON array of outline items;
/// returns new PDF bytes (incremental update).
#[wasm_bindgen]
pub fn set_outline(data: &[u8], json: &str) -> Result<Vec<u8>, JsError> {
    outline::set_outline_json(data, json).map_err(|e| JsError::new(&e))
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
