#![no_main]

use libfuzzer_sys::fuzz_target;
use nethop_subscription::{CapabilityMatrix, ParserLimits, parse_singbox_json};

fuzz_target!(|data: &[u8]| {
    let _ = parse_singbox_json(
        data,
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
});
