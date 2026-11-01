use winemetal_wire::{
    MemPtr, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, WMT_BLIT_COMMAND_COPY_TEXTURE,
    WMT_BLIT_COMMAND_FILL_BUFFER, WMT_BLIT_COMMAND_GENERATE_MIPMAPS, WMT_BLIT_COMMAND_NOP,
    WMT_BLIT_COMMAND_RESET_COMMANDS_IN_BUFFER, WMT_BLIT_COMMAND_RESOLVE_COUNTERS,
    WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE, WMT_COMPUTE_COMMAND_DISPATCH,
    WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT, WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
    WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_COMPUTE_COMMAND_MEMORY_BARRIER,
    WMT_COMPUTE_COMMAND_NOP, WMT_COMPUTE_COMMAND_SET_BUFFER, WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET,
    WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO, WMT_COMPUTE_COMMAND_SET_TEXTURE,
    WMT_COMPUTE_COMMAND_UPDATE_FENCE, WMT_COMPUTE_COMMAND_USE_RESOURCE,
    WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE, WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE,
    WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT, WMT_RENDER_COMMAND_DRAW_INDIRECT,
    WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS, WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT, WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
    WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_RENDER_COMMAND_MEMORY_BARRIER,
    WMT_RENDER_COMMAND_NOP, WMT_RENDER_COMMAND_SET_BLEND_FACTOR,
    WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
    WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE, WMT_RENDER_COMMAND_SET_DSSO,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE,
    WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE, WMT_RENDER_COMMAND_SET_MESH_BUFFER,
    WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER,
    WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_PSO,
    WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
    WMT_RENDER_COMMAND_SET_SCISSOR_RECTS, WMT_RENDER_COMMAND_SET_STENCIL_REF,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER, WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_VERTEX_BYTES, WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE,
    WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE, WMT_RENDER_COMMAND_SET_VIEWPORT,
    WMT_RENDER_COMMAND_SET_VIEWPORTS, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
    WMT_RENDER_COMMAND_UPDATE_FENCE, WMT_RENDER_COMMAND_USE_RESOURCE,
    WMT_RENDER_COMMAND_WAIT_FOR_FENCE, WMTOrigin, WMTScissorRect, WMTSize, WMTViewport,
    WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitCopyFromBufferToTexture,
    WmtcmdBlitCopyFromBufferToTextureWithblitoption, WmtcmdBlitCopyFromTextureToBuffer,
    WmtcmdBlitCopyFromTextureToBufferWithblitoption, WmtcmdBlitCopyFromTextureToTexture,
    WmtcmdBlitCopyTexture, WmtcmdBlitFenceOp, WmtcmdBlitFillbuffer, WmtcmdBlitGenerateMipmaps,
    WmtcmdBlitNop, WmtcmdBlitResetcommands, WmtcmdBlitResolvecounters, WmtcmdComputeDispatch,
    WmtcmdComputeDispatchIndirect, WmtcmdComputeExecutecommands, WmtcmdComputeFenceOp,
    WmtcmdComputeMemoryBarrier, WmtcmdComputeNop, WmtcmdComputeSetbuffer,
    WmtcmdComputeSetbufferoffset, WmtcmdComputeSetbytes, WmtcmdComputeSetpso,
    WmtcmdComputeSettexture, WmtcmdComputeUseresource, WmtcmdRenderDispatchThreadsPerTile,
    WmtcmdRenderDraw, WmtcmdRenderDrawIndexed, WmtcmdRenderDrawIndexedIndirect,
    WmtcmdRenderDrawIndirect, WmtcmdRenderDrawMeshthreadgroups,
    WmtcmdRenderDrawMeshthreadgroupsIndirect, WmtcmdRenderDxmtGeometryDraw,
    WmtcmdRenderDxmtGeometryDrawIndexed, WmtcmdRenderDxmtGeometryDrawIndexedIndirect,
    WmtcmdRenderDxmtGeometryDrawIndirect, WmtcmdRenderDxmtTessellationMeshDraw,
    WmtcmdRenderDxmtTessellationMeshDrawIndexed,
    WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect,
    WmtcmdRenderDxmtTessellationMeshDrawIndirect, WmtcmdRenderExecutecommands, WmtcmdRenderFenceOp,
    WmtcmdRenderMemoryBarrier, WmtcmdRenderNop, WmtcmdRenderSetblendcolor, WmtcmdRenderSetbuffer,
    WmtcmdRenderSetbufferoffset, WmtcmdRenderSetbytes, WmtcmdRenderSetdepthstencilstate,
    WmtcmdRenderSetdsso, WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate,
    WmtcmdRenderSetsampler, WmtcmdRenderSetscissorrect, WmtcmdRenderSetscissorrects,
    WmtcmdRenderSetstencilref, WmtcmdRenderSettexture, WmtcmdRenderSetviewport,
    WmtcmdRenderSetviewports, WmtcmdRenderSetvisibilitymode, WmtcmdRenderUseresource,
};

