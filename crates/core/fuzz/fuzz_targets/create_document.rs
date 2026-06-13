#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let _ = better_pdf_core::fuzz_api::create_document_json(data, &[], "[]");
});
