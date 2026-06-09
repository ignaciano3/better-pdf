use wasm_bindgen::prelude::*;

/// Returns the input bytes unchanged. Placeholder operation for Milestone 1;
/// later milestones replace the body with real parse/serialize.
#[wasm_bindgen]
pub fn round_trip(data: &[u8]) -> Vec<u8> {
    data.to_vec()
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
