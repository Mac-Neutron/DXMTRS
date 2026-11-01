use core::mem::size_of;

use super::{
    IID_ID3D10_DEVICE1, IID_ID3D11_DEVICE, IID_ID3D11_DEVICE_CONTEXT,
    ext::{D3D11Piece, D3D11UpscaleDesc, IID_IMTL_MIN_LOD_CLAMPABLE, IMTLD3D11DeviceContextVtbl},
};

#[test]
fn iids_match_headers() {
    assert_eq!(
        IID_ID3D11_DEVICE.to_string(),
        "db6f6ddb-ac77-4e88-8253-819df9bbf140"
    );
    assert_eq!(
        IID_ID3D11_DEVICE_CONTEXT.to_string(),
        "c0bfa96c-e089-44fb-8eaf-26f8796190da"
    );
    assert_eq!(
        IID_ID3D10_DEVICE1.to_string(),
        "9b7e4c8f-342c-4106-a19f-4f2704f689f0"
    );
    assert_eq!(
        IID_IMTL_MIN_LOD_CLAMPABLE.to_string(),
        "102d11a1-5be7-46c9-a19a-56b036c26f78"
    );
}

#[test]
fn companion_layouts_match_width() {
    assert_eq!(size_of::<D3D11Piece>(), 24);
    assert_eq!(
        size_of::<D3D11UpscaleDesc>(),
        if size_of::<usize>() == 8 { 88 } else { 64 }
    );
    assert_eq!(
        size_of::<IMTLD3D11DeviceContextVtbl>(),
        155 * size_of::<usize>()
    );
}
