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

#[test]
fn blend_desc_roundtrip() {
    use super::util::{d10_to_d11_blend_desc, d11_to_d10_blend_desc};
    use crate::abi::records::D3d10BlendDesc;

    let d10 = D3d10BlendDesc {
        alpha_to_coverage_enable: 1,
        blend_enable: [1, 0, 1, 0, 0, 1, 0, 1],
        src_blend: 5,
        dest_blend: 6,
        blend_op: 1,
        src_blend_alpha: 2,
        dest_blend_alpha: 1,
        blend_op_alpha: 1,
        render_target_write_mask: [0xf, 0x7, 0x3, 0x1, 0xe, 0xc, 0x8, 0x0],
    };
    let d11 = d10_to_d11_blend_desc(&d10);
    assert_eq!(d11.alpha_to_coverage_enable, 1);
    assert_eq!(d11.independent_blend_enable, 1);
    for i in 0..8 {
        assert_eq!(d11.render_target[i].blend_enable, d10.blend_enable[i]);
        assert_eq!(
            d11.render_target[i].render_target_write_mask,
            d10.render_target_write_mask[i]
        );
    }

    let back = d11_to_d10_blend_desc(&d11);
    assert_eq!(back.alpha_to_coverage_enable, d10.alpha_to_coverage_enable);
    assert_eq!(back.blend_enable, d10.blend_enable);
    assert_eq!(back.src_blend, d10.src_blend);
    assert_eq!(back.dest_blend, d10.dest_blend);
    assert_eq!(back.blend_op, d10.blend_op);
    assert_eq!(back.src_blend_alpha, d10.src_blend_alpha);
    assert_eq!(back.dest_blend_alpha, d10.dest_blend_alpha);
    assert_eq!(back.blend_op_alpha, d10.blend_op_alpha);
    assert_eq!(back.render_target_write_mask, d10.render_target_write_mask);
}

#[test]
fn shader_wrappers_unwrap_d3d11() {
    use super::{GEOMETRY_SHADER_VTBL, PIXEL_SHADER_VTBL, VERTEX_SHADER_VTBL};

    let mut vertex_raw = 0x11_u64;
    let mut pixel_raw = 0x22_u64;
    let mut geom_raw = 0x33_u64;
    let vertex_ptr = (&raw mut vertex_raw).cast::<c_void>();
    let pixel_ptr = (&raw mut pixel_raw).cast::<c_void>();
    let geom_ptr = (&raw mut geom_raw).cast::<c_void>();

    let mut vertex_wrap = Wrapper::new(&VERTEX_SHADER_VTBL, vertex_ptr, ptr::null_mut());
    let mut pixel_wrap = Wrapper::new(&PIXEL_SHADER_VTBL, pixel_ptr, ptr::null_mut());
    let mut geom_wrap = Wrapper::new(&GEOMETRY_SHADER_VTBL, geom_ptr, ptr::null_mut());

    // SAFETY: `vertex_wrap` is a live `Wrapper<ID3D10VertexShaderVtbl>`.
    let vertex_out = unsafe { d3d11_of((&raw mut vertex_wrap).cast::<c_void>()) };
    // SAFETY: `pixel_wrap` is a live `Wrapper<ID3D10PixelShaderVtbl>`.
    let pixel_out = unsafe { d3d11_of((&raw mut pixel_wrap).cast::<c_void>()) };
    // SAFETY: `geom_wrap` is a live `Wrapper<ID3D10GeometryShaderVtbl>`.
    let geom_out = unsafe { d3d11_of((&raw mut geom_wrap).cast::<c_void>()) };

    assert_eq!(vertex_out, vertex_ptr);
    assert_eq!(pixel_out, pixel_ptr);
    assert_eq!(geom_out, geom_ptr);
}

#[test]
fn input_layout_wrapper_unwraps_d3d11() {
    use super::INPUT_LAYOUT_VTBL;

    let mut raw_il = 0x44_u64;
    let il_ptr = (&raw mut raw_il).cast::<c_void>();
    let mut wrap = Wrapper::new(&INPUT_LAYOUT_VTBL, il_ptr, ptr::null_mut());
    // SAFETY: `wrap` is a live `Wrapper<ID3D10InputLayoutVtbl>`.
    let out = unsafe { d3d11_of((&raw mut wrap).cast::<c_void>()) };
    assert_eq!(out, il_ptr);
}

