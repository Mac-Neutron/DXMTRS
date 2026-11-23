use core::sync::atomic::{AtomicBool, Ordering};

use dxmt_util::dxgi_consts::{
    DXGI_FORMAT_R8_TYPELESS, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R16_TYPELESS, DXGI_FORMAT_R16_UNORM,
    DXGI_FORMAT_R32_FLOAT,
};

use super::{consume_reset, reactive_format_compatible};

#[test]
fn explicit_reset_short_circuits_fresh_flag() {
    let fresh = AtomicBool::new(true);
    assert_eq!(consume_reset(1, &fresh), 1);
    assert!(fresh.load(Ordering::SeqCst));
    assert_eq!(consume_reset(0, &fresh), 1);
    assert!(!fresh.load(Ordering::SeqCst));
    assert_eq!(consume_reset(0, &fresh), 0);
}

#[test]
fn reactive_format_compatible_handles_typeless_and_concrete() {
    assert!(reactive_format_compatible(
        DXGI_FORMAT_R8_UNORM,
        DXGI_FORMAT_R8_UNORM
    ));
    assert!(reactive_format_compatible(
        DXGI_FORMAT_R8_TYPELESS,
        DXGI_FORMAT_R8_UNORM
    ));
    assert!(reactive_format_compatible(
        DXGI_FORMAT_R16_TYPELESS,
        DXGI_FORMAT_R16_UNORM
    ));
    assert!(!reactive_format_compatible(
        DXGI_FORMAT_R8_TYPELESS,
        DXGI_FORMAT_R32_FLOAT
    ));
}
