use nethopd::{parse_default_route_interface, parse_process_stat, parse_statm_rss_bytes};

#[test]
fn proc_stat_parser_handles_spaces_and_parentheses_in_process_name() {
    let stat = "42 (sing box (core)) S 1 2 3 4 5 6 7 8 9 10 120 30 13 14 15 16 17 18 900";
    assert_eq!(parse_process_stat(stat), Some((150, 900)));
}

#[test]
fn statm_parser_uses_resident_pages_and_checks_overflow() {
    assert_eq!(
        parse_statm_rss_bytes("100 25 3 0 0 0 0", 4096),
        Some(102_400)
    );
    assert_eq!(parse_statm_rss_bytes("1 not-a-number", 4096), None);
    assert_eq!(parse_statm_rss_bytes("1 18446744073709551615", 4096), None);
}

#[test]
fn default_route_parser_requires_up_gateway_flags_and_safe_interface() {
    let route = "Iface Destination Gateway Flags RefCnt Use Metric Mask\nrmnet_data0 00000000 01010101 0003 0 0 0 00000000\nwlan0 00000000 00000000 0001 0 0 0 00000000\n";
    assert_eq!(
        parse_default_route_interface(route).as_deref(),
        Some("rmnet_data0")
    );
    assert_eq!(
        parse_default_route_interface(
            "Iface Destination Gateway Flags\nbad/if 00000000 01010101 0003\n"
        ),
        None
    );
}