#[test]
fn query_desc_roundtrip() {
    use super::util::{d10_to_d11_query_desc, d11_to_d10_query_desc};
    use crate::abi::records::D3d10QueryDesc;

    let d10 = D3d10QueryDesc {
        query: 2, // D3D10_QUERY_TIMESTAMP
        misc_flags: 1,
    };
    let d11 = d10_to_d11_query_desc(&d10);
    assert_eq!(d11.query, 2);
    assert_eq!(d11.misc_flags, 1);

    let back = d11_to_d10_query_desc(&d11);
    assert_eq!(back.query, 2);
    assert_eq!(back.misc_flags, 1);
}

#[test]
fn descs_as_the_probe() {
    use super::util::{
        d10_to_d11_buffer_desc, d10_to_d11_query_desc, d10_to_d11_sampler_desc,
        d10_to_d11_tex1d_desc, d10_to_d11_tex2d_desc, d10_to_d11_tex3d_desc, d11_to_d10_blend_desc,
        d11_to_d10_buffer_desc, d11_to_d10_query_desc, d11_to_d10_rtv_desc,
        d11_to_d10_sampler_desc, d11_to_d10_srv_desc, d11_to_d10_tex1d_desc, d11_to_d10_tex2d_desc,
        d11_to_d10_tex3d_desc,
    };
    use crate::abi::records::{
        D3d10BufferDesc, D3d10DepthStencilDesc, D3d10DepthStencilopDesc, D3d10QueryDesc,
        D3d10RasterizerDesc, D3d10SamplerDesc, D3d10Texture1dDesc, D3d10Texture2dDesc,
        D3d10Texture3dDesc, D3d11BlendDesc, D3d11DepthStencilDesc, D3d11RasterizerDesc,
        D3d11RenderTargetBlendDesc, D3d11RenderTargetViewDesc, D3d11RenderTargetViewDescUnion,
        D3d11ShaderResourceViewDesc, D3d11ShaderResourceViewDescUnion, D3d11Tex2dRtv,
        D3d11Tex2dSrv, DxgiSampleDesc,
    };

    let golden = include_str!("golden/d10.txt");
    let expected: Vec<&str> = golden
        .lines()
        .filter(|line| line.starts_with("d10 desc_"))
        .collect();
    assert_eq!(expected.len(), 11);

    // 1. desc_buf
    let buf_in = D3d10BufferDesc {
        byte_width: 64,
        usage: 2,
        bind_flags: 9,
        cpu_access_flags: 0x10000,
        misc_flags: 1,
    };
    let buf_d11 = d10_to_d11_buffer_desc(&buf_in);
    let buf_d10 = d11_to_d10_buffer_desc(&buf_d11);
    let line_buffer = format!(
        "d10 desc_buf hr=00000000 d10={},{},{:x},{:x},{:x} d11={},{},{:x},{:x},{:x},{}",
        buf_d10.byte_width,
        buf_d10.usage,
        buf_d10.bind_flags,
        buf_d10.cpu_access_flags,
        buf_d10.misc_flags,
        buf_d11.byte_width,
        buf_d11.usage,
        buf_d11.bind_flags,
        buf_d11.cpu_access_flags,
        buf_d11.misc_flags,
        buf_d11.structure_byte_stride,
    );

    // 2. desc_tex1d
    let t1d = D3d10Texture1dDesc {
        width: 32,
        mip_levels: 0,
        array_size: 2,
        format: 28,
        usage: 0,
        bind_flags: 8,
        cpu_access_flags: 0,
        misc_flags: 0,
    };
    let mut got_t1d11 = d10_to_d11_tex1d_desc(&t1d);
    if got_t1d11.mip_levels == 0 {
        got_t1d11.mip_levels = 32 - got_t1d11.width.leading_zeros();
    }
    let got_t1d10 = d11_to_d10_tex1d_desc(&got_t1d11);
    let line_tex1d = format!(
        "d10 desc_tex1d hr=00000000 d10={},{},{},{},{},{:x} d11={},{},{},{},{},{:x}",
        got_t1d10.width,
        got_t1d10.mip_levels,
        got_t1d10.array_size,
        got_t1d10.format,
        got_t1d10.usage,
        got_t1d10.bind_flags,
        got_t1d11.width,
        got_t1d11.mip_levels,
        got_t1d11.array_size,
        got_t1d11.format,
        got_t1d11.usage,
        got_t1d11.bind_flags,
    );

    // 3. desc_tex2d
    let td = D3d10Texture2dDesc {
        width: 16,
        height: 16,
        mip_levels: 1,
        array_size: 1,
        format: 28,
        sample_desc: DxgiSampleDesc {
            count: 1,
            quality: 0,
        },
        usage: 0,
        bind_flags: 0x28,
        cpu_access_flags: 0,
        misc_flags: 0,
    };
    let got_t2d11 = d10_to_d11_tex2d_desc(&td);
    let got_t2d10 = d11_to_d10_tex2d_desc(&got_t2d11);
    let line_tex2d = format!(
        "d10 desc_tex2d hr=00000000 d10={},{},{},{},{},{},{:x} d11={},{},{},{},{},{},{:x}",
        got_t2d10.width,
        got_t2d10.height,
        got_t2d10.mip_levels,
        got_t2d10.array_size,
        got_t2d10.format,
        got_t2d10.usage,
        got_t2d10.bind_flags,
        got_t2d11.width,
        got_t2d11.height,
        got_t2d11.mip_levels,
        got_t2d11.array_size,
        got_t2d11.format,
        got_t2d11.usage,
        got_t2d11.bind_flags,
    );

    // 4. desc_tex3d
    let t3d = D3d10Texture3dDesc {
        width: 8,
        height: 8,
        depth: 4,
        mip_levels: 1,
        format: 28,
        usage: 0,
        bind_flags: 8,
        cpu_access_flags: 0,
        misc_flags: 0,
    };
    let got_t3d11 = d10_to_d11_tex3d_desc(&t3d);
    let got_t3d10 = d11_to_d10_tex3d_desc(&got_t3d11);
    let line_tex3d = format!(
        "d10 desc_tex3d hr=00000000 d10={},{},{},{},{},{},{:x} d11={},{},{},{},{},{},{:x}",
        got_t3d10.width,
        got_t3d10.height,
        got_t3d10.depth,
        got_t3d10.mip_levels,
        got_t3d10.format,
        got_t3d10.usage,
        got_t3d10.bind_flags,
        got_t3d11.width,
        got_t3d11.height,
        got_t3d11.depth,
        got_t3d11.mip_levels,
        got_t3d11.format,
        got_t3d11.usage,
        got_t3d11.bind_flags,
    );

    // 5. desc_srv
    let got_srv11 = D3d11ShaderResourceViewDesc {
        format: got_t2d11.format,
        view_dimension: 4,
        u: D3d11ShaderResourceViewDescUnion {
            texture2d: D3d11Tex2dSrv {
                most_detailed_mip: 0,
                mip_levels: got_t2d11.mip_levels,
            },
        },
    };
    let got_srv10 = d11_to_d10_srv_desc(&got_srv11);
    // SAFETY: `view_dimension == 4` selects `texture2d`.
    let srv10_t2d = unsafe { got_srv10.u.texture2d };
    // SAFETY: `view_dimension == 4` selects `texture2d`.
    let srv11_t2d = unsafe { got_srv11.u.texture2d };
    let line_srv = format!(
        "d10 desc_srv hr=00000000 d10={},{},{},{} d11={},{},{},{}",
        got_srv10.format,
        got_srv10.view_dimension,
        srv10_t2d.most_detailed_mip,
        srv10_t2d.mip_levels,
        got_srv11.format,
        got_srv11.view_dimension,
        srv11_t2d.most_detailed_mip,
        srv11_t2d.mip_levels,
    );

    // 6. desc_rtv
    let got_rtv11 = D3d11RenderTargetViewDesc {
        format: got_t2d11.format,
        view_dimension: 4,
        u: D3d11RenderTargetViewDescUnion {
            texture2d: D3d11Tex2dRtv { mip_slice: 0 },
        },
    };
    let got_rtv10 = d11_to_d10_rtv_desc(&got_rtv11);
    // SAFETY: `view_dimension == 4` selects `texture2d`.
    let rtv10_t2d = unsafe { got_rtv10.u.texture2d };
    // SAFETY: `view_dimension == 4` selects `texture2d`.
    let rtv11_t2d = unsafe { got_rtv11.u.texture2d };
    let line_rtv = format!(
        "d10 desc_rtv hr=00000000 d10={},{},{} d11={},{},{}",
        got_rtv10.format,
        got_rtv10.view_dimension,
        rtv10_t2d.mip_slice,
        got_rtv11.format,
        got_rtv11.view_dimension,
        rtv11_t2d.mip_slice,
    );

    // 7. desc_bs
    let mut rt = [D3d11RenderTargetBlendDesc {
        blend_enable: 0,
        src_blend: 0,
        dest_blend: 0,
        blend_op: 0,
        src_blend_alpha: 0,
        dest_blend_alpha: 0,
        blend_op_alpha: 0,
        render_target_write_mask: 0,
    }; 8];
    rt[0] = D3d11RenderTargetBlendDesc {
        blend_enable: 1,
        src_blend: 5,
        dest_blend: 6,
        blend_op: 1,
        src_blend_alpha: 2,
        dest_blend_alpha: 1,
        blend_op_alpha: 1,
        render_target_write_mask: 0xf,
    };
    let blend_d11_1 = D3d11BlendDesc {
        alpha_to_coverage_enable: 1,
        independent_blend_enable: 0,
        render_target: rt,
    };
    let blend_d10_1 = blend_d11_1;
    let blend_d10 = d11_to_d10_blend_desc(&blend_d11_1);
    let line_blend = format!(
        "d10 desc_bs hr=00000000 d10={},{},{},{},{},{:x} d10_1={},{},{},{},{:x} d11_1={},{},{},{},{:x}",
        blend_d10.alpha_to_coverage_enable,
        blend_d10.blend_enable[0],
        blend_d10.src_blend,
        blend_d10.dest_blend,
        blend_d10.blend_op,
        blend_d10.render_target_write_mask[0],
        blend_d10_1.alpha_to_coverage_enable,
        blend_d10_1.render_target[0].blend_enable,
        blend_d10_1.render_target[0].src_blend,
        blend_d10_1.render_target[0].dest_blend,
        blend_d10_1.render_target[0].render_target_write_mask,
        blend_d11_1.alpha_to_coverage_enable,
        blend_d11_1.render_target[0].blend_enable,
        blend_d11_1.render_target[0].src_blend,
        blend_d11_1.render_target[0].dest_blend,
        blend_d11_1.render_target[0].render_target_write_mask,
    );

    // 8. desc_ds
    let face = D3d10DepthStencilopDesc {
        stencil_fail_op: 1,
        stencil_depth_fail_op: 1,
        stencil_pass_op: 1,
        stencil_func: 8,
    };
    let depth_in = D3d10DepthStencilDesc {
        depth_enable: 1,
        depth_write_mask: 1,
        depth_func: 4,
        stencil_enable: 0,
        stencil_read_mask: 0xff,
        stencil_write_mask: 0xff,
        front_face: face,
        back_face: face,
    };
    // SAFETY: `D3D10_DEPTH_STENCIL_DESC` and `D3D11_DEPTH_STENCIL_DESC` have identical layout.
    let depth_d11: D3d11DepthStencilDesc = unsafe { *(&raw const depth_in).cast() };
    // SAFETY: `D3D11_DEPTH_STENCIL_DESC` and `D3D10_DEPTH_STENCIL_DESC` have identical layout.
    let depth_d10: D3d10DepthStencilDesc = unsafe { *(&raw const depth_d11).cast() };
    let line_depth = format!(
        "d10 desc_ds hr=00000000 d10={},{},{} d11={},{},{}",
        depth_d10.depth_enable,
        depth_d10.depth_write_mask,
        depth_d10.depth_func,
        depth_d11.depth_enable,
        depth_d11.depth_write_mask,
        depth_d11.depth_func,
    );

    // 9. desc_rs
    let rast_in = D3d10RasterizerDesc {
        fill_mode: 3,
        cull_mode: 3,
        front_counter_clockwise: 0,
        depth_bias: 0,
        depth_bias_clamp: 0.0,
        slope_scaled_depth_bias: 0.0,
        depth_clip_enable: 1,
        scissor_enable: 0,
        multisample_enable: 0,
        antialiased_line_enable: 0,
    };
    // SAFETY: `D3D10_RASTERIZER_DESC` and `D3D11_RASTERIZER_DESC` have identical layout.
    let rast_d11: D3d11RasterizerDesc = unsafe { *(&raw const rast_in).cast() };
    // SAFETY: `D3D11_RASTERIZER_DESC` and `D3D10_RASTERIZER_DESC` have identical layout.
    let rast_d10: D3d10RasterizerDesc = unsafe { *(&raw const rast_d11).cast() };
    let line_rast = format!(
        "d10 desc_rs hr=00000000 d10={},{},{} d11={},{},{}",
        rast_d10.fill_mode,
        rast_d10.cull_mode,
        rast_d10.depth_clip_enable,
        rast_d11.fill_mode,
        rast_d11.cull_mode,
        rast_d11.depth_clip_enable,
    );

    // 10. desc_ss
    let sampler_in = D3d10SamplerDesc {
        filter: 0x15,
        address_u: 1,
        address_v: 3,
        address_w: 2,
        mip_lod_bias: 0.0,
        max_anisotropy: 4,
        comparison_func: 1,
        border_color: [0.0; 4],
        min_lod: 0.0,
        max_lod: f32::MAX,
    };
    let sampler_d11 = d10_to_d11_sampler_desc(&sampler_in);
    let sampler_d10 = d11_to_d10_sampler_desc(&sampler_d11);
    let line_sampler = format!(
        "d10 desc_ss hr=00000000 d10={:x},{},{},{},{} d11={:x},{},{},{},{}",
        sampler_d10.filter,
        sampler_d10.address_u,
        sampler_d10.address_v,
        sampler_d10.address_w,
        sampler_d10.max_anisotropy,
        sampler_d11.filter,
        sampler_d11.address_u,
        sampler_d11.address_v,
        sampler_d11.address_w,
        sampler_d11.max_anisotropy,
    );

    // 11. desc_q
    let query_in = D3d10QueryDesc {
        query: 0,
        misc_flags: 0,
    };
    let query_d11 = d10_to_d11_query_desc(&query_in);
    let query_d10 = d11_to_d10_query_desc(&query_d11);
    let line_query = format!(
        "d10 desc_q hr=00000000 d10={},{:x} d11={},{:x}",
        query_d10.query, query_d10.misc_flags, query_d11.query, query_d11.misc_flags,
    );

    let actual = [
        line_buffer.as_str(),
        line_tex1d.as_str(),
        line_tex2d.as_str(),
        line_tex3d.as_str(),
        line_srv.as_str(),
        line_rtv.as_str(),
        line_blend.as_str(),
        line_depth.as_str(),
        line_rast.as_str(),
        line_sampler.as_str(),
        line_query.as_str(),
    ];
    assert_eq!(actual.as_slice(), expected.as_slice());
}

