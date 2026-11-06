use super::*;

#[test]
fn codes_have_the_sdk_values() {
    assert_eq!(DXGI_ERROR_INVALID_CALL.cast_unsigned(), 0x887a_0001);
    assert_eq!(DXGI_ERROR_UNSUPPORTED.cast_unsigned(), 0x887a_0004);
    assert_eq!(DXGI_ERROR_WAS_STILL_DRAWING.cast_unsigned(), 0x887a_000a);
    assert_eq!(
        DXGI_ERROR_NOT_CURRENTLY_AVAILABLE.cast_unsigned(),
        0x887a_0022
    );
    assert_eq!(DXGI_ERROR_ALREADY_EXISTS.cast_unsigned(), 0x887a_0036);
    assert_eq!(
        D3D12_ERROR_DRIVER_VERSION_MISMATCH.cast_unsigned(),
        0x887e_0002
    );
    assert_eq!(DXGI_ERROR_NOT_FOUND.cast_unsigned(), 0x887a_0002);
    assert_eq!(DXGI_ERROR_MORE_DATA.cast_unsigned(), 0x887a_0003);
    assert_eq!(E_INVALIDARG.cast_unsigned(), 0x8007_0057);
}

#[test]
fn statuses_are_success_codes_and_errors_are_failures() {
    for status in [
        DXGI_STATUS_OCCLUDED,
        DXGI_STATUS_MODE_CHANGE_IN_PROGRESS,
        DXGI_STATUS_PRESENT_REQUIRED,
    ] {
        assert!(status >= 0);
    }
    for error in [
        DXGI_ERROR_INVALID_CALL,
        DXGI_ERROR_ALREADY_EXISTS,
        D3D12_ERROR_DRIVER_VERSION_MISMATCH,
    ] {
        assert!(error < 0);
    }
    assert_eq!(DXGI_STATUS_MODE_CHANGE_IN_PROGRESS, 0x087a_0008);
    assert_eq!(DXGI_STATUS_PRESENT_REQUIRED, 0x087a_0025);
}
