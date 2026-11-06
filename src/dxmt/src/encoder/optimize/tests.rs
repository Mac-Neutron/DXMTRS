use core::ptr::{self, NonNull};
use std::sync::Arc;

use winemetal_sys::{
    Obj,
    wire::{
        WMT_LOAD_ACTION_DONT_CARE, WMT_PIXEL_FORMAT_RGBA8_UNORM,
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET, WMT_TEXTURE_TYPE2D,
        WMT_TEXTURE_USAGE_RENDER_TARGET, WMTClearColor, WMTTextureInfo, WmtcmdBase,
        WmtcmdRenderSetbufferoffset,
    },
};
use winemetal_wire::MemPtr;

use super::*;
use crate::{
    deptrack::Access,
    encoder::{Target, stage},
    resource::{Texture, TextureAllocationFlags},
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
    // SAFETY: made-up handles; the fakes that see them (the shader cache tests' `NSObject_retain`
    // and `NSObject_release`, which log per thread) never dereference them.
    let device = unsafe { Obj::adopt(0xde) }.expect("a handle");
    // SAFETY: as above.
    let heap = unsafe { Obj::adopt(0x4e) }.expect("a handle");
    let info = WMTTextureInfo {
        pixel_format: WMT_PIXEL_FORMAT_RGBA8_UNORM,
        width: 64,
        height: 64,
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
