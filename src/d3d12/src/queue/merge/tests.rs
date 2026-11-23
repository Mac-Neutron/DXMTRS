//! Unit tests for render-pass merging and clear folding.

use std::sync::Arc;

use dxmt_core::resource::{Texture, TextureAllocationFlags, TextureViewDescriptor};
use winemetal_sys::{
    Obj,
    wire::{
        WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_DONT_CARE, WMT_LOAD_ACTION_LOAD,
        WMT_STORE_ACTION_DONT_CARE, WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE2D, WMTClearColor,
        WMTRenderPassInfo, WMTTextureInfo,
    },
};

use super::{apply_clear, same_target};
use crate::{
    encoder::{
        CLEAR_DEPTH_STENCIL, ClearEncoderData, EncoderData, EncoderType, TextureViewRef, clear_slot,
    },
    list::List,
};

fn make_test_view(pixel_format: u32) -> TextureViewRef {
    let info = WMTTextureInfo {
        pixel_format,
        width: 64,
        height: 64,
        depth: 1,
        array_length: 1,
        bits: (WMT_TEXTURE_TYPE2D & 0xff) | (1 << 8) | (1 << 16),
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    // SAFETY: `crate::tests` stubs retain/release for test `Obj` handles.
    let obj = unsafe { Obj::adopt(1) }.expect("non-zero fake handle");
    let tex = Arc::new(Texture::new(info, &obj));
    let allocation = tex.allocate(TextureAllocationFlags::SHARED);
    let key = TextureViewDescriptor {
        format: pixel_format,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();
    TextureViewRef {
        descriptor: tex,
        allocation,
        key,
        texture: u64::from(pixel_format),
        gpu_resource_id: u64::from(pixel_format),
    }
}

#[test]
fn same_target_merges_readonly_dsv_dont_care() {
    let ds = make_test_view(260);
    let mut a = List::empty_render_pass();
    let mut b = List::empty_render_pass();
    a.dsv_planar_flags = 3;
    b.dsv_planar_flags = 3;
    a.dsv_readonly_flags = 3;
    b.dsv_readonly_flags = 3;

    a.depth.attachment = Some(ds.clone());
    b.depth.attachment = Some(ds.clone());
    a.depth.load_action = WMT_LOAD_ACTION_LOAD;
    b.depth.load_action = WMT_LOAD_ACTION_LOAD;
    a.depth.store_action = WMT_STORE_ACTION_DONT_CARE;
    b.depth.store_action = WMT_STORE_ACTION_DONT_CARE;

    a.stencil.attachment = Some(ds.clone());
    b.stencil.attachment = Some(ds);
    a.stencil.load_action = WMT_LOAD_ACTION_LOAD;
    b.stencil.load_action = WMT_LOAD_ACTION_LOAD;
    a.stencil.store_action = WMT_STORE_ACTION_DONT_CARE;
    b.stencil.store_action = WMT_STORE_ACTION_DONT_CARE;

    assert!(same_target(&a, &b));

    // Writable depth/stencil (`dsv_readonly_flags == 0`) with DONT_CARE must not merge.
    a.dsv_readonly_flags = 1;
    b.dsv_readonly_flags = 1;
    assert!(!same_target(&a, &b));

    a.dsv_readonly_flags = 2;
    b.dsv_readonly_flags = 2;
    assert!(!same_target(&a, &b));
}

#[test]
fn apply_clear_preserves_dont_care_load_action() {
    let rt0 = make_test_view(70);
    let ds = make_test_view(260);

    let mut r = List::empty_render_pass();
    r.render_target_width = 64;
    r.render_target_height = 64;
    r.render_target_array_length = 1;
    r.render_target_count = 1;
    r.colors[0].attachment = Some(rt0.clone());
    r.colors[0].load_action = WMT_LOAD_ACTION_DONT_CARE;
    r.depth.attachment = Some(ds.clone());
    r.depth.load_action = WMT_LOAD_ACTION_DONT_CARE;
    r.stencil.attachment = Some(ds.clone());
    r.stencil.load_action = WMT_LOAD_ACTION_DONT_CARE;
    r.dsv_planar_flags = 3;

    let clear_rt = ClearEncoderData {
        base: EncoderData::new(EncoderType::Clear, 0),
        color: WMTClearColor {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        depth_stencil: (0.0, 0),
        attachment: Some(rt0),
        clear_dsv: 0,
        array_length: 1,
        width: 64,
        height: 64,
        depth_plane: 0,
    };
    let clear_ds = ClearEncoderData {
        base: EncoderData::new(EncoderType::Clear, 1),
        color: WMTClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
        depth_stencil: (0.5, 42),
        attachment: Some(ds),
        clear_dsv: 3,
        array_length: 1,
        width: 64,
        height: 64,
        depth_plane: 0,
    };

    // SAFETY: `WMTRenderPassInfo` is a C POD struct valid when zeroed.
    let mut info: WMTRenderPassInfo = unsafe { core::mem::zeroed() };
    info.colors[0].load_action = WMT_LOAD_ACTION_DONT_CARE;
    info.colors[0].store_action = WMT_STORE_ACTION_STORE;
    info.depth.load_action = WMT_LOAD_ACTION_DONT_CARE;
    info.depth.store_action = WMT_STORE_ACTION_DONT_CARE;
    info.stencil.load_action = WMT_LOAD_ACTION_DONT_CARE;
    info.stencil.store_action = WMT_STORE_ACTION_DONT_CARE;

    assert_eq!(clear_slot(&clear_rt, &r), 0);
    assert_eq!(clear_slot(&clear_ds, &r), CLEAR_DEPTH_STENCIL);
    apply_clear(&clear_rt, &r, &mut info);
    apply_clear(&clear_ds, &r, &mut info);

    assert_eq!(info.colors[0].load_action, WMT_LOAD_ACTION_DONT_CARE);
    assert_eq!(info.depth.load_action, WMT_LOAD_ACTION_DONT_CARE);
    assert_eq!(info.stencil.load_action, WMT_LOAD_ACTION_DONT_CARE);

    // When the pass genuinely loads, the clear folds as usual.
    r.colors[0].load_action = WMT_LOAD_ACTION_LOAD;
    r.depth.load_action = WMT_LOAD_ACTION_LOAD;
    r.stencil.load_action = WMT_LOAD_ACTION_LOAD;
    info.colors[0].load_action = WMT_LOAD_ACTION_LOAD;
    info.depth.load_action = WMT_LOAD_ACTION_LOAD;
    info.stencil.load_action = WMT_LOAD_ACTION_LOAD;

    apply_clear(&clear_rt, &r, &mut info);
    apply_clear(&clear_ds, &r, &mut info);

    assert_eq!(info.colors[0].load_action, WMT_LOAD_ACTION_CLEAR);
    assert_eq!(info.depth.load_action, WMT_LOAD_ACTION_CLEAR);
    assert_eq!(info.stencil.load_action, WMT_LOAD_ACTION_CLEAR);
}
