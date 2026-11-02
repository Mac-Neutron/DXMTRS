//! Unit tests for the D3D10 object layer (`src/d3d11/src/d3d10`).

use core::{
    ffi::c_void,
    mem::{offset_of, size_of},
    ptr,
};

use super::{
    BUFFER_VTBL, Wrapper, d3d11_of,
    util::{
        D3D10_RESOURCE_MISC_GDI_COMPATIBLE, D3D10_RESOURCE_MISC_GENERATE_MIPS,
        D3D10_RESOURCE_MISC_SHARED, D3D10_RESOURCE_MISC_SHARED_KEYEDMUTEX,
        D3D10_RESOURCE_MISC_TEXTURECUBE, D3D11_RESOURCE_MISC_GDI_COMPATIBLE,
        D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_RESOURCE_MISC_SHARED,
        D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX, D3D11_RESOURCE_MISC_TEXTURECUBE,
        convert_d3d10_resource_flags, convert_d3d11_resource_flags,
    },
};

#[test]
fn flags_convert_both_ways() {
    let pairs = [
        (
            D3D10_RESOURCE_MISC_GENERATE_MIPS,
            D3D11_RESOURCE_MISC_GENERATE_MIPS,
        ),
        (D3D10_RESOURCE_MISC_SHARED, D3D11_RESOURCE_MISC_SHARED),
        (
            D3D10_RESOURCE_MISC_TEXTURECUBE,
            D3D11_RESOURCE_MISC_TEXTURECUBE,
        ),
        (
            D3D10_RESOURCE_MISC_SHARED_KEYEDMUTEX,
            D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX,
        ),
        (
            D3D10_RESOURCE_MISC_GDI_COMPATIBLE,
            D3D11_RESOURCE_MISC_GDI_COMPATIBLE,
        ),
    ];

    assert_eq!(convert_d3d10_resource_flags(0), 0);
    assert_eq!(convert_d3d11_resource_flags(0), 0);

    let mut all_d10 = 0;
    let mut all_d11 = 0;
    for (d10, d11) in pairs {
        assert_eq!(convert_d3d10_resource_flags(d10), d11);
        assert_eq!(convert_d3d11_resource_flags(d11), d10);
        all_d10 |= d10;
        all_d11 |= d11;
    }
    assert_eq!(convert_d3d10_resource_flags(all_d10), all_d11);
    assert_eq!(convert_d3d11_resource_flags(all_d11), all_d10);

    // D3D11-only misc flags (e.g. DRAWINDIRECT_ARGS = 0x10, BUFFER_ALLOW_RAW_VIEWS = 0x20)
    // must be stripped when converting back to D3D10.
    assert_eq!(convert_d3d11_resource_flags(0x10 | 0x20 | 0x40), 0);
}

#[test]
fn wrapper_layout_is_vtbl_then_d3d11() {
    assert_eq!(offset_of!(Wrapper<u8>, vtbl), 0);
    assert_eq!(offset_of!(Wrapper<u8>, d3d11), size_of::<*const u8>());
    assert_eq!(offset_of!(Wrapper<u8>, context), 2 * size_of::<*const u8>());
    assert_eq!(size_of::<Wrapper<u8>>(), 3 * size_of::<*const u8>());

    let mut fake_d3d11 = 0x1111_u64;
    let mut fake_ctx = 0x2222_u64;
    let d3d11_ptr = (&raw mut fake_d3d11).cast::<c_void>();
    let ctx_ptr = (&raw mut fake_ctx).cast::<c_void>();

    let mut wrapper = Wrapper::new(&BUFFER_VTBL, d3d11_ptr, ctx_ptr);
    let wrapper_ptr = (&raw mut wrapper).cast::<c_void>();

    // SAFETY: `wrapper_ptr` points to a live `Wrapper<ID3D10BufferVtbl>`.
    let unwrapped = unsafe { d3d11_of(wrapper_ptr) };
    assert_eq!(unwrapped, d3d11_ptr);

    // SAFETY: null input is allowed and returns null.
    let null_unwrapped = unsafe { d3d11_of(ptr::null_mut()) };
    assert!(null_unwrapped.is_null());
}
