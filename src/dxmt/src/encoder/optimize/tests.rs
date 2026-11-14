use core::ptr::{self, NonNull};
use std::sync::Arc;

use winemetal_sys::{
    Obj,
    wire::{
        WMT_LOAD_ACTION_DONT_CARE, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_PIXEL_FORMAT_RGBA8_UNORM,
        WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET, WMT_TEXTURE_TYPE2D,
        WMT_TEXTURE_USAGE_RENDER_TARGET, WMTClearColor, WMTTextureInfo, WmtcmdBase,
        WmtcmdRenderDraw, WmtcmdRenderSetbufferoffset,
    },
};
use winemetal_wire::MemPtr;

use super::*;
use crate::{
    deptrack::Access,
    encoder::{Target, direct, stage},
    resource::{Texture, TextureAllocationFlags},
    test_support::{Records, format_pass_records},
};

/// `MTLHeap_newTexture`: a made-up handle per offset, which nothing dereferences.
#[unsafe(export_name = "MTLHeap_newTexture")]
extern "C-unwind" fn fake_heap_new_texture(
    _heap: u64,
    _info: *mut WMTTextureInfo,
    offset: u64,
) -> u64 {
    0x7e00_0000 + offset
}

/// A 64×64 render target with an allocation (on a made-up heap, at `at`).
fn texture(at: u64) -> Arc<Texture> {
    sized(at, WMT_PIXEL_FORMAT_RGBA8_UNORM, 64)
}

