#![no_main]

use libfuzzer_sys::fuzz_target;
use nethop_subscription::{FormatHint, ParserLimits, detect_bytes};

fuzz_target!(|data: &[u8]| {
    let _ = detect_bytes(data, FormatHint::Auto, &ParserLimits::default());
});
