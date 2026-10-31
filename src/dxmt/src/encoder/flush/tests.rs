use core::ptr;

use winemetal_sys::wire::{
    SM50_BINDING_INDEX_ARGUMENT_TABLE, SM50_BINDING_INDEX_CONSTANT_BUFFER,
    SM50_BINDING_INDEX_VERTEX_BUFFER,
};

use super::*;

/// The fence operations of `chain`, each `(type, fence, stages)`, in list order.
fn fence_ops(chain: &Chain) -> Vec<(u16, u64, u8)> {
    let mut out = Vec::new();
    let mut at = chain.head;
    while let Some(record) = at {
        // SAFETY: a render fence operation the chain placed in the encoder's memory.
        let op = unsafe { record.cast::<WmtcmdRenderFenceOp>().as_ref() };
        out.push((op.r#type, op.fence, op.stages));
        at = NonNull::new(ptr::with_exposed_provenance_mut::<WmtcmdBase>(
            usize::try_from(op.next.ptr).expect("an address"),
        ));
    }
    out
}

/// A headless encoder whose fence pool holds made-up handles `0xf000 + slot`.
fn encoder() -> Encoder {
    let mut encoder = Encoder::headless();
    encoder.fences = (0..256)
        // SAFETY: made-up handles, which only the per-thread `NSObject_release` fake sees.
        .map(|slot| unsafe { Obj::adopt(0xf000 + slot) })
        .collect();
    encoder
}

#[test]
fn render_fences_split_by_half() {
    const PRE_RASTER: u8 = WMT_RENDER_STAGE_PRE_RASTER;
    const FRAGMENT: u8 = WMT_RENDER_STAGE_FRAGMENT;
    const WAIT: u16 = WMT_RENDER_COMMAND_WAIT_FOR_FENCE;
    const UPDATE: u16 = WMT_RENDER_COMMAND_UPDATE_FENCE;
    let mut encoder = encoder();
    encoder.start_render_pass(0, 0, 0, None);
    let mut pass = encoder.pass.take().expect("a render pass");
    assert_eq!((pass.id_vertex, pass.id), (256, 257));
    for id in [3, 70] {
        pass.fence_wait_vertex.set(id);
    }
    for id in [3, 5, 200] {
        pass.fence_wait.set(id);
    }
    let waits = render_waits(&mut encoder.heap, &encoder.fences, &pass);
    assert_eq!(
        fence_ops(&waits),
        [
            (WAIT, 0xf003, PRE_RASTER),
            (WAIT, 0xf005, FRAGMENT),
            (WAIT, 0xf046, PRE_RASTER),
            (WAIT, 0xf0c8, FRAGMENT),
        ],
        "slot order; a fence the vertex half waits for, before pre-raster only"
    );
    let updates = render_updates(&mut encoder.heap, &encoder.fences, &pass);
    assert_eq!(
        fence_ops(&updates),
        [(UPDATE, 0xf000, PRE_RASTER), (UPDATE, 0xf001, FRAGMENT)],
        "the vertex half's fence after pre-raster, the pass's after the fragment stage"
    );
}

#[test]
fn render_pass_info_as_cpp() {
    let info = initial_render_pass_info();
    for color in &info.colors {
        assert_eq!(
            (color.texture, color.load_action, color.store_action),
            (0, 0, WMT_STORE_ACTION_STORE)
        );
        let c = color.clear_color;
        assert_eq!([c.r, c.g, c.b, c.a], [0.0, 0.0, 0.0, 1.0]);
    }
    assert_eq!(
        (info.depth.load_action, info.depth.clear_depth),
        (WMT_LOAD_ACTION_CLEAR, 1.0)
    );
    assert_eq!(info.stencil.load_action, WMT_LOAD_ACTION_CLEAR);
    assert_eq!(low_byte(0x1_02), 2, "a uint8_t keeps the low byte");
    let set: Vec<_> = render_bindings(false, RenderUses::empty()).collect();
    assert_eq!(
        set,
        [
            (
                WMT_RENDER_COMMAND_SET_VERTEX_BUFFER,
                SM50_BINDING_INDEX_VERTEX_BUFFER
            ),
            (
                WMT_RENDER_COMMAND_SET_VERTEX_BUFFER,
                SM50_BINDING_INDEX_CONSTANT_BUFFER
            ),
            (
                WMT_RENDER_COMMAND_SET_VERTEX_BUFFER,
                SM50_BINDING_INDEX_ARGUMENT_TABLE
            ),
            (
                WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
                SM50_BINDING_INDEX_CONSTANT_BUFFER
            ),
            (
                WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
                SM50_BINDING_INDEX_ARGUMENT_TABLE
            ),
        ]
    );
    assert_eq!(
        render_bindings(true, RenderUses::TESSELLATION).count(),
        14,
        "every binding for a tessellated pass with an emulated stream output"
    );
    assert_eq!(render_bindings(false, RenderUses::GEOMETRY).count(), 11);
}