#[test]
fn device_layout_and_local_methods() {
    use dxmt_util::com::E_NOTIMPL;

    use super::{DEVICE_VTBL, Device};
    use crate::abi::records::D3d10CounterInfo;

    assert_eq!(offset_of!(Device, vtbl), 0);
    assert_eq!(offset_of!(Device, d3d11), size_of::<*const u8>());
    assert_eq!(offset_of!(Device, context), 2 * size_of::<*const u8>());

    let mut fake_dev = 0x1234_u64;
    let mut fake_ctx = 0x5678_u64;
    let mut dev = Device::new(
        (&raw mut fake_dev).cast::<c_void>(),
        (&raw mut fake_ctx).cast::<c_void>(),
    );
    let dev_ptr = (&raw mut *dev).cast::<c_void>();

    let mut info = D3d10CounterInfo::default();
    // SAFETY: `dev_ptr` is a live `Device` and `info` is a local `D3d10CounterInfo`.
    unsafe { (DEVICE_VTBL.check_counter_info)(dev_ptr, (&raw mut info).cast()) };
    assert_eq!(info.last_device_dependent_counter, 0x4000_0000);
    assert_eq!(info.num_simultaneous_counters, 0);
    assert_eq!(info.num_detectable_parallel_units, 0);

    let mut counter_out: *mut c_void = ptr::dangling_mut();
    // SAFETY: `dev_ptr` is a live `Device` and `counter_out` is a writable pointer slot.
    let hr = unsafe { (DEVICE_VTBL.create_counter)(dev_ptr, ptr::null(), &raw mut counter_out) };
    assert_eq!(hr, E_NOTIMPL);
    assert!(counter_out.is_null());

    let mut w = 99_u32;
    let mut h = 99_u32;
    // SAFETY: `dev_ptr` is a live `Device` and `w`, `h` are writable `u32` slots.
    unsafe { (DEVICE_VTBL.get_text_filter_size)(dev_ptr, &raw mut w, &raw mut h) };
    assert_eq!(w, 0);
    assert_eq!(h, 0);
}