/// A `size`×`size` render target of `pixel_format` with an allocation.
///
/// On a made-up heap, at `at`.
fn sized(at: u64, pixel_format: u32, size: u32) -> Arc<Texture> {
    // SAFETY: made-up handles; the fakes that see them (the shader cache tests' `NSObject_retain`
    // and `NSObject_release`, which log per thread) never dereference them.
    let device = unsafe { Obj::adopt(0xde) }.expect("a handle");
    // SAFETY: as above.
    let heap = unsafe { Obj::adopt(0x4e) }.expect("a handle");
    let info = WMTTextureInfo {
        pixel_format,
        width: size,
        height: size,
        depth: 1,
        array_length: 1,
        bits: (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
            | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
            | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
            | (WMT_TEXTURE_USAGE_RENDER_TARGET << WMTTextureInfo::USAGE_SHIFT),
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let texture = Arc::new(Texture::new(info, &device));
    let allocation = texture.allocate_on_heap(TextureAllocationFlags::empty(), &heap, at);
    assert!(texture.rename(allocation).is_none());
    texture
}

const COLOR: WMTClearColor = WMTClearColor {
    r: 0.25,
    g: 0.5,
    b: 0.75,
    a: 1.0,
};

/// A render pass drawing to `texture` (loaded with `load`, stored), with records `values`.
fn draw(encoder: &mut Encoder, texture: &Arc<Texture>, load: u32, values: &[u64]) {
    encoder.start_render_pass(0, 0, 1, None);
    let attachment =
        encoder.attachment::<{ stage::PIXEL }>(texture, texture.full_view(), Access::WRITE);
    let targets = encoder.render_targets().expect("a render pass");
    targets.colors[0].target = Target {
        attachment,
        load_action: load,
        store_action: WMT_STORE_ACTION_STORE,
        level: 0,
        slice: 0,
        depth_plane: 0,
    };
    targets.render_target_array_length = 1;
    for &offset in values {
        encoder.encode_render_command(WmtcmdRenderSetbufferoffset {
            r#type: WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
            reserved: [0; 3],
            next: MemPtr::NULL,
            offset,
            index: 0,
        });
    }
    encoder.end_pass();
}

/// A compute pass that reads `texture`.
fn read(encoder: &mut Encoder, texture: &Texture) {
    encoder.start_compute_pass(None);
    encoder.access_texture_view::<{ stage::COMPUTE }>(texture, texture.full_view(), Access::READ);
    encoder.end_pass();
}

fn targets(pass: &Pass) -> &RenderTargets {
    match &pass.kind {
        PassKind::Render(targets) => targets,
        _ => panic!("a render pass"),
    }
}

/// The offsets of the pass's records after its head, in list order.
fn offsets(pass: &Pass) -> Vec<u64> {
    let mut out = Vec::new();
    let mut at = pass.records();
    loop {
        // SAFETY: a record of the pass in the encoder's memory, led by `WmtcmdBase`'s fields.
        let next = unsafe { at.as_ref() }.next.ptr;
        let Some(record) = NonNull::new(ptr::with_exposed_provenance_mut::<WmtcmdBase>(
            usize::try_from(next).expect("an address"),
        )) else {
            return out;
        };
        // SAFETY: the tests record only set-buffer-offset records.
        out.push(unsafe { record.cast::<WmtcmdRenderSetbufferoffset>().as_ref() }.offset);
        at = record;
    }
}

#[test]
fn optimizer_merges_clear_into_render() {
    let (target, other) = (texture(0x100), texture(0x200));
    let mut encoder = Encoder::headless();
    encoder.clear_color(&target, target.full_view(), 1, COLOR);
    let clear = encoder.passes[0].id;
    read(&mut encoder, &other);
    draw(&mut encoder, &target, WMT_LOAD_ACTION_LOAD, &[1]);
    assert!(
        encoder.passes[2].fence_wait.test(clear),
        "the render pass waits for the clear"
    );
    encoder.optimize();
    assert!(matches!(encoder.passes[0].kind, PassKind::Null));
    assert!(
        matches!(encoder.passes[1].kind, PassKind::Compute),
        "a pass that reads another texture stays between"
    );
    let render = &encoder.passes[2];
    let color = &targets(render).colors[0];
    assert_eq!(color.target.load_action, WMT_LOAD_ACTION_CLEAR);
    let c = color.clear_color;
    assert_eq!([c.r, c.g, c.b, c.a], [0.25, 0.5, 0.75, 1.0]);
    assert!(
        render.fence_update.test(clear),
        "it updates the clear's fence"
    );
    assert!(!render.fence_wait.test(clear), "and no longer waits for it");
}

#[test]
fn optimizer_refuses_across_dependency() {
    let target = texture(0x100);
    let mut encoder = Encoder::headless();
    encoder.clear_color(&target, target.full_view(), 1, COLOR);
    read(&mut encoder, &target);
    draw(&mut encoder, &target, WMT_LOAD_ACTION_LOAD, &[1]);
    read(&mut encoder, &target);
    draw(&mut encoder, &target, WMT_LOAD_ACTION_LOAD, &[2]);
    encoder.optimize();
    let kinds: Vec<_> = encoder
        .passes
        .iter()
        .map(|p| match p.kind {
            PassKind::Clear(_) => 'c',
            PassKind::Compute => 'x',
            PassKind::Render(_) => 'r',
            _ => '?',
        })
        .collect();
    assert_eq!(
        kinds,
        ['c', 'x', 'r', 'x', 'r'],
        "a pass that reads the target in between keeps every pass"
    );
    for render in [&encoder.passes[2], &encoder.passes[4]] {
        assert_eq!(
            targets(render).colors[0].target.load_action,
            WMT_LOAD_ACTION_LOAD
        );
    }
    assert_eq!(offsets(&encoder.passes[4]), [2]);
}

#[test]
fn records_keep_their_order() {
    let target = texture(0x100);
    let mut encoder = Encoder::headless();
    encoder.clear_color(&target, target.full_view(), 1, COLOR);
    draw(&mut encoder, &target, WMT_LOAD_ACTION_DONT_CARE, &[1, 2]);
    let first = encoder.passes[1].id;
    draw(&mut encoder, &target, WMT_LOAD_ACTION_LOAD, &[3, 4]);
    encoder.optimize();
    assert!(matches!(encoder.passes[0].kind, PassKind::Null));
    assert!(matches!(encoder.passes[1].kind, PassKind::Null));
    let merged = &encoder.passes[2];
    assert_eq!(
        offsets(merged),
        [1, 2, 3, 4],
        "the earlier pass's records come first"
    );
    assert_eq!(
        targets(merged).colors[0].target.load_action,
        WMT_LOAD_ACTION_DONT_CARE,
        "the earlier pass's load action; the clear before it, past the merged pass, is dropped"
    );
    assert!(merged.fence_update.test(first) && merged.fence_update_vertex.test(first - 1));
    assert!(!merged.fence_wait.test(first));
    assert_eq!(
        offsets(&encoder.passes[1]),
        [] as [u64; 0],
        "the merged pass keeps no record"
    );
}

/// A target drawn again 128 ids on keeps its first pass where it is (DXMTRS C4's deptrack fix).
///
/// The second pass no longer waits for the first's fence itself (its write is over [`LANE`] ids
/// old); a pass about 64 ids after the first does, through the weak waits, and stops the scan
/// before the first pass could be moved or merged past the second.
#[test]
fn a_target_drawn_again_128_ids_on_keeps_its_first_pass() {
    let target = texture(0x100);
    let mut encoder = Encoder::headless();
    draw(&mut encoder, &target, WMT_LOAD_ACTION_DONT_CARE, &[1]);
    let first = encoder.passes[0].id;
    let others: Vec<_> = (0..63).map(|n| texture(0x1000 + n * 0x100)).collect();
    for other in &others {
        draw(&mut encoder, other, WMT_LOAD_ACTION_DONT_CARE, &[0]);
    }
    draw(&mut encoder, &target, WMT_LOAD_ACTION_LOAD, &[2]);
    let last = encoder.passes.len() - 1;
    assert!(
        encoder.passes[last].id - first >= 128,
        "the second pass is 128 ids on"
    );
    assert!(
        !encoder.passes[last].fence_wait.test(first),
        "and waits for the first only through other passes"
    );
    encoder.optimize();
    assert_eq!(
        offsets(&encoder.passes[0]),
        [1],
        "the first pass stays first"
    );
    assert_eq!(
        offsets(&encoder.passes[last]),
        [2],
        "the second keeps only its own records"
    );
}

/// A render pass drawing to `texture` (loaded with `load`), which binds `bytes` itself, then draws.
fn draw_with_own_binds(encoder: &mut Encoder, texture: &Arc<Texture>, load: u32, bytes: &[u8]) {
    encoder.start_render_pass(0, 0, 1, None);
    let attachment =
        encoder.attachment::<{ stage::PIXEL }>(texture, texture.full_view(), Access::WRITE);
    let targets = encoder.render_targets().expect("a render pass");
    targets.colors[0].target = Target {
        attachment,
        load_action: load,
        store_action: WMT_STORE_ACTION_STORE,
        level: 0,
        slice: 0,
        depth_plane: 0,
    };
    targets.render_target_array_length = 1;
    direct::bind_bytes::<{ stage::VERTEX }>(encoder, 0, bytes);
    encoder.encode_render_command(WmtcmdRenderDraw {
        r#type: WMT_RENDER_COMMAND_DRAW,
        reserved: [0; 3],
        next: MemPtr::NULL,
        primitive_type: 3,
        vertex_start: 0,
        vertex_count: 3,
        instance_count: 1,
        base_instance: 0,
    });
    encoder.end_pass();
}

/// Two same-target passes without argument buffers merge, each pass's binds before its own draw.
///
/// DXMTRS C4-8: a D3D9 render pass re-emits all its state.
#[test]
fn merged_passes_keep_each_pass_state() {
    let target = texture(0x100);
    let mut encoder = Encoder::headless();
    draw_with_own_binds(&mut encoder, &target, WMT_LOAD_ACTION_DONT_CARE, &[1; 16]);
    draw_with_own_binds(&mut encoder, &target, WMT_LOAD_ACTION_LOAD, &[2; 32]);
    encoder.optimize();
    assert!(matches!(encoder.passes[0].kind, PassKind::Null));
    // SAFETY: the merged pass's own list, in the encoder's memory.
    let lines =
        unsafe { format_pass_records(&Records::Render, encoder.passes[1].records().as_ptr()) };
    let draw = "render 19 primitive_type=3 vertex_start=0 vertex_count=3 instance_count=1 \
                base_instance=0";
    assert_eq!(
        lines,
        [
            "render 46 length=16 index=0",
            draw,
            "render 46 length=32 index=0",
            draw
        ]
    );
}

/// A render pass of `width`×`height` (0: unset) that loads and stores `depth`'s depth plane.
fn depth_pass(encoder: &mut Encoder, depth: &Arc<Texture>, width: u32, height: u32) {
    encoder.start_render_pass(1, 0, 0, None);
    let attachment =
        encoder.attachment::<{ stage::PIXEL }>(depth, depth.full_view(), Access::WRITE);
    let targets = encoder.render_targets().expect("a render pass");
    targets.depth.target = Target {
        attachment,
        load_action: WMT_LOAD_ACTION_LOAD,
        store_action: WMT_STORE_ACTION_STORE,
        level: 0,
        slice: 0,
        depth_plane: 0,
    };
    targets.render_target_array_length = 1;
    targets.render_target_width = width;
    targets.render_target_height = height;
    encoder.end_pass();
}

#[test]
fn a_clear_is_not_folded_into_a_smaller_pass() {
    // A depth view of 128×128 cleared, then a pass of this area that attaches it.
    for (width, height, folded) in [(64, 32, false), (128, 128, true), (0, 0, true)] {
        let depth = sized(0x100, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 128);
        let mut encoder = Encoder::headless();
        encoder.clear_depth_stencil(&depth, depth.full_view(), 1, 1, 0.5, 0);
        depth_pass(&mut encoder, &depth, width, height);
        encoder.optimize();
        assert_eq!(
            matches!(encoder.passes[0].kind, PassKind::Null),
            folded,
            "a {width}x{height} pass"
        );
        let load = if folded {
            WMT_LOAD_ACTION_CLEAR
        } else {
            WMT_LOAD_ACTION_LOAD
        };
        assert_eq!(targets(&encoder.passes[1]).depth.target.load_action, load);
    }
    // A colour view of 128×128 cleared, then a pass of 64×64 on it.
    let color = sized(0x200, WMT_PIXEL_FORMAT_RGBA8_UNORM, 128);
    let mut encoder = Encoder::headless();
    encoder.clear_color(&color, color.full_view(), 1, COLOR);
    draw(&mut encoder, &color, WMT_LOAD_ACTION_LOAD, &[1]);
    if let PassKind::Render(targets) = &mut encoder.passes[1].kind {
        targets.render_target_width = 64;
        targets.render_target_height = 64;
    }
    encoder.optimize();
    assert!(matches!(encoder.passes[0].kind, PassKind::Clear(_)));
    assert_eq!(
        targets(&encoder.passes[1]).colors[0].target.load_action,
        WMT_LOAD_ACTION_LOAD
    );
}