use super::*;

const NULL: MemPtr = MemPtr { ptr: 0 };
const RESERVED: [u16; 3] = [0; 3];

/// A list of `kind` that walked `records`.
fn walked(kind: Kind, records: &[u16]) -> List {
    let mut list = List::new(kind);
    for &record in records {
        list.record(record);
    }
    list
}

fn text(histogram: &Histogram<Vec<u8>>) -> &str {
    core::str::from_utf8(&histogram.out).expect("the lines are text")
}

fn dump_text(dump: &Dump<Vec<u8>>) -> &str {
    core::str::from_utf8(&dump.out).expect("the lines are text")
}

fn dump_node<T>(dump: &mut Dump<Vec<u8>>, kind: &Kind, node: &T) {
    let head = MemPtr {
        ptr: core::ptr::from_ref(node) as usize as u64,
    };
    // SAFETY: `head` points to `node`, a single-record chain (`next.ptr == 0`) with valid fields.
    unsafe { dump.chain(kind, head) };
}

#[test]
fn lines_count_since_the_last_commit() {
    let mut histogram = Histogram::new(Vec::new());
    histogram.add(walked(Kind::Render, &[0, 5, 5, 9]));
    histogram.add(walked(Kind::Blit, &[2]));
    histogram.add(walked(Kind::Render, &[5, 7]));
    histogram.commit();
    assert_eq!(
        text(&histogram),
        "render 0 1\nrender 5 3\nrender 7 1\nrender 9 1\nblit 2 1\n"
    );
    histogram.add(walked(Kind::Compute, &[1, 1]));
    histogram.commit();
    assert_eq!(
        text(&histogram),
        "render 0 1\nrender 5 3\nrender 7 1\nrender 9 1\nblit 2 1\ncompute 1 2\n"
    );
}

#[test]
fn a_commit_with_nothing_counted_writes_nothing() {
    let mut histogram = Histogram::new(Vec::new());
    histogram.commit();
    histogram.add(walked(Kind::Compute, &[]));
    histogram.commit();
    assert_eq!(text(&histogram), "");
}

#[test]
fn kinds_are_written_render_compute_blit() {
    let mut histogram = Histogram::new(Vec::new());
    histogram.add(walked(Kind::Blit, &[1]));
    histogram.add(walked(Kind::Compute, &[1]));
    histogram.add(walked(Kind::Render, &[1]));
    histogram.commit();
    assert_eq!(text(&histogram), "render 1 1\ncompute 1 1\nblit 1 1\n");
}

#[test]
fn handles_renumber_by_first_use() {
    let mut dump = Dump::new(Vec::new());
    let copy1 = WmtcmdBlitCopyFromBufferToBuffer {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
        reserved: RESERVED,
        next: NULL,
        src: 0xdead_0010,
        src_offset: 4,
        dst: 0xdead_0020,
        dst_offset: 8,
        copy_length: 16,
    };
    let copy2 = WmtcmdBlitCopyFromBufferToBuffer {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
        reserved: RESERVED,
        next: NULL,
        src: 0xdead_0020,
        src_offset: 0,
        dst: 0,
        dst_offset: 0,
        copy_length: 32,
    };
    let mip = WmtcmdBlitGenerateMipmaps {
        r#type: WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
        reserved: RESERVED,
        next: NULL,
        texture: 0xdead_0030,
    };
    dump_node(&mut dump, &Kind::Blit, &copy1);
    dump_node(&mut dump, &Kind::Blit, &copy2);
    dump_node(&mut dump, &Kind::Blit, &mip);
    assert_eq!(
        dump_text(&dump),
        concat!(
            "blit 1 src=#1 src_offset=4 dst=#2 dst_offset=8 copy_length=16\n",
            "blit 1 src=#2 src_offset=0 dst=0 dst_offset=0 copy_length=32\n",
            "blit 5 texture=#3\n",
        )
    );
}

