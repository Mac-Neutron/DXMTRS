use core::{ffi::c_void, fmt::Write as _, mem::ManuallyDrop, ptr};
use std::sync::Arc;

use dxmt_core::{
    dynamic::DynamicBuffer,
    encoder::{NO_NEXT, Pass},
    resource::{Buffer, BufferAllocationFlags},
    staging::Staging,
};
use winemetal_sys::{
    Obj,
    wire::{
        WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_NOP,
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET, WMT_RENDER_COMMAND_USE_RESOURCE,
        WMT_RENDER_STAGE_FRAGMENT, WMT_RESOURCE_USAGE_READ, WmtcmdBase, WmtcmdRenderDraw,
        WmtcmdRenderSetbufferoffset, WmtcmdRenderUseresource,
    },
};

use super::*;
use crate::{
    abi::records::D3d11MappedSubresource,
    com::{E_NOTIMPL, S_OK},
    context::{Context, Immediate},
    resource::Common,
};

fn format_render_pass(pass: &Pass) -> String {
    let mut out = String::new();
    let mut cur = Some(pass.records());
    while let Some(node) = cur {
        // SAFETY: `node` is a valid `WmtcmdBase` record in the pass's arena.
        let base = unsafe { node.as_ref() };
        match base.r#type {
            WMT_RENDER_COMMAND_NOP => {
                out.push_str("render 0\n");
            }
            WMT_RENDER_COMMAND_USE_RESOURCE => {
                // SAFETY: `base.r#type` matched `WMT_RENDER_COMMAND_USE_RESOURCE`.
                let u = unsafe { node.cast::<WmtcmdRenderUseresource>().as_ref() };
                let res = u.resource;
                let usage = u.usage;
                let stages = u.stages;
                let _ = writeln!(
                    out,
                    "render 1 resource=#{res} usage={usage} stages={stages}"
                );
            }
            WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET => {
                // SAFETY: `base.r#type` matched `WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET`.
                let o = unsafe { node.cast::<WmtcmdRenderSetbufferoffset>().as_ref() };
                let offset = o.offset;
                let index = o.index;
                let _ = writeln!(out, "render 5 offset={offset} index={index}");
            }
            WMT_RENDER_COMMAND_DRAW => {
                // SAFETY: `base.r#type` matched `WMT_RENDER_COMMAND_DRAW`.
                let d = unsafe { node.cast::<WmtcmdRenderDraw>().as_ref() };
                let pt = d.primitive_type;
                let vs = d.vertex_start;
                let vc = d.vertex_count;
                let ic = d.instance_count;
                let bi = d.base_instance;
                let _ = writeln!(
                    out,
                    "render 19 primitive_type={pt} vertex_start={vs} vertex_count={vc} instance_count={ic} base_instance={bi}"
                );
            }
            other => {
                let _ = writeln!(out, "render {other}");
            }
        }
        let next_addr = usize::try_from(base.next.ptr).unwrap_or(0);
        cur = core::ptr::NonNull::new(core::ptr::with_exposed_provenance_mut::<WmtcmdBase>(
            next_addr,
        ));
    }
    out
}

fn fake_device() -> ManuallyDrop<Obj> {
    // SAFETY: Non-zero sentinel handle wrapped in `ManuallyDrop` so `Obj_release` never runs.
    ManuallyDrop::new(unsafe { Obj::adopt(1) }.expect("non-zero"))
}

const fn zero_mapped() -> D3d11MappedSubresource {
    D3d11MappedSubresource {
        p_data: ptr::null_mut(),
        row_pitch: 0,
        depth_pitch: 0,
    }
}

#[test]
fn do_not_wait_on_busy_answers_still_drawing() {
    let dev = fake_device();
    let staging = Arc::new(Staging::new(&dev, 128, 128, 128));
    staging.use_copy_destination(1);

    let mut common = Common {
        staging: Some(Arc::clone(&staging)),
        usage: 3,
        ..Common::default()
    };
    let res_ptr = (&raw mut common).cast::<c_void>();

    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.mode.has_dirty_op = true;

    let mut mapped = zero_mapped();
    let hr = imm.map(
        res_ptr,
        0,
        D3D11_MAP_READ,
        D3D11_MAP_FLAG_DO_NOT_WAIT,
        (&raw mut mapped).cast(),
    );
    assert_eq!(hr, DXGI_ERROR_WAS_STILL_DRAWING);
    assert!(mapped.p_data.is_null());
    // Mapping with DO_NOT_WAIT on the active sequence flushes the pending work.
    assert_eq!(imm.mode.commit_count, 1);
}

