use dxmt_core::resource::TextureViewDescriptor;
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, WMT_PIXEL_FORMAT_R32_FLOAT,
    WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB, WMT_TEXTURE_TYPE_CUBE,
    WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE3D, WMTGPU_FAMILY_APPLE8, WMTTextureInfo,
};

use super::helper;
use crate::{
    abi::records::{
        D3d11BufferDesc, D3d11DepthStencilViewDesc, D3d11DepthStencilViewDescUnion,
        D3d11RenderTargetViewDesc1, D3d11RenderTargetViewDesc1Union, D3d11ShaderResourceViewDesc1,
        D3d11ShaderResourceViewDesc1Union, D3d11Tex2dArrayDsv, D3d11Tex2dArrayRtv1,
        D3d11Tex2dArraySrv1, D3d11Tex2dArrayUav1, D3d11Tex2dSrv1, D3d11Texture1dDesc,
        D3d11Texture2dDesc1, D3d11Texture3dDesc1, D3d11UnorderedAccessViewDesc1,
        D3d11UnorderedAccessViewDesc1Union, DxgiSampleDesc,
    },
    resource::helper as res_helper,
    subresource::ResourceSubset,
};

const DXGI_FORMAT_UNKNOWN: u32 = 0;
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_R8G8B8A8_UNORM_SRGB: u32 = 29;
const DXGI_FORMAT_D32_FLOAT: u32 = 40;
const DXGI_FORMAT_R32_FLOAT: u32 = 41;
const DXGI_FORMAT_D24_UNORM_S8_UINT: u32 = 45;

const D3D11_BIND_SHADER_RESOURCE: u32 = 0x8;
const D3D11_BIND_RENDER_TARGET: u32 = 0x20;
const D3D11_BIND_DEPTH_STENCIL: u32 = 0x40;
const D3D11_BIND_UNORDERED_ACCESS: u32 = 0x80;
const D3D11_RESOURCE_MISC_BUFFER_STRUCTURED: u32 = 0x40;

const D3D11_SRV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
const D3D11_RTV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
const D3D11_UAV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
const D3D11_DSV_DIMENSION_TEXTURE2DARRAY: u32 = 4;

const D3D11_DSV_READ_ONLY_DEPTH: u32 = 0x1;
const D3D11_DSV_READ_ONLY_STENCIL: u32 = 0x2;

const fn zero_tex_info() -> WMTTextureInfo {
    WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: 0,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    }
}