#[test]
fn gpu_addresses_as_buffer_and_offset() {
    let mut dump = Dump::new(Vec::new());
    dump.buffer(0x5000_0000, 0x100);
    dump.buffer(0x6000_0000, 0x200);
    let words: [u64; 4] = [0x5000_0000, 0x6000_01fc, 0x6000_0200, 0];
    let set_bytes = WmtcmdComputeSetbytes {
        r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
        reserved: RESERVED,
        next: NULL,
        bytes: MemPtr {
            ptr: words.as_ptr() as usize as u64,
        },
        length: 32,
        index: 2,
    };
    dump_node(&mut dump, &Kind::Compute, &set_bytes);
    assert_eq!(
        dump_text(&dump),
        "compute 7 bytes=32B length=32 index=2 words=[buf#1+0,buf#2+508,0x60000200,0x0]\n"
    );
}

#[test]
fn dump_lists_every_field() {
    let mut dump = Dump::new(Vec::new());
    dump.buffer(0x1000, 0x100);
    dump.buffer(0x2000, 0x200);
    let bytes: [u64; 4] = [0x1000, 0x2004, 0xdead_beef, 7];
    let viewports = [WMTViewport {
        origin_x: 0.0,
        origin_y: 1.0,
        width: 640.0,
        height: 480.0,
        znear: 0.0,
        zfar: 1.0,
    }];
    let rects = [WMTScissorRect {
        x: 1,
        y: 2,
        width: 640,
        height: 480,
    }];

    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitNop {
            r#type: WMT_BLIT_COMMAND_NOP,
            reserved: RESERVED,
            next: NULL,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitCopyFromBufferToBuffer {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
            reserved: RESERVED,
            next: NULL,
            src: 100,
            src_offset: 16,
            dst: 200,
            dst_offset: 32,
            copy_length: 64,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitCopyFromBufferToTexture {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
            reserved: RESERVED,
            next: NULL,
            src: 100,
            src_offset: 64,
            bytes_per_row: 256,
            bytes_per_image: 1024,
            size: WMTSize {
                width: 4,
                height: 8,
                depth: 2,
            },
            dst: 200,
            slice: 1,
            level: 2,
            origin: WMTOrigin { x: 3, y: 5, z: 7 },
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitCopyFromTextureToBuffer {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
            reserved: RESERVED,
            next: NULL,
            src: 100,
            slice: 1,
            level: 2,
            origin: WMTOrigin { x: 3, y: 4, z: 5 },
            size: WMTSize {
                width: 6,
                height: 7,
                depth: 8,
            },
            dst: 200,
            offset: 128,
            bytes_per_row: 512,
            bytes_per_image: 2048,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitCopyFromTextureToTexture {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
            reserved: RESERVED,
            next: NULL,
            src: 100,
            src_slice: 1,
            src_level: 2,
            src_origin: WMTOrigin { x: 3, y: 4, z: 5 },
            src_size: WMTSize {
                width: 6,
                height: 7,
                depth: 8,
            },
            dst: 200,
            dst_slice: 9,
            dst_level: 10,
            dst_origin: WMTOrigin {
                x: 11,
                y: 12,
                z: 13,
            },
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitGenerateMipmaps {
            r#type: WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
            reserved: RESERVED,
            next: NULL,
            texture: 100,
        },
    );
    for r#type in [
        WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
        WMT_BLIT_COMMAND_UPDATE_FENCE,
    ] {
        dump_node(
            &mut dump,
            &Kind::Blit,
            &WmtcmdBlitFenceOp {
                r#type,
                reserved: RESERVED,
                next: NULL,
                fence: 100,
            },
        );
    }
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitFillbuffer {
            r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
            reserved: RESERVED,
            next: NULL,
            buffer: 100,
            offset: 16,
            length: 32,
            value: 255,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitResolvecounters {
            r#type: WMT_BLIT_COMMAND_RESOLVE_COUNTERS,
            reserved: RESERVED,
            next: NULL,
            sample_buffer: 100,
            start: 2,
            len: 4,
            dst_buffer: 200,
            dst_offset: 64,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitCopyFromBufferToTextureWithblitoption {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
            reserved: RESERVED,
            next: NULL,
            src: 100,
            src_offset: 16,
            bytes_per_row: 64,
            bytes_per_image: 256,
            size: WMTSize {
                width: 2,
                height: 3,
                depth: 4,
            },
            dst: 200,
            slice: 5,
            level: 6,
            options: 7,
            origin: WMTOrigin { x: 8, y: 9, z: 10 },
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitCopyFromTextureToBufferWithblitoption {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
            reserved: RESERVED,
            next: NULL,
            src: 100,
            slice: 2,
            level: 3,
            options: 4,
            origin: WMTOrigin { x: 5, y: 6, z: 7 },
            size: WMTSize {
                width: 8,
                height: 9,
                depth: 10,
            },
            dst: 200,
            offset: 32,
            bytes_per_row: 128,
            bytes_per_image: 512,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitResetcommands {
            r#type: WMT_BLIT_COMMAND_RESET_COMMANDS_IN_BUFFER,
            reserved: RESERVED,
            next: NULL,
            indirect_command_buffer: 100,
            location: 4,
            length: 8,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Blit,
        &WmtcmdBlitCopyTexture {
            r#type: WMT_BLIT_COMMAND_COPY_TEXTURE,
            reserved: RESERVED,
            next: NULL,
            src: 100,
            dst: 200,
        },
    );

    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeNop {
            r#type: WMT_COMPUTE_COMMAND_NOP,
            reserved: RESERVED,
            next: NULL,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH,
            reserved: RESERVED,
            next: NULL,
            size: WMTSize {
                width: 2,
                height: 4,
                depth: 8,
            },
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeDispatchIndirect {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            indirect_args_buffer: 100,
            indirect_args_offset: 64,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: RESERVED,
            next: NULL,
            pso: 100,
            threadgroup_size: WMTSize {
                width: 8,
                height: 8,
                depth: 1,
            },
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeSetbuffer {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
            reserved: RESERVED,
            next: NULL,
            buffer: 100,
            offset: 32,
            index: 3,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeSetbufferoffset {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET,
            reserved: RESERVED,
            next: NULL,
            offset: 48,
            index: 3,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeUseresource {
            r#type: WMT_COMPUTE_COMMAND_USE_RESOURCE,
            reserved: RESERVED,
            next: NULL,
            resource: 100,
            usage: 3,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeSetbytes {
            r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
            reserved: RESERVED,
            next: NULL,
            bytes: MemPtr {
                ptr: bytes.as_ptr() as usize as u64,
            },
            length: 32,
            index: 0,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: RESERVED,
            next: NULL,
            texture: 100,
            index: 5,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
            reserved: RESERVED,
            next: NULL,
            size: WMTSize {
                width: 16,
                height: 32,
                depth: 1,
            },
        },
    );
    for r#type in [
        WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE,
        WMT_COMPUTE_COMMAND_UPDATE_FENCE,
    ] {
        dump_node(
            &mut dump,
            &Kind::Compute,
            &WmtcmdComputeFenceOp {
                r#type,
                reserved: RESERVED,
                next: NULL,
                fence: 100,
            },
        );
    }
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeMemoryBarrier {
            r#type: WMT_COMPUTE_COMMAND_MEMORY_BARRIER,
            reserved: RESERVED,
            next: NULL,
            scope: 3,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Compute,
        &WmtcmdComputeExecutecommands {
            r#type: WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER,
            reserved: RESERVED,
            next: NULL,
            indirect_command_buffer: 100,
            location: 2,
            length: 6,
        },
    );

    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderNop {
            r#type: WMT_RENDER_COMMAND_NOP,
            reserved: RESERVED,
            next: NULL,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderUseresource {
            r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
            reserved: RESERVED,
            next: NULL,
            resource: 100,
            usage: 1,
            stages: 3,
        },
    );
    for r#type in [
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER,
        WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
        WMT_RENDER_COMMAND_SET_MESH_BUFFER,
        WMT_RENDER_COMMAND_SET_OBJECT_BUFFER,
    ] {
        dump_node(
            &mut dump,
            &Kind::Render,
            &WmtcmdRenderSetbuffer {
                r#type,
                reserved: RESERVED,
                next: NULL,
                buffer: 100,
                offset: 16,
                index: 2,
            },
        );
    }
    for r#type in [
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
        WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
        WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET,
        WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET,
    ] {
        dump_node(
            &mut dump,
            &Kind::Render,
            &WmtcmdRenderSetbufferoffset {
                r#type,
                reserved: RESERVED,
                next: NULL,
                offset: 24,
                index: 2,
            },
        );
    }
    for r#type in [
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
        WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE,
    ] {
        dump_node(
            &mut dump,
            &Kind::Render,
            &WmtcmdRenderSettexture {
                r#type,
                reserved: RESERVED,
                next: NULL,
                texture: 100,
                index: 4,
            },
        );
    }
    for r#type in [
        WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
        WMT_RENDER_COMMAND_SET_VERTEX_BYTES,
    ] {
        dump_node(
            &mut dump,
            &Kind::Render,
            &WmtcmdRenderSetbytes {
                r#type,
                reserved: RESERVED,
                next: NULL,
                bytes: MemPtr {
                    ptr: bytes.as_ptr() as usize as u64,
                },
                length: 32,
                index: 1,
            },
        );
    }
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetrasterizerstate {
            r#type: WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
            reserved: RESERVED,
            next: NULL,
            fill_mode: 0,
            cull_mode: 2,
            depth_clip_mode: 1,
            winding: 1,
            depth_bias: 1.0,
            scole_scale: 2.0,
            depth_bias_clamp: 0.5,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetviewports {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
            reserved: RESERVED,
            next: NULL,
            viewports: MemPtr {
                ptr: viewports.as_ptr() as usize as u64,
            },
            viewport_count: 1,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetscissorrects {
            r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
            reserved: RESERVED,
            next: NULL,
            scissor_rects: MemPtr {
                ptr: rects.as_ptr() as usize as u64,
            },
            rect_count: 1,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: RESERVED,
            next: NULL,
            pso: 100,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetdsso {
            r#type: WMT_RENDER_COMMAND_SET_DSSO,
            reserved: RESERVED,
            next: NULL,
            dsso: 100,
            stencil_ref: 7,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetblendcolor {
            r#type: WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
            reserved: RESERVED,
            next: NULL,
            red: 0.25,
            green: 0.5,
            blue: 0.75,
            alpha: 1.0,
            stencil_ref: 9,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetvisibilitymode {
            r#type: WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
            reserved: RESERVED,
            next: NULL,
            offset: 64,
            mode: 1,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: RESERVED,
            next: NULL,
            primitive_type: 3,
            vertex_start: 0,
            vertex_count: 6,
            instance_count: 2,
            base_instance: 1,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDrawIndexed {
            r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
            reserved: RESERVED,
            next: NULL,
            primitive_type: 3,
            index_type: 1,
            index_count: 12,
            index_buffer: 100,
            index_buffer_offset: 32,
            instance_count: 2,
            base_vertex: -1,
            base_instance: 4,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDrawIndirect {
            r#type: WMT_RENDER_COMMAND_DRAW_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            primitive_type: 3,
            indirect_args_buffer: 100,
            indirect_args_offset: 16,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDrawIndexedIndirect {
            r#type: WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            primitive_type: 3,
            index_type: 1,
            index_buffer: 100,
            index_buffer_offset: 8,
            indirect_args_buffer: 200,
            indirect_args_offset: 16,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDrawMeshthreadgroups {
            r#type: WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS,
            reserved: RESERVED,
            next: NULL,
            threadgroup_per_grid: WMTSize {
                width: 1,
                height: 2,
                depth: 3,
            },
            object_threadgroup_size: WMTSize {
                width: 4,
                height: 5,
                depth: 6,
            },
            mesh_threadgroup_size: WMTSize {
                width: 7,
                height: 8,
                depth: 9,
            },
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDrawMeshthreadgroupsIndirect {
            r#type: WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            indirect_args_buffer: 100,
            indirect_args_offset: 32,
            object_threadgroup_size: WMTSize {
                width: 4,
                height: 5,
                depth: 6,
            },
            mesh_threadgroup_size: WMTSize {
                width: 7,
                height: 8,
                depth: 9,
            },
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderMemoryBarrier {
            r#type: WMT_RENDER_COMMAND_MEMORY_BARRIER,
            reserved: RESERVED,
            next: NULL,
            scope: 1,
            stages_before: 2,
            stages_after: 4,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtGeometryDraw {
            r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW,
            reserved: RESERVED,
            next: NULL,
            draw_arguments_offset: 16,
            warp_count: 4,
            instance_count: 2,
            vertex_per_warp: 32,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtGeometryDrawIndexed {
            r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
            reserved: RESERVED,
            next: NULL,
            draw_arguments_offset: 16,
            index_buffer: 100,
            index_buffer_offset: 32,
            warp_count: 4,
            instance_count: 2,
            vertex_per_warp: 32,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtGeometryDrawIndirect {
            r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            imm_draw_arguments: 100,
            indirect_args_buffer: 200,
            indirect_args_offset: 16,
            dispatch_args_buffer: 100,
            dispatch_args_offset: 32,
            vertex_per_warp: 16,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtGeometryDrawIndexedIndirect {
            r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            index_buffer: 100,
            index_buffer_offset: 8,
            imm_draw_arguments: 200,
            indirect_args_buffer: 100,
            indirect_args_offset: 16,
            dispatch_args_buffer: 200,
            dispatch_args_offset: 32,
            vertex_per_warp: 16,
        },
    );
    for r#type in [
        WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
        WMT_RENDER_COMMAND_UPDATE_FENCE,
    ] {
        dump_node(
            &mut dump,
            &Kind::Render,
            &WmtcmdRenderFenceOp {
                r#type,
                reserved: RESERVED,
                next: NULL,
                fence: 100,
                stages: 3,
            },
        );
    }
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetviewport {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
            reserved: RESERVED,
            next: NULL,
            viewport: viewports[0],
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetscissorrect {
            r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
            reserved: RESERVED,
            next: NULL,
            scissor_rect: rects[0],
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtTessellationMeshDraw {
            r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
            reserved: RESERVED,
            next: NULL,
            draw_arguments_offset: 8,
            instance_count: 2,
            threads_per_patch: 4,
            patch_per_group: 8,
            patch_per_mesh_instance: 16,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtTessellationMeshDrawIndexed {
            r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
            reserved: RESERVED,
            next: NULL,
            draw_arguments_offset: 8,
            index_buffer: 100,
            index_buffer_offset: 16,
            instance_count: 2,
            threads_per_patch: 4,
            patch_per_group: 8,
            patch_per_mesh_instance: 16,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtTessellationMeshDrawIndirect {
            r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            imm_draw_arguments: 100,
            indirect_args_buffer: 200,
            indirect_args_offset: 16,
            dispatch_args_buffer: 100,
            dispatch_args_offset: 32,
            threads_per_patch: 4,
            patch_per_group: 8,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect {
            r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
            reserved: RESERVED,
            next: NULL,
            imm_draw_arguments: 100,
            indirect_args_buffer: 200,
            indirect_args_offset: 16,
            dispatch_args_buffer: 100,
            dispatch_args_offset: 32,
            index_buffer: 200,
            index_buffer_offset: 64,
            threads_per_patch: 4,
            patch_per_group: 8,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderDispatchThreadsPerTile {
            r#type: WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE,
            reserved: RESERVED,
            next: NULL,
            width: 16,
            height: 16,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderExecutecommands {
            r#type: WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER,
            reserved: RESERVED,
            next: NULL,
            indirect_command_buffer: 100,
            location: 4,
            length: 12,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetblendcolor {
            r#type: WMT_RENDER_COMMAND_SET_BLEND_FACTOR,
            reserved: RESERVED,
            next: NULL,
            red: 0.25,
            green: 0.5,
            blue: 0.75,
            alpha: 1.0,
            stencil_ref: 0,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetstencilref {
            r#type: WMT_RENDER_COMMAND_SET_STENCIL_REF,
            reserved: RESERVED,
            next: NULL,
            stencil_ref: 5,
        },
    );
    dump_node(
        &mut dump,
        &Kind::Render,
        &WmtcmdRenderSetdepthstencilstate {
            r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
            reserved: RESERVED,
            next: NULL,
            depth_stencil_state: 100,
        },
    );
    for r#type in [
        WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE,
        WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE,
    ] {
        dump_node(
            &mut dump,
            &Kind::Render,
            &WmtcmdRenderSetsampler {
                r#type,
                reserved: RESERVED,
                next: NULL,
                sampler: 100,
                index: 3,
            },
        );
    }

    let expected = [
        "blit 0",
        "blit 1 src=#1 src_offset=16 dst=#2 dst_offset=32 copy_length=64",
        "blit 2 src=#1 src_offset=64 bytes_per_row=256 bytes_per_image=1024 size.width=4 size.height=8 size.depth=2 dst=#2 slice=1 level=2 origin.x=3 origin.y=5 origin.z=7",
        "blit 3 src=#1 slice=1 level=2 origin.x=3 origin.y=4 origin.z=5 size.width=6 size.height=7 size.depth=8 dst=#2 offset=128 bytes_per_row=512 bytes_per_image=2048",
        "blit 4 src=#1 src_slice=1 src_level=2 src_origin.x=3 src_origin.y=4 src_origin.z=5 src_size.width=6 src_size.height=7 src_size.depth=8 dst=#2 dst_slice=9 dst_level=10 dst_origin.x=11 dst_origin.y=12 dst_origin.z=13",
        "blit 5 texture=#1",
        "blit 6 fence=#1",
        "blit 7 fence=#1",
        "blit 8 buffer=#1 offset=16 length=32 value=255",
        "blit 9 sample_buffer=#1 start=2 len=4 dst_buffer=#2 dst_offset=64",
        "blit 10 src=#1 src_offset=16 bytes_per_row=64 bytes_per_image=256 size.width=2 size.height=3 size.depth=4 dst=#2 slice=5 level=6 options=7 origin.x=8 origin.y=9 origin.z=10",
        "blit 11 src=#1 slice=2 level=3 options=4 origin.x=5 origin.y=6 origin.z=7 size.width=8 size.height=9 size.depth=10 dst=#2 offset=32 bytes_per_row=128 bytes_per_image=512",
        "blit 12 indirect_command_buffer=#1 location=4 length=8",
        "blit 13 src=#1 dst=#2",
        "compute 0",
        "compute 1 size.width=2 size.height=4 size.depth=8",
        "compute 2 indirect_args_buffer=#1 indirect_args_offset=64",
        "compute 3 pso=#1 threadgroup_size.width=8 threadgroup_size.height=8 threadgroup_size.depth=1",
        "compute 4 buffer=#1 offset=32 index=3",
        "compute 5 offset=48 index=3",
        "compute 6 resource=#1 usage=3",
        "compute 7 bytes=32B length=32 index=0 words=[buf#1+0,buf#2+4,0xdeadbeef,0x7]",
        "compute 8 texture=#1 index=5",
        "compute 9 size.width=16 size.height=32 size.depth=1",
        "compute 10 fence=#1",
        "compute 11 fence=#1",
        "compute 12 scope=3",
        "compute 13 indirect_command_buffer=#1 location=2 length=6",
        "render 0",
        "render 1 resource=#1 usage=1 stages=3",
        "render 2 buffer=#1 offset=16 index=2",
        "render 4 buffer=#1 offset=16 index=2",
        "render 6 buffer=#1 offset=16 index=2",
        "render 8 buffer=#1 offset=16 index=2",
        "render 3 offset=24 index=2",
        "render 5 offset=24 index=2",
        "render 7 offset=24 index=2",
        "render 9 offset=24 index=2",
        "render 10 texture=#1 index=4",
        "render 47 texture=#1 index=4",
        "render 11 bytes=32B length=32 index=1 words=[buf#1+0,buf#2+4,0xdeadbeef,0x7]",
        "render 46 bytes=32B length=32 index=1 words=[buf#1+0,buf#2+4,0xdeadbeef,0x7]",
        "render 12 fill_mode=0 cull_mode=2 depth_clip_mode=1 winding=1 depth_bias=0x3f800000 scole_scale=0x40000000 depth_bias_clamp=0x3f000000",
        "render 13 viewports=48B viewport_count=1 viewports[0].origin_x=0x00000000 viewports[0].origin_y=0x3f800000 viewports[0].width=0x44200000 viewports[0].height=0x43f00000 viewports[0].znear=0x00000000 viewports[0].zfar=0x3f800000",
        "render 14 scissor_rects=32B rect_count=1 scissor_rects[0].x=1 scissor_rects[0].y=2 scissor_rects[0].width=640 scissor_rects[0].height=480",
        "render 15 pso=#1",
        "render 16 dsso=#1 stencil_ref=7",
        "render 17 red=0x3e800000 green=0x3f000000 blue=0x3f400000 alpha=0x3f800000 stencil_ref=9",
        "render 18 offset=64 mode=1",
        "render 19 primitive_type=3 vertex_start=0 vertex_count=6 instance_count=2 base_instance=1",
        "render 20 primitive_type=3 index_type=1 index_count=12 index_buffer=#1 index_buffer_offset=32 instance_count=2 base_vertex=-1 base_instance=4",
        "render 21 primitive_type=3 indirect_args_buffer=#1 indirect_args_offset=16",
        "render 22 primitive_type=3 index_type=1 index_buffer=#1 index_buffer_offset=8 indirect_args_buffer=#2 indirect_args_offset=16",
        "render 23 threadgroup_per_grid.width=1 threadgroup_per_grid.height=2 threadgroup_per_grid.depth=3 object_threadgroup_size.width=4 object_threadgroup_size.height=5 object_threadgroup_size.depth=6 mesh_threadgroup_size.width=7 mesh_threadgroup_size.height=8 mesh_threadgroup_size.depth=9",
        "render 24 indirect_args_buffer=#1 indirect_args_offset=32 object_threadgroup_size.width=4 object_threadgroup_size.height=5 object_threadgroup_size.depth=6 mesh_threadgroup_size.width=7 mesh_threadgroup_size.height=8 mesh_threadgroup_size.depth=9",
        "render 25 scope=1 stages_before=2 stages_after=4",
        "render 29 draw_arguments_offset=16 warp_count=4 instance_count=2 vertex_per_warp=32",
        "render 30 draw_arguments_offset=16 index_buffer=#1 index_buffer_offset=32 warp_count=4 instance_count=2 vertex_per_warp=32",
        "render 31 imm_draw_arguments=#1 indirect_args_buffer=#2 indirect_args_offset=16 dispatch_args_buffer=#1 dispatch_args_offset=32 vertex_per_warp=16",
        "render 32 index_buffer=#1 index_buffer_offset=8 imm_draw_arguments=#2 indirect_args_buffer=#1 indirect_args_offset=16 dispatch_args_buffer=#2 dispatch_args_offset=32 vertex_per_warp=16",
        "render 33 fence=#1 stages=3",
        "render 34 fence=#1 stages=3",
        "render 35 viewport.origin_x=0x00000000 viewport.origin_y=0x3f800000 viewport.width=0x44200000 viewport.height=0x43f00000 viewport.znear=0x00000000 viewport.zfar=0x3f800000",
        "render 36 scissor_rect.x=1 scissor_rect.y=2 scissor_rect.width=640 scissor_rect.height=480",
        "render 37 draw_arguments_offset=8 instance_count=2 threads_per_patch=4 patch_per_group=8 patch_per_mesh_instance=16",
        "render 38 draw_arguments_offset=8 index_buffer=#1 index_buffer_offset=16 instance_count=2 threads_per_patch=4 patch_per_group=8 patch_per_mesh_instance=16",
        "render 39 imm_draw_arguments=#1 indirect_args_buffer=#2 indirect_args_offset=16 dispatch_args_buffer=#1 dispatch_args_offset=32 threads_per_patch=4 patch_per_group=8",
        "render 40 imm_draw_arguments=#1 indirect_args_buffer=#2 indirect_args_offset=16 dispatch_args_buffer=#1 dispatch_args_offset=32 index_buffer=#2 index_buffer_offset=64 threads_per_patch=4 patch_per_group=8",
        "render 41 width=16 height=16",
        "render 42 indirect_command_buffer=#1 location=4 length=12",
        "render 43 red=0x3e800000 green=0x3f000000 blue=0x3f400000 alpha=0x3f800000",
        "render 44 stencil_ref=5",
        "render 45 depth_stencil_state=#1",
        "render 48 sampler=#1 index=3",
        "render 49 sampler=#1 index=3",
    ];
    let lines: Vec<&str> = dump_text(&dump).lines().collect();
    assert_eq!(lines, expected);
}
