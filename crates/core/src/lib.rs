use wasm_bindgen::prelude::*;

mod appearance;
pub mod create;
mod draw;
mod fill;
mod flatten;
mod font_metrics;
mod forms;
mod pages;

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
pub fn apply_draw_ops(data: &[u8], ops_json: &str, images: &[u8]) -> Result<Vec<u8>, JsError> {
    draw::apply_draw_ops_json(data, ops_json, images).map_err(|e| JsError::new(&e))
}

/// Build a new PDF document from scratch using a JSON array of create ops
/// (addPage, text, image, etc.) and return the PDF bytes. `images` is the
/// concatenated image blob that Image ops index into via imageOffset / imageLength.
#[wasm_bindgen]
pub fn create_document(ops_json: &str, images: &[u8]) -> Result<Vec<u8>, JsError> {
    create::create_document_json(ops_json, images).map_err(|e| JsError::new(&e))
}

/// Width in points of `text` in standard-14 `font` at `size`.
#[wasm_bindgen]
pub fn measure_text(font: &str, size: f32, text: &str) -> Result<f32, JsError> {
    appearance::measure_text_width(font, size, text).map_err(|e| JsError::new(&e))
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

/// Internal re-exports for the fuzz targets in `fuzz/`. Not a public API.
#[doc(hidden)]
pub mod fuzz_api {
    pub use crate::appearance::{parse_da, signature_image};
    pub use crate::create::create_document_json;
    pub use crate::draw::apply_draw_ops_json;
    pub use crate::fill::fill_fields_json;
    pub use crate::forms::read_fields_json;
}