#[test]
fn ignore_no_wait_waits() {
    let dev = fake_device();
    let staging = Arc::new(Staging::new(&dev, 128, 128, 128));
    staging.use_copy_destination(1);

    let mut common = Common {
        staging: Some(Arc::clone(&staging)),
        usage: 3,
        ..Common::default()
    };
    let res_ptr = (&raw mut common).cast::<c_void>();

    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.mode.ignore_no_wait = true;
    imm.mode.has_dirty_op = true;

    let mut mapped = zero_mapped();
    let hr = imm.map(
        res_ptr,
        0,
        D3D11_MAP_READ,
        D3D11_MAP_FLAG_DO_NOT_WAIT,
        (&raw mut mapped).cast(),
    );
    assert_eq!(hr, S_OK);
    assert!(!mapped.p_data.is_null());
    assert_eq!(mapped.row_pitch, 128);
    assert_eq!(imm.mode.coherent_seq(), 1);
}

#[test]
fn discard_renames_on_the_caller() {
    let dev = fake_device();
    let buffer = Arc::new(Buffer::new(256, &dev));
    let flags = BufferAllocationFlags::CPU_PLACED;
    let first = buffer.allocate(flags);
    buffer.rename(first);
    let dyn_buf = Arc::new(DynamicBuffer::new(buffer, flags).expect("dynamic buffer"));
    let initial_name = dyn_buf.immediate_name();

    let mut common = Common {
        dynamic_buffer: Some(Arc::clone(&dyn_buf)),
        bind_flags: 0x1, // D3D11_BIND_VERTEX_BUFFER
        usage: 2,        // D3D11_USAGE_DYNAMIC
        ..Common::default()
    };
    let res_ptr = (&raw mut common).cast::<c_void>();

    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.begin_render_pass();

    // First Map(DISCARD) + Draw(3, 0).
    let mut mapped1 = zero_mapped();
    assert_eq!(
        imm.map(
            res_ptr,
            0,
            D3D11_MAP_WRITE_DISCARD,
            0,
            (&raw mut mapped1).cast()
        ),
        S_OK
    );
    assert!(!mapped1.p_data.is_null());
    let name_after_discard1 = dyn_buf.immediate_name();
    assert!(!Arc::ptr_eq(&initial_name, &name_after_discard1));
    imm.unmap(res_ptr, 0);

    Immediate::emit_op(&mut imm, |enc| {
        enc.use_render_resource(69, WMT_RESOURCE_USAGE_READ, WMT_RENDER_STAGE_FRAGMENT);
        enc.encode_render_command(WmtcmdRenderSetbufferoffset {
            r#type: WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
            reserved: [0; 3],
            next: NO_NEXT,
            offset: 0,
            index: 0,
        });
        enc.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        });
    });

    // Second Map(DISCARD) + Draw(3, 0).
    let mut mapped2 = zero_mapped();
    assert_eq!(
        imm.map(
            res_ptr,
            0,
            D3D11_MAP_WRITE_DISCARD,
            0,
            (&raw mut mapped2).cast()
        ),
        S_OK
    );
    let name_after_discard2 = dyn_buf.immediate_name();
    assert!(!Arc::ptr_eq(&name_after_discard1, &name_after_discard2));
    imm.unmap(res_ptr, 0);

    Immediate::emit_op(&mut imm, |enc| {
        enc.encode_render_command(WmtcmdRenderSetbufferoffset {
            r#type: WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
            reserved: [0; 3],
            next: NO_NEXT,
            offset: 32,
            index: 0,
        });
        enc.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        });
    });

    let expected_golden = include_str!("golden/d3d11_dynamic.txt");
    let pass = imm.mode.encoder_mut().pass().expect("render pass active");
    let formatted = format_render_pass(pass);
    assert_eq!(formatted, expected_golden);
}

#[test]
fn default_map_is_not_implemented() {
    let dev = fake_device();
    let mut common = Common {
        buffer: Some(Arc::new(Buffer::new(64, &dev))),
        usage: 0, // D3D11_USAGE_DEFAULT
        ..Common::default()
    };
    let res_ptr = (&raw mut common).cast::<c_void>();

    let mut imm = Context::new_immediate(ptr::null_mut());
    let mut mapped = zero_mapped();
    let hr = imm.map(res_ptr, 0, D3D11_MAP_READ, 0, (&raw mut mapped).cast());
    assert_eq!(hr, E_NOTIMPL);
}
