#![no_main]

use libfuzzer_sys::fuzz_target;
use nethop_subscription::{CapabilityMatrix, ParserLimits, parse_surfboard_ini};

fuzz_target!(|data: &[u8]| {
    let _ = parse_surfboard_ini(
        data,
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
});
