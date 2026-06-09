use wasm_bindgen::prelude::*;

mod appearance;
mod fill;
mod flatten;
mod forms;

/// Returns the input bytes unchanged. Placeholder operation for Milestone 1;
/// later milestones replace the body with real parse/serialize.
#[wasm_bindgen]
pub fn round_trip(data: &[u8]) -> Vec<u8> {
    data.to_vec()
}

/// Read the AcroForm fields of a PDF, returned as a JSON array string.
#[wasm_bindgen]
pub fn read_fields(data: &[u8]) -> Result<String, JsError> {
    forms::read_fields_json(data).map_err(|e| JsError::new(&e))
}

/// Apply fill ops (JSON array of {name, value}) to a PDF and return new bytes.
#[wasm_bindgen]
pub fn fill_fields(data: &[u8], ops_json: &str) -> Result<Vec<u8>, JsError> {
    fill::fill_fields_json(data, ops_json).map_err(|e| JsError::new(&e))
}

/// Flatten the named fields (JSON array of names) and return new PDF bytes.
#[wasm_bindgen]
pub fn flatten_fields(data: &[u8], names_json: &str) -> Result<Vec<u8>, JsError> {
    flatten::flatten_fields_json(data, names_json).map_err(|e| JsError::new(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_returns_input_unchanged() {
        let input: Vec<u8> = vec![0x25, 0x50, 0x44, 0x46, 0x2d]; // "%PDF-"
        let output = round_trip(&input);
        assert_eq!(output, input);
    }
}