#[test]
fn subset_of_every_view_kind() {
    // 1. Buffer view subset: byte range overlap.
    let buf_desc = D3d11BufferDesc {
        byte_width: 256,
        usage: 0,
        bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_UNORDERED_ACCESS,
        cpu_access_flags: 0,
        misc_flags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED,
        structure_byte_stride: 16,
    };
    let (off_a, size_a) = res_helper::calculate_buffer_view_offset_and_size(&buf_desc, 16, 0, 4);
    let (off_b, size_b) = res_helper::calculate_buffer_view_offset_and_size(&buf_desc, 16, 4, 4);
    let (off_c, size_c) = res_helper::calculate_buffer_view_offset_and_size(&buf_desc, 16, 2, 4);
    let sub_a = ResourceSubset::buffer(off_a, size_a);
    let sub_b = ResourceSubset::buffer(off_b, size_b);
    let sub_c = ResourceSubset::buffer(off_c, size_c);
    assert!(!sub_a.overlaps(sub_b));
    assert!(sub_a.overlaps(sub_c));
    assert!(sub_b.overlaps(sub_c));

    // 2. Texture 2D array SRV, UAV, RTV subsets on a 4-mip, 6-slice texture.
    let t2d = D3d11Texture2dDesc1 {
        width: 128,
        height: 128,
        mip_levels: 4,
        array_size: 6,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_desc: DxgiSampleDesc {
            count: 1,
            quality: 0,
        },
        usage: 0,
        bind_flags: D3D11_BIND_SHADER_RESOURCE
            | D3D11_BIND_RENDER_TARGET
            | D3D11_BIND_UNORDERED_ACCESS,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: 0,
    };
    let mut t2d_out = t2d;
    let mut info = zero_tex_info();
    let _ = res_helper::create_mtl_texture_descriptor_2d(
        &t2d,
        &mut t2d_out,
        &mut info,
        |f| f <= WMTGPU_FAMILY_APPLE8,
        true,
    );

    // SRV covering mips 0..2, slices 0..2
    let mut srv_desc = D3d11ShaderResourceViewDesc1 {
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        view_dimension: D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
        u: D3d11ShaderResourceViewDesc1Union {
            texture2d_array: D3d11Tex2dArraySrv1 {
                most_detailed_mip: 0,
                mip_levels: 2,
                first_array_slice: 0,
                array_size: 2,
                plane_slice: 0,
            },
        },
    };
    let srv_mtl = helper::initialize_and_normalize_srv_descriptor(
        t2d_out.mip_levels,
        t2d_out.array_size,
        &info,
        &mut srv_desc,
    )
    .expect("srv normalize");
    let srv_sub =
        ResourceSubset::from_texture_view(&srv_mtl, t2d_out.mip_levels, t2d_out.array_size, 0);

    // UAV at mip 1, slice 1 (overlaps SRV) and UAV at mip 2, slice 0 (disjoint from SRV)
    let mut uav_hit = D3d11UnorderedAccessViewDesc1 {
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        view_dimension: D3D11_UAV_DIMENSION_TEXTURE2DARRAY,
        u: D3d11UnorderedAccessViewDesc1Union {
            texture2d_array: D3d11Tex2dArrayUav1 {
                mip_slice: 1,
                first_array_slice: 1,
                array_size: 1,
                plane_slice: 0,
            },
        },
    };
    let uav_hit_mtl = helper::initialize_and_normalize_uav_descriptor(
        t2d_out.mip_levels,
        t2d_out.array_size,
        &info,
        &mut uav_hit,
    )
    .expect("uav_hit normalize");
    let uav_hit_sub =
        ResourceSubset::from_texture_view(&uav_hit_mtl, t2d_out.mip_levels, t2d_out.array_size, 0);
    assert!(srv_sub.overlaps(uav_hit_sub));

    // RTV at mip 2, slice 0..2 (disjoint from SRV mips 0..2)
    let mut rtv_miss = D3d11RenderTargetViewDesc1 {
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        view_dimension: D3D11_RTV_DIMENSION_TEXTURE2DARRAY,
        u: D3d11RenderTargetViewDesc1Union {
            texture2d_array: D3d11Tex2dArrayRtv1 {
                mip_slice: 2,
                first_array_slice: 0,
                array_size: 2,
                plane_slice: 0,
            },
        },
    };
    let (_, rtv_miss_mtl) = helper::initialize_and_normalize_rtv_descriptor(
        t2d_out.mip_levels,
        t2d_out.array_size,
        &info,
        &mut rtv_miss,
    )
    .expect("rtv_miss normalize");
    let rtv_miss_sub =
        ResourceSubset::from_texture_view(&rtv_miss_mtl, t2d_out.mip_levels, t2d_out.array_size, 0);
    assert!(!srv_sub.overlaps(rtv_miss_sub));

    // 3. DSV subset with read-only planar flags on D24S8 texture.
    let d24 = D3d11Texture2dDesc1 {
        format: DXGI_FORMAT_D24_UNORM_S8_UINT,
        bind_flags: D3D11_BIND_DEPTH_STENCIL,
        mip_levels: 1,
        array_size: 2,
        ..t2d
    };
    let mut d24_out = d24;
    let mut d24_info = zero_tex_info();
    let _ = res_helper::create_mtl_texture_descriptor_2d(
        &d24,
        &mut d24_out,
        &mut d24_info,
        |f| f <= WMTGPU_FAMILY_APPLE8,
        true,
    );
    let mut dsv_ro_depth = D3d11DepthStencilViewDesc {
        format: DXGI_FORMAT_D24_UNORM_S8_UINT,
        view_dimension: D3D11_DSV_DIMENSION_TEXTURE2DARRAY,
        flags: D3D11_DSV_READ_ONLY_DEPTH,
        u: D3d11DepthStencilViewDescUnion {
            texture2d_array: D3d11Tex2dArrayDsv {
                mip_slice: 0,
                first_array_slice: 0,
                array_size: 2,
            },
        },
    };
    let (_, dsv_mtl) = helper::initialize_and_normalize_dsv_descriptor(
        d24_out.mip_levels,
        d24_out.array_size,
        &d24_info,
        &mut dsv_ro_depth,
    )
    .expect("dsv normalize");
    assert_eq!(dsv_mtl.format, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8);
    let sub_ro_depth = ResourceSubset::from_texture_view(
        &dsv_mtl,
        d24_out.mip_levels,
        d24_out.array_size,
        dsv_ro_depth.flags & 0b11,
    );
    let sub_ro_stencil = ResourceSubset::from_texture_view(
        &dsv_mtl,
        d24_out.mip_levels,
        d24_out.array_size,
        D3D11_DSV_READ_ONLY_STENCIL,
    );
    // Read-only depth writes only stencil (plane 1), read-only stencil writes only depth (plane 0).
    assert!(!sub_ro_depth.overlaps(sub_ro_stencil));

    // Cube view subset covers 6 slices.
    let cube_mtl = TextureViewDescriptor {
        format: WMT_PIXEL_FORMAT_RGBA8_UNORM,
        texture_type: WMT_TEXTURE_TYPE_CUBE,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 6,
    };
    let slice5_mtl = TextureViewDescriptor {
        format: WMT_PIXEL_FORMAT_RGBA8_UNORM,
        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
        first_mip: 0,
        mip_count: 1,
        first_slice: 5,
        array_size: 1,
    };
    assert!(
        ResourceSubset::from_texture_view(&cube_mtl, 1, 6, 0)
            .overlaps(ResourceSubset::from_texture_view(&slice5_mtl, 1, 6, 0))
    );
}

