use dxmt_core::resource::TextureViewDescriptor;
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, WMT_PIXEL_FORMAT_R32X8X32,
    WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_PIXEL_FORMAT_X32_STENCIL8, WMT_PIXEL_FORMAT_X32G8X32,
    WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
};

use super::{ResourceSubset, planar_count, planar_mask};

#[test]
fn sets_as_cpp() {
    assert_eq!(planar_count(WMT_PIXEL_FORMAT_RGBA8_UNORM), 1);
    assert_eq!(planar_count(WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8), 2);
    assert_eq!(planar_count(WMT_PIXEL_FORMAT_R32X8X32), 2);
    assert_eq!(planar_count(WMT_PIXEL_FORMAT_X32G8X32), 2);
    assert_eq!(planar_count(WMT_PIXEL_FORMAT_X32_STENCIL8), 2);

    assert_eq!(planar_mask(WMT_PIXEL_FORMAT_RGBA8_UNORM), 0b01);
    assert_eq!(planar_mask(WMT_PIXEL_FORMAT_R32X8X32), 0b01);
    assert_eq!(planar_mask(WMT_PIXEL_FORMAT_X32G8X32), 0b10);
    assert_eq!(planar_mask(WMT_PIXEL_FORMAT_X32_STENCIL8), 0b10);
    assert_eq!(planar_mask(WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8), 0b11);

    // Whole resource overlaps with everything.
    let whole = ResourceSubset::WHOLE;
    let buf_a = ResourceSubset::buffer(0, 64);
    let buf_b = ResourceSubset::buffer(64, 64);
    let buf_c = ResourceSubset::buffer(32, 64);
    assert!(whole.overlaps(buf_a));
    assert!(buf_a.overlaps(whole));
    assert!(!buf_a.overlaps(buf_b));
    assert!(buf_a.overlaps(buf_c));
    assert!(buf_b.overlaps(buf_c));

    // Bitmask-encoded texture subset (total_planar * total_mip_count * total_array_size <= 62).
    let view_mip0_slice0 = TextureViewDescriptor {
        format: WMT_PIXEL_FORMAT_RGBA8_UNORM,
        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    };
    let view_mip1_slice0 = TextureViewDescriptor {
        first_mip: 1,
        ..view_mip0_slice0
    };
    let view_mip0_slice1 = TextureViewDescriptor {
        first_slice: 1,
        ..view_mip0_slice0
    };
    let sub_m0_s0 = ResourceSubset::from_texture_view(&view_mip0_slice0, 4, 4, 0);
    let sub_m1_s0 = ResourceSubset::from_texture_view(&view_mip1_slice0, 4, 4, 0);
    let sub_m0_s1 = ResourceSubset::from_texture_view(&view_mip0_slice1, 4, 4, 0);
    assert_eq!(sub_m0_s0.0 & 0b11, 0b11);
    assert!(sub_m0_s0.overlaps(sub_m0_s0));
    assert!(!sub_m0_s0.overlaps(sub_m1_s0));
    assert!(!sub_m0_s0.overlaps(sub_m0_s1));
    assert!(!sub_m0_s0.overlaps(buf_a));

    // Planar depth/stencil bitmask subsets and ignore_planar_mask (readonly DSV).
    let depth_srv = TextureViewDescriptor {
        format: WMT_PIXEL_FORMAT_R32X8X32,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    };
    let stencil_srv = TextureViewDescriptor {
        format: WMT_PIXEL_FORMAT_X32G8X32,
        ..depth_srv
    };
    let ds_dsv = TextureViewDescriptor {
        format: WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
        ..depth_srv
    };
    let sub_depth = ResourceSubset::from_texture_view(&depth_srv, 1, 1, 0);
    let sub_stencil = ResourceSubset::from_texture_view(&stencil_srv, 1, 1, 0);
    let sub_dsv_rw = ResourceSubset::from_texture_view(&ds_dsv, 1, 1, 0);
    let sub_dsv_ro_depth = ResourceSubset::from_texture_view(&ds_dsv, 1, 1, 0b01);
    let sub_dsv_ro_both = ResourceSubset::from_texture_view(&ds_dsv, 1, 1, 0b11);
    assert!(!sub_depth.overlaps(sub_stencil));
    assert!(sub_depth.overlaps(sub_dsv_rw));
    assert!(sub_stencil.overlaps(sub_dsv_rw));
    assert!(!sub_depth.overlaps(sub_dsv_ro_depth));
    assert!(sub_stencil.overlaps(sub_dsv_ro_depth));
    assert!(!sub_depth.overlaps(sub_dsv_ro_both));
    assert!(!sub_stencil.overlaps(sub_dsv_ro_both));

    // Range-encoded texture subset (total_planar * total_mip_count * total_array_size > 62).
    let large_a = ResourceSubset::from_texture_view(&view_mip0_slice0, 8, 16, 0);
    let large_b = ResourceSubset::from_texture_view(&view_mip1_slice0, 8, 16, 0);
    let large_c = ResourceSubset::from_texture_view(&view_mip0_slice1, 8, 16, 0);
    assert_eq!(large_a.0 & 0b11, 0b10);
    assert!(large_a.overlaps(large_a));
    assert!(!large_a.overlaps(large_b));
    assert!(!large_a.overlaps(large_c));
}
