#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (&[u8], &str)| {
    let (data, ops) = input;
    let _ = better_pdf_core::fuzz_api::fill_fields_json(data, ops, &[], false);
});
