#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|s: &str| {
    let _ = better_pdf_core::fuzz_api::parse_da(s);
});
