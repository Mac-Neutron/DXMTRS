use core::slice;
use std::sync::Arc;

use winemetal_sys::{
    Obj,
    wire::{
        WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_SHADER_READ,
        WMTTextureInfo, WmtcmdRenderSetbytes,
    },
};

use super::*;
use crate::{
    resource::TextureAllocationFlags,
    test_support::{Records, format_pass_records},
};

/// A 2-level 16×16 texture with an allocation on a made-up heap.
fn mipmapped() -> Arc<Texture> {
    // SAFETY: made-up handles, which the test binary's fakes never dereference.
    let device = unsafe { Obj::adopt(0xde) }.expect("a handle");
    // SAFETY: as above.
    let heap = unsafe { Obj::adopt(0x4e) }.expect("a handle");
    let info = WMTTextureInfo {
        pixel_format: WMT_PIXEL_FORMAT_RGBA8_UNORM,
        width: 16,
        height: 16,
        depth: 1,
        array_length: 1,
        bits: (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
            | (2 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
            | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
            | (WMT_TEXTURE_USAGE_SHADER_READ << WMTTextureInfo::USAGE_SHIFT),
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let texture = Arc::new(Texture::new(info, &device));
    let allocation = texture.allocate_on_heap(TextureAllocationFlags::empty(), &heap, 0x300);
    assert!(texture.rename(allocation).is_none());
    texture
}

/// The open render pass's records, as text.
fn lines(encoder: &Encoder) -> Vec<String> {
    let head = encoder.pass().expect("a pass").records();
    // SAFETY: the pass's own list, in the encoder's memory.
    unsafe { format_pass_records(&Records::Render, head.as_ptr()) }
}

/// The view's whole range is read: a later pass writing level 1 alone waits for the bind's pass.
#[test]
fn bind_texture_tracks_the_view_then_records() {
    let texture = mipmapped();
    let mut encoder = Encoder::headless();
    encoder.start_render_pass(0, 0, 0, None);
    bind_texture::<{ stage::PIXEL }>(&mut encoder, 3, Some((&texture, texture.full_view())));
    let view = texture.current().expect("an allocation").texture();
    assert_eq!(
        lines(&encoder),
        [format!("render 10 texture=#{view} index=3")]
    );
    bind_texture::<{ stage::VERTEX }>(&mut encoder, 0, None);
    assert_eq!(lines(&encoder)[1], "render 47 texture=#0 index=0");
    encoder.end_pass();
    encoder.start_compute_pass(None);
    encoder.access_texture::<{ stage::COMPUTE }>(&texture, 1, 0, Access::WRITE);
    encoder.end_pass();
    let (bind, write) = (&encoder.passes[0], &encoder.passes[1]);
    assert!(
        write.fence_wait.test(bind.id),
        "the write to level 1 waits for the pass that reads the whole view"
    );
}

/// 4096 bytes go in the record, copied; 4097 go to the argument-buffer ring, bound as a buffer.
#[test]
fn bind_bytes_inline_up_to_4k_and_ring_beyond() {
    let mut encoder = Encoder::headless();
    encoder.start_render_pass(0, 0, 0, None);
    let small = [7_u8; SET_BYTES_MAX];
    bind_bytes::<{ stage::VERTEX }>(&mut encoder, 5, &small);
    bind_bytes::<{ stage::PIXEL }>(&mut encoder, 6, &[9; SET_BYTES_MAX + 1]);
    assert_eq!(
        lines(&encoder),
        [
            "render 46 length=4096 index=5",
            // A headless encoder's ring is its own memory, in no Metal buffer.
            "render 4 buffer=#0 offset=0 index=6",
        ]
    );
    let head = encoder.pass().expect("a pass").records();
    // SAFETY: the pass's head, in the encoder's memory.
    let first = unsafe { head.as_ref() }.next.ptr;
    let first = core::ptr::with_exposed_provenance::<WmtcmdRenderSetbytes>(
        usize::try_from(first).expect("an address"),
    );
    // SAFETY: the pass's first record, a set-bytes one, in the encoder's memory.
    let record = unsafe { &*first };
    let copy = record.bytes.as_ptr::<u8>();
    assert_ne!(
        copy.cast_const(),
        small.as_ptr(),
        "a copy, not the caller's bytes"
    );
    // SAFETY: the record's `length` bytes, in the encoder's memory until the next chunk.
    let copied = unsafe { slice::from_raw_parts(copy, SET_BYTES_MAX) };
    assert_eq!(copied, small);
    encoder.end_pass();
}
