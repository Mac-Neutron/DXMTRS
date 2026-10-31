use super::*;

#[test]
fn hex4_exactly_four() {
    assert_eq!(hex4(b"10de"), Some(0x10de));
    assert_eq!(hex4(b"BEEF"), Some(0xbeef));
    assert_eq!(hex4(b"0000"), Some(0));
    for bad in [
        &b""[..],
        b"10d",
        b"10dee",
        b"0x10",
        b"12g4",
        b" 123",
        b"\xe9123",
    ] {
        assert_eq!(hex4(bad), None);
    }
}

/// The four options, from their keys; an empty description is none.
#[test]
fn options_read_the_four_keys() {
    let config = Config::from_user(
        None,
        b"dxgi.customVendorId=beef;dxgi.customDeviceId=12345;dxgi.forceSDR=TRUE",
        b"",
    );
    let options = Options::from_config(&config);
    assert_eq!(
        (options.vendor_id, options.device_id, options.force_sdr),
        (Some(0xbeef), None, true)
    );
    assert_eq!(options.device_desc, None);
    let config = Config::from_user(None, b"dxgi.customDeviceDesc=\"a b\"", b"");
    let options = Options::from_config(&config);
    assert_eq!(options.device_desc.as_deref(), Some(&b"a b"[..]));
    assert!(!options.force_sdr);
}
