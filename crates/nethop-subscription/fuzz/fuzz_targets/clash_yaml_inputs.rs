#![no_main]

use libfuzzer_sys::fuzz_target;
use nethop_subscription::{CapabilityMatrix, ParserLimits, parse_clash_yaml};

fuzz_target!(|data: &[u8]| {
    let _ = parse_clash_yaml(
        data,
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
});