#[test]
fn null_desc_takes_the_resource_format() {
    // 1D texture with R32_FLOAT: null SRV/UAV/RTV desc inherits DXGI_FORMAT_R32_FLOAT -> WMT_PIXEL_FORMAT_R32_FLOAT.
    let t1d = D3d11Texture1dDesc {
        width: 64,
        mip_levels: 2,
        array_size: 1,
        format: DXGI_FORMAT_R32_FLOAT,
        usage: 0,
        bind_flags: D3D11_BIND_SHADER_RESOURCE
            | D3D11_BIND_RENDER_TARGET
            | D3D11_BIND_UNORDERED_ACCESS,
        cpu_access_flags: 0,
        misc_flags: 0,
    };
    let mut t1d_out = t1d;
    let mut info_1d = zero_tex_info();
    let _ = res_helper::create_mtl_texture_descriptor_1d(
        &t1d,
        &mut t1d_out,
        &mut info_1d,
        |f| f <= WMTGPU_FAMILY_APPLE8,
        true,
    );
    let mut srv_1d = res_helper::extract_entire_srv_1d(&t1d_out, None).expect("default 1d srv");
    assert_eq!(srv_1d.format, DXGI_FORMAT_R32_FLOAT);
    let srv_1d_mtl = helper::initialize_and_normalize_srv_descriptor(
        t1d_out.mip_levels,
        t1d_out.array_size,
        &info_1d,
        &mut srv_1d,
    )
    .expect("norm 1d srv");
    assert_eq!(srv_1d_mtl.format, WMT_PIXEL_FORMAT_R32_FLOAT);

    // 2D texture with R8G8B8A8_UNORM_SRGB: null desc and explicit DXGI_FORMAT_UNKNOWN both take the resource format.
    let t2d = D3d11Texture2dDesc1 {
        width: 64,
        height: 64,
        mip_levels: 1,
        array_size: 1,
        format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        sample_desc: DxgiSampleDesc {
            count: 1,
            quality: 0,
        },
        usage: 0,
        bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: 0,
    };
    let mut t2d_out = t2d;
    let mut info_2d = zero_tex_info();
    let _ = res_helper::create_mtl_texture_descriptor_2d(
        &t2d,
        &mut t2d_out,
        &mut info_2d,
        |f| f <= WMTGPU_FAMILY_APPLE8,
        true,
    );
    let mut rtv_2d = res_helper::extract_entire_rtv_2d(&t2d_out, None).expect("default 2d rtv");
    assert_eq!(rtv_2d.format, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB);
    let (_, rtv_2d_mtl) = helper::initialize_and_normalize_rtv_descriptor(
        t2d_out.mip_levels,
        t2d_out.array_size,
        &info_2d,
        &mut rtv_2d,
    )
    .expect("norm 2d rtv");
    assert_eq!(rtv_2d_mtl.format, WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB);

    // Explicit SRV with DXGI_FORMAT_UNKNOWN also resolves to the resource's format.
    let unknown_srv_in = D3d11ShaderResourceViewDesc1 {
        format: DXGI_FORMAT_UNKNOWN,
        view_dimension: 4, // TEXTURE2D
        u: D3d11ShaderResourceViewDesc1Union {
            texture2d: D3d11Tex2dSrv1 {
                most_detailed_mip: 0,
                mip_levels: 1,
                plane_slice: 0,
            },
        },
    };
    let mut srv_2d =
        res_helper::extract_entire_srv_2d(&t2d_out, Some(&unknown_srv_in)).expect("srv unknown");
    assert_eq!(srv_2d.format, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB);
    let srv_2d_mtl = helper::initialize_and_normalize_srv_descriptor(
        t2d_out.mip_levels,
        t2d_out.array_size,
        &info_2d,
        &mut srv_2d,
    )
    .expect("norm 2d srv");
    assert_eq!(srv_2d_mtl.format, WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB);

    // 2D D32_FLOAT texture: null DSV desc takes DXGI_FORMAT_D32_FLOAT.
    let t2d_d32 = D3d11Texture2dDesc1 {
        format: DXGI_FORMAT_D32_FLOAT,
        bind_flags: D3D11_BIND_DEPTH_STENCIL,
        ..t2d
    };
    let dsv_2d = res_helper::extract_entire_dsv_2d(&t2d_d32, None).expect("default 2d dsv");
    assert_eq!(dsv_2d.format, DXGI_FORMAT_D32_FLOAT);

    // 3D texture with R8G8B8A8_UNORM: null UAV desc takes DXGI_FORMAT_R8G8B8A8_UNORM -> WMT_PIXEL_FORMAT_RGBA8_UNORM.
    let t3d = D3d11Texture3dDesc1 {
        width: 16,
        height: 16,
        depth: 4,
        mip_levels: 1,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        usage: 0,
        bind_flags: D3D11_BIND_UNORDERED_ACCESS,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: 0,
    };
    let mut t3d_out = t3d;
    let mut info_3d = zero_tex_info();
    let _ = res_helper::create_mtl_texture_descriptor_3d(
        &t3d,
        &mut t3d_out,
        &mut info_3d,
        |f| f <= WMTGPU_FAMILY_APPLE8,
        true,
    );
    let mut uav_3d = res_helper::extract_entire_uav_3d(&t3d_out, None).expect("default 3d uav");
    assert_eq!(uav_3d.format, DXGI_FORMAT_R8G8B8A8_UNORM);
    let uav_3d_mtl = helper::initialize_and_normalize_uav_descriptor(
        t3d_out.mip_levels,
        t3d_out.depth,
        &info_3d,
        &mut uav_3d,
    )
    .expect("norm 3d uav");
    assert_eq!(uav_3d_mtl.format, WMT_PIXEL_FORMAT_RGBA8_UNORM);
    assert_eq!(uav_3d_mtl.texture_type, WMT_TEXTURE_TYPE3D);
}
