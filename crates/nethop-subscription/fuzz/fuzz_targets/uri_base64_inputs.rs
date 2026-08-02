#![no_main]

use libfuzzer_sys::fuzz_target;
use nethop_subscription::{ParserLimits, decode_base64_and_detect, parse_uri_list};

fuzz_target!(|data: &[u8]| {
    let limits = ParserLimits::default();
    let _ = parse_uri_list(data, None, &limits);
    let _ = decode_base64_and_detect(data, &limits);
});
