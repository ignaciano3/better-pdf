#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = better_pdf_core::fuzz_api::read_fields_json(data);
});
