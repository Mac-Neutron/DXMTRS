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

#[test]
fn buffer_desc_roundtrip() {
    use super::util::{d10_to_d11_buffer_desc, d11_to_d10_buffer_desc};
    use crate::abi::records::D3d10BufferDesc;

    let d10 = D3d10BufferDesc {
        byte_width: 4096,
        usage: 2,
        bind_flags: 1,
        cpu_access_flags: 0x10000,
        misc_flags: D3D10_RESOURCE_MISC_SHARED,
    };
    let d11 = d10_to_d11_buffer_desc(&d10);
    assert_eq!(d11.byte_width, 4096);
    assert_eq!(d11.usage, 2);
    assert_eq!(d11.bind_flags, 1);
    assert_eq!(d11.cpu_access_flags, 0x10000);
    assert_eq!(d11.misc_flags, D3D11_RESOURCE_MISC_SHARED);
    assert_eq!(d11.structure_byte_stride, 0);

    let back = d11_to_d10_buffer_desc(&d11);
    assert_eq!(back.byte_width, d10.byte_width);
    assert_eq!(back.usage, d10.usage);
    assert_eq!(back.bind_flags, d10.bind_flags);
    assert_eq!(back.cpu_access_flags, d10.cpu_access_flags);
    assert_eq!(back.misc_flags, d10.misc_flags);
}

#[test]
fn texture_unwrap_reads_the_buffer_layout() {
    use super::{TEXTURE1D_VTBL, TEXTURE2D_VTBL, TEXTURE3D_VTBL};
    use crate::abi::vtbl::{
        ID3D10BufferVtbl, ID3D10Texture1DVtbl, ID3D10Texture2DVtbl, ID3D10Texture3DVtbl,
    };

    assert_eq!(
        offset_of!(Wrapper<ID3D10Texture1DVtbl>, d3d11),
        offset_of!(Wrapper<ID3D10BufferVtbl>, d3d11)
    );
    assert_eq!(
        offset_of!(Wrapper<ID3D10Texture2DVtbl>, d3d11),
        offset_of!(Wrapper<ID3D10BufferVtbl>, d3d11)
    );
    assert_eq!(
        offset_of!(Wrapper<ID3D10Texture3DVtbl>, d3d11),
        offset_of!(Wrapper<ID3D10BufferVtbl>, d3d11)
    );

    let mut tex1d_inner = 0x1010_u64;
    let mut tex2d_inner = 0x2020_u64;
    let mut tex3d_inner = 0x3030_u64;
    let mut ctx = 0x9999_u64;
    let p1 = (&raw mut tex1d_inner).cast::<c_void>();
    let p2 = (&raw mut tex2d_inner).cast::<c_void>();
    let p3 = (&raw mut tex3d_inner).cast::<c_void>();
    let ctx_ptr = (&raw mut ctx).cast::<c_void>();

    let mut w1 = Wrapper::new(&TEXTURE1D_VTBL, p1, ctx_ptr);
    let mut w2 = Wrapper::new(&TEXTURE2D_VTBL, p2, ctx_ptr);
    let mut w3 = Wrapper::new(&TEXTURE3D_VTBL, p3, ctx_ptr);

    // SAFETY: `w1`, `w2`, `w3` are live `Wrapper<V>` instances.
    let u1 = unsafe { d3d11_of((&raw mut w1).cast::<c_void>()) };
    // SAFETY: `w2` is a live `Wrapper<ID3D10Texture2DVtbl>`.
    let u2 = unsafe { d3d11_of((&raw mut w2).cast::<c_void>()) };
    // SAFETY: `w3` is a live `Wrapper<ID3D10Texture3DVtbl>`.
    let u3 = unsafe { d3d11_of((&raw mut w3).cast::<c_void>()) };

    assert_eq!(u1, p1);
    assert_eq!(u2, p2);
    assert_eq!(u3, p3);
}

#[test]
fn dsv_desc_roundtrip() {
    use super::util::{d10_to_d11_dsv_desc, d11_to_d10_dsv_desc};
    use crate::abi::records::{
        D3d10DepthStencilViewDesc, D3d10DepthStencilViewDescUnion, D3d10Tex2dArrayDsv,
    };

    let d10 = D3d10DepthStencilViewDesc {
        format: 45,
        view_dimension: 4, // D3D10_DSV_DIMENSION_TEXTURE2DARRAY
        u: D3d10DepthStencilViewDescUnion {
            texture2d_array: D3d10Tex2dArrayDsv {
                mip_slice: 1,
                first_array_slice: 2,
                array_size: 4,
            },
        },
    };
    let d11 = d10_to_d11_dsv_desc(&d10);
    assert_eq!(d11.format, 45);
    assert_eq!(d11.view_dimension, 4);
    assert_eq!(d11.flags, 0);

    let back = d11_to_d10_dsv_desc(&d11);
    assert_eq!(back.format, 45);
    assert_eq!(back.view_dimension, 4);
    // SAFETY: `view_dimension == 4` selects the `texture2d_array` union arm.
    let arr = unsafe { back.u.texture2d_array };
    assert_eq!(arr.mip_slice, 1);
    assert_eq!(arr.first_array_slice, 2);
    assert_eq!(arr.array_size, 4);
}
