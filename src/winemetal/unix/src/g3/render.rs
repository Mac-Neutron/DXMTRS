//! Slot 38: a PE render command list on a render command encoder.

use core::ffi::c_void;

use objc2::{extern_methods, runtime::ProtocolObject};
use objc2_foundation::NSRange;
use objc2_metal::{
    MTLBarrierScope, MTLBuffer, MTLCullMode, MTLDepthClipMode, MTLFence, MTLIndexType,
    MTLIndirectCommandBuffer, MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPipelineState,
    MTLRenderStages, MTLResource, MTLResourceUsage, MTLScissorRect, MTLSize, MTLTriangleFillMode,
    MTLViewport, MTLVisibilityResultMode, MTLWinding,
};
use winemetal_wire::{
    MemPtr, SM50_BINDING_INDEX_DRAW_ARGUMENTS, SM50_BINDING_INDEX_INDEX_BUFFER,
    SM50_BINDING_INDEX_INDIRECT_ARGUMENTS, UnixcallGenericObjCmdNoret,
    WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE, WMT_RENDER_COMMAND_DRAW,
    WMT_RENDER_COMMAND_DRAW_INDEXED, WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DRAW_INDIRECT, WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS,
    WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT, WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
    WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_RENDER_COMMAND_MEMORY_BARRIER,
    WMT_RENDER_COMMAND_SET_BLEND_FACTOR, WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
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
    WMT_RENDER_COMMAND_WAIT_FOR_FENCE, WmtcmdBase, WmtcmdRenderDispatchThreadsPerTile,
    WmtcmdRenderDraw, WmtcmdRenderDrawIndexed, WmtcmdRenderDrawIndexedIndirect,
    WmtcmdRenderDrawIndirect, WmtcmdRenderDrawMeshthreadgroups,
    WmtcmdRenderDrawMeshthreadgroupsIndirect, WmtcmdRenderDxmtGeometryDraw,
    WmtcmdRenderDxmtGeometryDrawIndexed, WmtcmdRenderDxmtGeometryDrawIndexedIndirect,
    WmtcmdRenderDxmtGeometryDrawIndirect, WmtcmdRenderDxmtTessellationMeshDraw,
    WmtcmdRenderDxmtTessellationMeshDrawIndexed,
    WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect,
    WmtcmdRenderDxmtTessellationMeshDrawIndirect, WmtcmdRenderExecutecommands, WmtcmdRenderFenceOp,
    WmtcmdRenderMemoryBarrier, WmtcmdRenderSetblendcolor, WmtcmdRenderSetbuffer,
    WmtcmdRenderSetbufferoffset, WmtcmdRenderSetbytes, WmtcmdRenderSetdepthstencilstate,
    WmtcmdRenderSetdsso, WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate,
    WmtcmdRenderSetsampler, WmtcmdRenderSetscissorrect, WmtcmdRenderSetscissorrects,
    WmtcmdRenderSetstencilref, WmtcmdRenderSettexture, WmtcmdRenderSetviewport,
    WmtcmdRenderSetviewports, WmtcmdRenderSetvisibilitymode, WmtcmdRenderUseresource,
};

use super::{body, range, raw, size};
use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, mem_ref, params},
    float_cast::u64_to_usize,
    handle::obj,
    histogram::{self, Kind},
};

type Encoder = ProtocolObject<dyn MTLRenderCommandEncoder>;
type Buffer = ProtocolObject<dyn MTLBuffer>;
type Fence = ProtocolObject<dyn MTLFence>;

/// Cached encoder state across a render command list.
///
/// Elides redundant Objective-C sends and turns same-buffer rebinds into lightweight
/// `*BufferOffset:atIndex:` updates.
struct RenderState {
    vertex_buffers: [(u64, usize); 32],
    fragment_buffers: [(u64, usize); 32],
    mesh_buffers: [(u64, usize); 32],
    object_buffers: [(u64, usize); 32],
    pso: u64,
    dss: u64,
    stencil_ref: u32,
    fill_mode: u8,
    cull_mode: u8,
    depth_clip_mode: u8,
    winding: u8,
    depth_bias_bits: [u32; 3],
}

impl RenderState {
    const fn new() -> Self {
        Self {
            vertex_buffers: [(u64::MAX, usize::MAX); 32],
            fragment_buffers: [(u64::MAX, usize::MAX); 32],
            mesh_buffers: [(u64::MAX, usize::MAX); 32],
            object_buffers: [(u64::MAX, usize::MAX); 32],
            pso: u64::MAX,
            dss: u64::MAX,
            stencil_ref: u32::MAX,
            fill_mode: u8::MAX,
            cull_mode: u8::MAX,
            depth_clip_mode: u8::MAX,
            winding: u8::MAX,
            depth_bias_bits: [u32::MAX; 3],
        }
    }
}

/// Slot 38: each command of the list on the encoder, in order.
///
/// Nop and unknown types are skipped and the walk goes on. A nil encoder sends nothing, as the
/// C's sends to nil did.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// `cmd_head` is null or the first node of a list the PE built: each node live and of the
/// structure its type names, its handles 0 or live objects, its `next` null or the next node.
#[unsafe(export_name = "_MTLRenderCommandEncoder_encodeCommands")]
pub unsafe extern "C-unwind" fn encode_commands(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjCmdNoret>(args) };
    // SAFETY: as above.
    let Some(encoder) = (unsafe { obj::<Encoder>(p.encoder) }) else {
        return STATUS_SUCCESS;
    };
    let mut state = RenderState::new();
    let mut counted = histogram::list(Kind::Render);
    // SAFETY: the caller's contract.
    unsafe { histogram::dump(&Kind::Render, p.cmd_head) };
    let mut at = p.cmd_head;
    // SAFETY: as above.
    while let Some(cmd) = unsafe { mem_ref::<WmtcmdBase>(at) } {
        if let Some(list) = counted.as_mut() {
            list.record(cmd.r#type);
        }
        // SAFETY: as above.
        unsafe { encode(encoder, &mut state, cmd.r#type, at) };
        at = cmd.next;
    }
    histogram::finish(counted);
    STATUS_SUCCESS
}

/// The render sends a list's commands make, each object parameter nullable: 0 goes out as nil.
trait RawRender {
    // allow: the class of CONVENTIONS' exception 4: Metal's selector takes these arguments one by
    // one, and a re-declaration must take them as the selector does.
    #[allow(clippy::too_many_arguments)]
    unsafe fn draw_indexed(
        &self,
        primitive_type: MTLPrimitiveType,
        index_count: usize,
        index_type: MTLIndexType,
        index_buffer: Option<&Buffer>,
        index_buffer_offset: usize,
        instance_count: usize,
        base_vertex: isize,
        base_instance: usize,
    );
    unsafe fn draw_indirect(
        &self,
        primitive_type: MTLPrimitiveType,
        indirect_buffer: Option<&Buffer>,
        indirect_buffer_offset: usize,
    );
    unsafe fn draw_indexed_indirect(
        &self,
        primitive_type: MTLPrimitiveType,
        index_type: MTLIndexType,
        index_buffer: Option<&Buffer>,
        index_buffer_offset: usize,
        indirect_buffer: Option<&Buffer>,
        indirect_buffer_offset: usize,
    );
    unsafe fn draw_mesh_threadgroups_indirect(
        &self,
        indirect_buffer: Option<&Buffer>,
        indirect_buffer_offset: usize,
        threads_per_object_threadgroup: MTLSize,
        threads_per_mesh_threadgroup: MTLSize,
    );
    fn use_resource(
        &self,
        resource: Option<&ProtocolObject<dyn MTLResource>>,
        usage: MTLResourceUsage,
        stages: MTLRenderStages,
    );
    unsafe fn set_vertex_bytes(&self, bytes: *const c_void, length: usize, index: usize);
    unsafe fn set_fragment_bytes(&self, bytes: *const c_void, length: usize, index: usize);
    unsafe fn set_viewports(&self, viewports: *const MTLViewport, count: usize);
    unsafe fn set_scissor_rects(&self, rects: *const MTLScissorRect, count: usize);
    fn set_pipeline_state(&self, state: Option<&ProtocolObject<dyn MTLRenderPipelineState>>);
    fn update_fence(&self, fence: Option<&Fence>, stages: MTLRenderStages);
    fn wait_for_fence(&self, fence: Option<&Fence>, stages: MTLRenderStages);
    unsafe fn execute_commands(
        &self,
        buffer: Option<&ProtocolObject<dyn MTLIndirectCommandBuffer>>,
        range: NSRange,
    );
}

impl RawRender for Encoder {
    extern_methods!(
        /// `-drawIndexedPrimitives:indexCount:indexType:indexBuffer:…:baseInstance:`.
        #[unsafe(method(drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:instanceCount:baseVertex:baseInstance:))]
        #[unsafe(method_family = none)]
        unsafe fn draw_indexed(
            &self,
            primitive_type: MTLPrimitiveType,
            index_count: usize,
            index_type: MTLIndexType,
            index_buffer: Option<&Buffer>,
            index_buffer_offset: usize,
            instance_count: usize,
            base_vertex: isize,
            base_instance: usize,
        );

        /// `-drawPrimitives:indirectBuffer:indirectBufferOffset:`.
        #[unsafe(method(drawPrimitives:indirectBuffer:indirectBufferOffset:))]
        #[unsafe(method_family = none)]
        unsafe fn draw_indirect(
            &self,
            primitive_type: MTLPrimitiveType,
            indirect_buffer: Option<&Buffer>,
            indirect_buffer_offset: usize,
        );

        /// `-drawIndexedPrimitives:indexType:indexBuffer:…:indirectBufferOffset:`.
        #[unsafe(method(drawIndexedPrimitives:indexType:indexBuffer:indexBufferOffset:indirectBuffer:indirectBufferOffset:))]
        #[unsafe(method_family = none)]
        unsafe fn draw_indexed_indirect(
            &self,
            primitive_type: MTLPrimitiveType,
            index_type: MTLIndexType,
            index_buffer: Option<&Buffer>,
            index_buffer_offset: usize,
            indirect_buffer: Option<&Buffer>,
            indirect_buffer_offset: usize,
        );

        /// `-drawMeshThreadgroupsWithIndirectBuffer:…:threadsPerMeshThreadgroup:`.
        #[unsafe(method(drawMeshThreadgroupsWithIndirectBuffer:indirectBufferOffset:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:))]
        #[unsafe(method_family = none)]
        unsafe fn draw_mesh_threadgroups_indirect(
            &self,
            indirect_buffer: Option<&Buffer>,
            indirect_buffer_offset: usize,
            threads_per_object_threadgroup: MTLSize,
            threads_per_mesh_threadgroup: MTLSize,
        );

        /// `-useResource:usage:stages:`.
        #[unsafe(method(useResource:usage:stages:))]
        #[unsafe(method_family = none)]
        fn use_resource(
            &self,
            resource: Option<&ProtocolObject<dyn MTLResource>>,
            usage: MTLResourceUsage,
            stages: MTLRenderStages,
        );

        /// `-setVertexBytes:length:atIndex:`, the guest's address as it is.
        #[unsafe(method(setVertexBytes:length:atIndex:))]
        #[unsafe(method_family = none)]
        unsafe fn set_vertex_bytes(&self, bytes: *const c_void, length: usize, index: usize);

        /// `-setFragmentBytes:length:atIndex:`, the guest's address as it is.
        #[unsafe(method(setFragmentBytes:length:atIndex:))]
        #[unsafe(method_family = none)]
        unsafe fn set_fragment_bytes(&self, bytes: *const c_void, length: usize, index: usize);

        /// `-setViewports:count:`, the guest's address as it is.
        #[unsafe(method(setViewports:count:))]
        #[unsafe(method_family = none)]
        unsafe fn set_viewports(&self, viewports: *const MTLViewport, count: usize);

        /// `-setScissorRects:count:`, the guest's address as it is.
        #[unsafe(method(setScissorRects:count:))]
        #[unsafe(method_family = none)]
        unsafe fn set_scissor_rects(&self, rects: *const MTLScissorRect, count: usize);

        /// `-setRenderPipelineState:`.
        #[unsafe(method(setRenderPipelineState:))]
        #[unsafe(method_family = none)]
        fn set_pipeline_state(&self, state: Option<&ProtocolObject<dyn MTLRenderPipelineState>>);

        /// `-updateFence:afterStages:`.
        #[unsafe(method(updateFence:afterStages:))]
        #[unsafe(method_family = none)]
        fn update_fence(&self, fence: Option<&Fence>, stages: MTLRenderStages);

        /// `-waitForFence:beforeStages:`.
        #[unsafe(method(waitForFence:beforeStages:))]
        #[unsafe(method_family = none)]
        fn wait_for_fence(&self, fence: Option<&Fence>, stages: MTLRenderStages);

        /// `-executeCommandsInBuffer:withRange:`.
        #[unsafe(method(executeCommandsInBuffer:withRange:))]
        #[unsafe(method_family = none)]
        unsafe fn execute_commands(
            &self,
            buffer: Option<&ProtocolObject<dyn MTLIndirectCommandBuffer>>,
            range: NSRange,
        );
    );
}

/// One command on the encoder; Nop and unknown types send nothing.
///
/// # Safety
///
/// `at` points to a live node of type `ty`, of the structure the type names, whose handles are 0
/// or live objects of their kinds.
unsafe fn encode(encoder: &Encoder, state: &mut RenderState, ty: u16, at: MemPtr) {
    match ty {
        WMT_RENDER_COMMAND_USE_RESOURCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderUseresource>(at) };
            let usage = MTLResourceUsage(c.usage as usize);
            let stages = MTLRenderStages(usize::from(c.stages));
            // SAFETY: as above.
            encoder.use_resource(unsafe { obj(c.resource) }, usage, stages);
        }
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbuffer>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            if let Some(slot) = state.vertex_buffers.get_mut(index) {
                if *slot == (c.buffer, offset) {
                    return;
                }
                if slot.0 == c.buffer && c.buffer != 0 {
                    slot.1 = offset;
                    // SAFETY: the C's send of the guest's values: Metal takes them as the C's.
                    unsafe { encoder.setVertexBufferOffset_atIndex(offset, index) };
                    return;
                }
                *slot = (c.buffer, offset);
            }
            // SAFETY: as above.
            let buffer = unsafe { obj(c.buffer) };
            // SAFETY: the C's send of the guest's values: Metal takes them as the C's.
            unsafe { encoder.setVertexBuffer_offset_atIndex(buffer, offset, index) };
        }
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbufferoffset>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            if let Some(slot) = state.vertex_buffers.get_mut(index) {
                if slot.0 != u64::MAX && slot.1 == offset {
                    return;
                }
                slot.1 = offset;
            }
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setVertexBufferOffset_atIndex(offset, index) };
        }
        WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbuffer>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            if let Some(slot) = state.fragment_buffers.get_mut(index) {
                if *slot == (c.buffer, offset) {
                    return;
                }
                if slot.0 == c.buffer && c.buffer != 0 {
                    slot.1 = offset;
                    // SAFETY: the C's send of the guest's values, as above.
                    unsafe { encoder.setFragmentBufferOffset_atIndex(offset, index) };
                    return;
                }
                *slot = (c.buffer, offset);
            }
            // SAFETY: as above.
            let buffer = unsafe { obj(c.buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setFragmentBuffer_offset_atIndex(buffer, offset, index) };
        }
        WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbufferoffset>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            if let Some(slot) = state.fragment_buffers.get_mut(index) {
                if slot.0 != u64::MAX && slot.1 == offset {
                    return;
                }
                slot.1 = offset;
            }
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setFragmentBufferOffset_atIndex(offset, index) };
        }
        WMT_RENDER_COMMAND_SET_MESH_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbuffer>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            if let Some(slot) = state.mesh_buffers.get_mut(index) {
                if *slot == (c.buffer, offset) {
                    return;
                }
                if slot.0 == c.buffer && c.buffer != 0 {
                    slot.1 = offset;
                    // SAFETY: the C's send of the guest's values, as above.
                    unsafe { encoder.setMeshBufferOffset_atIndex(offset, index) };
                    return;
                }
                *slot = (c.buffer, offset);
            }
            // SAFETY: as above.
            let buffer = unsafe { obj(c.buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setMeshBuffer_offset_atIndex(buffer, offset, index) };
        }
        WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbufferoffset>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            if let Some(slot) = state.mesh_buffers.get_mut(index) {
                if slot.0 != u64::MAX && slot.1 == offset {
                    return;
                }
                slot.1 = offset;
            }
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setMeshBufferOffset_atIndex(offset, index) };
        }
        WMT_RENDER_COMMAND_SET_OBJECT_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbuffer>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            // SAFETY: as above.
            unsafe { set_object_buffer_cached(encoder, state, c.buffer, offset, index) };
        }
        WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbufferoffset>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { set_object_buffer_offset_cached(encoder, state, offset, index) };
        }
        WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbytes>(at) };
            let (length, index) = (u64_to_usize(c.length), usize::from(c.index));
            if let Some(slot) = state.fragment_buffers.get_mut(index) {
                *slot = (u64::MAX, usize::MAX);
            }
            // SAFETY: the guest's `length` bytes at `bytes`, as the C passed them.
            unsafe { encoder.set_fragment_bytes(raw(c.bytes), length, index) };
        }
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSettexture>(at) };
            // SAFETY: as above.
            let texture = unsafe { obj(c.texture) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setFragmentTexture_atIndex(texture, usize::from(c.index)) };
        }
        WMT_RENDER_COMMAND_SET_RASTERIZER_STATE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetrasterizerstate>(at) };
            if state.fill_mode != c.fill_mode {
                state.fill_mode = c.fill_mode;
                encoder.setTriangleFillMode(MTLTriangleFillMode(usize::from(c.fill_mode)));
            }
            if state.cull_mode != c.cull_mode {
                state.cull_mode = c.cull_mode;
                encoder.setCullMode(MTLCullMode(usize::from(c.cull_mode)));
            }
            if state.depth_clip_mode != c.depth_clip_mode {
                state.depth_clip_mode = c.depth_clip_mode;
                encoder.setDepthClipMode(MTLDepthClipMode(usize::from(c.depth_clip_mode)));
            }
            let bias_bits = [
                c.depth_bias.to_bits(),
                c.scole_scale.to_bits(),
                c.depth_bias_clamp.to_bits(),
            ];
            if state.depth_bias_bits != bias_bits {
                state.depth_bias_bits = bias_bits;
                encoder.setDepthBias_slopeScale_clamp(
                    c.depth_bias,
                    c.scole_scale,
                    c.depth_bias_clamp,
                );
            }
            if state.winding != c.winding {
                state.winding = c.winding;
                encoder.setFrontFacingWinding(MTLWinding(usize::from(c.winding)));
            }
        }
        WMT_RENDER_COMMAND_SET_VIEWPORTS => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetviewports>(at) };
            // SAFETY: the guest's `viewport_count` viewports, the C's `MTLViewport` layout.
            unsafe { encoder.set_viewports(raw(c.viewports), usize::from(c.viewport_count)) };
        }
        WMT_RENDER_COMMAND_SET_SCISSOR_RECTS => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetscissorrects>(at) };
            // SAFETY: the guest's `rect_count` rectangles, the C's `MTLScissorRect` layout.
            unsafe { encoder.set_scissor_rects(raw(c.scissor_rects), usize::from(c.rect_count)) };
        }
        WMT_RENDER_COMMAND_SET_PSO => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetpso>(at) };
            if state.pso != c.pso {
                state.pso = c.pso;
                // SAFETY: as above.
                encoder.set_pipeline_state(unsafe { obj(c.pso) });
            }
        }
        WMT_RENDER_COMMAND_SET_DSSO => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetdsso>(at) };
            if state.dss != c.dsso {
                state.dss = c.dsso;
                // SAFETY: as above.
                encoder.setDepthStencilState(unsafe { obj(c.dsso) });
            }
            let sref = u32::from(c.stencil_ref);
            if state.stencil_ref != sref {
                state.stencil_ref = sref;
                encoder.setStencilReferenceValue(sref);
            }
        }
        WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetdepthstencilstate>(at) };
            if state.dss != c.depth_stencil_state {
                state.dss = c.depth_stencil_state;
                // SAFETY: as above.
                encoder.setDepthStencilState(unsafe { obj(c.depth_stencil_state) });
            }
        }
        WMT_RENDER_COMMAND_SET_VERTEX_BYTES => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetbytes>(at) };
            let (length, index) = (u64_to_usize(c.length), usize::from(c.index));
            if let Some(slot) = state.vertex_buffers.get_mut(index) {
                *slot = (u64::MAX, usize::MAX);
            }
            // SAFETY: the guest's `length` bytes at `bytes`, as the C passed them.
            unsafe { encoder.set_vertex_bytes(raw(c.bytes), length, index) };
        }
        WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSettexture>(at) };
            // SAFETY: as above.
            let texture = unsafe { obj(c.texture) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setVertexTexture_atIndex(texture, usize::from(c.index)) };
        }
        WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetsampler>(at) };
            // SAFETY: as above.
            let sampler = unsafe { obj(c.sampler) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setVertexSamplerState_atIndex(sampler, usize::from(c.index)) };
        }
        WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetsampler>(at) };
            // SAFETY: as above.
            let sampler = unsafe { obj(c.sampler) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setFragmentSamplerState_atIndex(sampler, usize::from(c.index)) };
        }
        WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetblendcolor>(at) };
            encoder.setBlendColorRed_green_blue_alpha(c.red, c.green, c.blue, c.alpha);
            let sref = u32::from(c.stencil_ref);
            if state.stencil_ref != sref {
                state.stencil_ref = sref;
                encoder.setStencilReferenceValue(sref);
            }
        }
        WMT_RENDER_COMMAND_SET_BLEND_FACTOR => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetblendcolor>(at) };
            encoder.setBlendColorRed_green_blue_alpha(c.red, c.green, c.blue, c.alpha);
        }
        WMT_RENDER_COMMAND_SET_STENCIL_REF => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetstencilref>(at) };
            let sref = u32::from(c.stencil_ref);
            if state.stencil_ref != sref {
                state.stencil_ref = sref;
                encoder.setStencilReferenceValue(sref);
            }
        }
        WMT_RENDER_COMMAND_SET_VISIBILITY_MODE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetvisibilitymode>(at) };
            let mode = MTLVisibilityResultMode(usize::from(c.mode));
            encoder.setVisibilityResultMode_offset(mode, u64_to_usize(c.offset));
        }
        WMT_RENDER_COMMAND_DRAW => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDraw>(at) };
            let primitive = MTLPrimitiveType(usize::from(c.primitive_type));
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                    primitive,
                    u64_to_usize(c.vertex_start),
                    u64_to_usize(c.vertex_count),
                    c.instance_count as usize,
                    c.base_instance as usize,
                );
            }
        }
        WMT_RENDER_COMMAND_DRAW_INDEXED => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDrawIndexed>(at) };
            // SAFETY: as above.
            let index_buffer = unsafe { obj(c.index_buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.draw_indexed(
                    MTLPrimitiveType(usize::from(c.primitive_type)),
                    u64_to_usize(c.index_count),
                    MTLIndexType(usize::from(c.index_type)),
                    index_buffer,
                    u64_to_usize(c.index_buffer_offset),
                    c.instance_count as usize,
                    c.base_vertex as isize,
                    c.base_instance as usize,
                );
            }
        }
        WMT_RENDER_COMMAND_DRAW_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDrawIndirect>(at) };
            // SAFETY: as above.
            let buffer = unsafe { obj(c.indirect_args_buffer) };
            let primitive = MTLPrimitiveType(usize::from(c.primitive_type));
            let offset = u64_to_usize(c.indirect_args_offset);
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.draw_indirect(primitive, buffer, offset) };
        }
        WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDrawIndexedIndirect>(at) };
            // SAFETY: as above.
            let index_buffer = unsafe { obj(c.index_buffer) };
            // SAFETY: as above.
            let indirect_buffer = unsafe { obj(c.indirect_args_buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.draw_indexed_indirect(
                    MTLPrimitiveType(usize::from(c.primitive_type)),
                    MTLIndexType(usize::from(c.index_type)),
                    index_buffer,
                    u64_to_usize(c.index_buffer_offset),
                    indirect_buffer,
                    u64_to_usize(c.indirect_args_offset),
                );
            }
        }
        WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDrawMeshthreadgroups>(at) };
            encoder.drawMeshThreadgroups_threadsPerObjectThreadgroup_threadsPerMeshThreadgroup(
                size(&c.threadgroup_per_grid),
                size(&c.object_threadgroup_size),
                size(&c.mesh_threadgroup_size),
            );
        }
        WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDrawMeshthreadgroupsIndirect>(at) };
            // SAFETY: as above.
            let buffer = unsafe { obj(c.indirect_args_buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.draw_mesh_threadgroups_indirect(
                    buffer,
                    u64_to_usize(c.indirect_args_offset),
                    size(&c.object_threadgroup_size),
                    size(&c.mesh_threadgroup_size),
                );
            }
        }
        WMT_RENDER_COMMAND_MEMORY_BARRIER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderMemoryBarrier>(at) };
            encoder.memoryBarrierWithScope_afterStages_beforeStages(
                MTLBarrierScope(usize::from(c.scope)),
                MTLRenderStages(usize::from(c.stages_after)),
                MTLRenderStages(usize::from(c.stages_before)),
            );
        }
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtGeometryDraw>(at) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { set_draw_arguments_offset(encoder, state, c.draw_arguments_offset) };
            geometry_draw(encoder, c.warp_count, c.instance_count, c.vertex_per_warp);
        }
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtGeometryDrawIndexed>(at) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { set_index_buffer(encoder, state, c.index_buffer, c.index_buffer_offset) };
            // SAFETY: as above.
            unsafe { set_draw_arguments_offset(encoder, state, c.draw_arguments_offset) };
            geometry_draw(encoder, c.warp_count, c.instance_count, c.vertex_per_warp);
        }
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtGeometryDrawIndirect>(at) };
            // SAFETY: as above.
            let dispatch = unsafe { obj(c.dispatch_args_buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                set_indirect_arguments(
                    encoder,
                    state,
                    c.indirect_args_buffer,
                    c.indirect_args_offset,
                );
            }
            let object = threads(c.vertex_per_warp as usize, 1);
            // SAFETY: as above.
            unsafe { mesh_indirect(encoder, dispatch, c.dispatch_args_offset, object, 1) };
            // SAFETY: as above.
            unsafe { set_draw_arguments(encoder, state, c.imm_draw_arguments) };
        }
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtGeometryDrawIndexedIndirect>(at) };
            // SAFETY: as above.
            let dispatch = unsafe { obj(c.dispatch_args_buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { set_index_buffer(encoder, state, c.index_buffer, c.index_buffer_offset) };
            // SAFETY: as above.
            unsafe {
                set_indirect_arguments(
                    encoder,
                    state,
                    c.indirect_args_buffer,
                    c.indirect_args_offset,
                );
            }
            let object = threads(c.vertex_per_warp as usize, 1);
            // SAFETY: as above.
            unsafe { mesh_indirect(encoder, dispatch, c.dispatch_args_offset, object, 1) };
            // SAFETY: as above.
            unsafe { set_draw_arguments(encoder, state, c.imm_draw_arguments) };
        }
        WMT_RENDER_COMMAND_WAIT_FOR_FENCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderFenceOp>(at) };
            let stages = MTLRenderStages(usize::from(c.stages));
            // SAFETY: as above.
            encoder.wait_for_fence(unsafe { obj(c.fence) }, stages);
        }
        WMT_RENDER_COMMAND_UPDATE_FENCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderFenceOp>(at) };
            let stages = MTLRenderStages(usize::from(c.stages));
            // SAFETY: as above.
            encoder.update_fence(unsafe { obj(c.fence) }, stages);
        }
        WMT_RENDER_COMMAND_SET_VIEWPORT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetviewport>(at) };
            let v = &c.viewport;
            encoder.setViewport(MTLViewport {
                originX: v.origin_x,
                originY: v.origin_y,
                width: v.width,
                height: v.height,
                znear: v.znear,
                zfar: v.zfar,
            });
        }
        WMT_RENDER_COMMAND_SET_SCISSOR_RECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderSetscissorrect>(at) };
            let r = &c.scissor_rect;
            encoder.setScissorRect(MTLScissorRect {
                x: u64_to_usize(r.x),
                y: u64_to_usize(r.y),
                width: u64_to_usize(r.width),
                height: u64_to_usize(r.height),
            });
        }
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtTessellationMeshDraw>(at) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { set_draw_arguments_offset(encoder, state, c.draw_arguments_offset) };
            tessellation_draw(
                encoder,
                c.patch_per_mesh_instance,
                c.instance_count,
                c.threads_per_patch,
                c.patch_per_group,
            );
        }
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtTessellationMeshDrawIndexed>(at) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { set_index_buffer(encoder, state, c.index_buffer, c.index_buffer_offset) };
            // SAFETY: as above.
            unsafe { set_draw_arguments_offset(encoder, state, c.draw_arguments_offset) };
            tessellation_draw(
                encoder,
                c.patch_per_mesh_instance,
                c.instance_count,
                c.threads_per_patch,
                c.patch_per_group,
            );
        }
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtTessellationMeshDrawIndirect>(at) };
            // SAFETY: as above.
            let dispatch = unsafe { obj(c.dispatch_args_buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                set_indirect_arguments(
                    encoder,
                    state,
                    c.indirect_args_buffer,
                    c.indirect_args_offset,
                );
            }
            let object = threads(c.threads_per_patch as usize, c.patch_per_group as usize);
            // SAFETY: as above.
            unsafe { mesh_indirect(encoder, dispatch, c.dispatch_args_offset, object, 32) };
            // SAFETY: as above.
            unsafe { set_draw_arguments(encoder, state, c.imm_draw_arguments) };
        }
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect>(at) };
            // SAFETY: as above.
            let dispatch = unsafe { obj(c.dispatch_args_buffer) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { set_index_buffer(encoder, state, c.index_buffer, c.index_buffer_offset) };
            // SAFETY: as above.
            unsafe {
                set_indirect_arguments(
                    encoder,
                    state,
                    c.indirect_args_buffer,
                    c.indirect_args_offset,
                );
            }
            let object = threads(c.threads_per_patch as usize, c.patch_per_group as usize);
            // SAFETY: as above.
            unsafe { mesh_indirect(encoder, dispatch, c.dispatch_args_offset, object, 32) };
            // SAFETY: as above.
            unsafe { set_draw_arguments(encoder, state, c.imm_draw_arguments) };
        }
        WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderDispatchThreadsPerTile>(at) };
            encoder.dispatchThreadsPerTile(threads(c.width as usize, c.height as usize));
        }
        WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdRenderExecutecommands>(at) };
            // SAFETY: as above.
            let buffer = unsafe { obj(c.indirect_command_buffer) };
            let executed = range(c.location as usize, c.length as usize);
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.execute_commands(buffer, executed) };
        }
        _ => {}
    }
}

/// Binds an object buffer at `index` with `(buffer, offset)` state deduplication.
///
/// # Safety
///
/// `buffer` is 0 or a live `MTLBuffer` handle; Metal takes `offset` and `index` as C's send did.
unsafe fn set_object_buffer_cached(
    encoder: &Encoder,
    state: &mut RenderState,
    buffer: u64,
    offset: usize,
    index: usize,
) {
    if let Some(slot) = state.object_buffers.get_mut(index) {
        if *slot == (buffer, offset) {
            return;
        }
        if slot.0 == buffer && buffer != 0 {
            slot.1 = offset;
            // SAFETY: the caller's contract.
            unsafe { encoder.setObjectBufferOffset_atIndex(offset, index) };
            return;
        }
        *slot = (buffer, offset);
    }
    // SAFETY: the caller's contract.
    let buf_obj = unsafe { obj(buffer) };
    // SAFETY: the caller's contract.
    unsafe { encoder.setObjectBuffer_offset_atIndex(buf_obj, offset, index) };
}

/// Updates the object buffer offset at `index` with state deduplication.
///
/// # Safety
///
/// Metal takes `offset` and `index` as the C's send did.
unsafe fn set_object_buffer_offset_cached(
    encoder: &Encoder,
    state: &mut RenderState,
    offset: usize,
    index: usize,
) {
    if let Some(slot) = state.object_buffers.get_mut(index) {
        if slot.0 != u64::MAX && slot.1 == offset {
            return;
        }
        slot.1 = offset;
    }
    // SAFETY: the caller's contract.
    unsafe { encoder.setObjectBufferOffset_atIndex(offset, index) };
}

/// The geometry draws' object buffer at `SM50_BINDING_INDEX_INDEX_BUFFER`: the index buffer.
///
/// # Safety
///
/// `buffer` is 0 or a live `MTLBuffer` handle; Metal takes `offset` as the C's send did.
unsafe fn set_index_buffer(encoder: &Encoder, state: &mut RenderState, buffer: u64, offset: u64) {
    let index = SM50_BINDING_INDEX_INDEX_BUFFER as usize;
    // SAFETY: the caller's contract.
    unsafe { set_object_buffer_cached(encoder, state, buffer, u64_to_usize(offset), index) };
}

/// The draw arguments' offset at `SM50_BINDING_INDEX_DRAW_ARGUMENTS`.
///
/// # Safety
///
/// Metal takes `offset` as the C's send did.
unsafe fn set_draw_arguments_offset(encoder: &Encoder, state: &mut RenderState, offset: u64) {
    let index = SM50_BINDING_INDEX_DRAW_ARGUMENTS as usize;
    // SAFETY: the caller's contract.
    unsafe { set_object_buffer_offset_cached(encoder, state, u64_to_usize(offset), index) };
}

/// The indirect arguments at `SM50_BINDING_INDEX_INDIRECT_ARGUMENTS`.
///
/// # Safety
///
/// `buffer` is 0 or a live `MTLBuffer` handle; Metal takes `offset` as the C's send did.
unsafe fn set_indirect_arguments(
    encoder: &Encoder,
    state: &mut RenderState,
    buffer: u64,
    offset: u64,
) {
    let index = SM50_BINDING_INDEX_INDIRECT_ARGUMENTS as usize;
    // SAFETY: the caller's contract.
    unsafe { set_object_buffer_cached(encoder, state, buffer, u64_to_usize(offset), index) };
}

/// The immediate draw arguments at `SM50_BINDING_INDEX_DRAW_ARGUMENTS`, offset 0.
///
/// # Safety
///
/// `buffer` is 0 or a live `MTLBuffer` handle.
unsafe fn set_draw_arguments(encoder: &Encoder, state: &mut RenderState, buffer: u64) {
    let index = SM50_BINDING_INDEX_DRAW_ARGUMENTS as usize;
    // SAFETY: the caller's contract.
    unsafe { set_object_buffer_cached(encoder, state, buffer, 0, index) };
}

/// An indirect mesh draw with `mesh_width` threads per mesh threadgroup.
///
/// # Safety
///
/// Metal takes `offset` as the C's send did.
unsafe fn mesh_indirect(
    encoder: &Encoder,
    buffer: Option<&Buffer>,
    offset: u64,
    object: MTLSize,
    mesh_width: usize,
) {
    let mesh = threads(mesh_width, 1);
    // SAFETY: the caller's contract.
    unsafe { encoder.draw_mesh_threadgroups_indirect(buffer, u64_to_usize(offset), object, mesh) };
}

/// A geometry draw: `warp_count` by `instance_count` threadgroups of `vertex_per_warp` threads.
fn geometry_draw(encoder: &Encoder, warp_count: u32, instance_count: u32, vertex_per_warp: u32) {
    encoder.drawMeshThreadgroups_threadsPerObjectThreadgroup_threadsPerMeshThreadgroup(
        threads(warp_count as usize, instance_count as usize),
        threads(vertex_per_warp as usize, 1),
        threads(1, 1),
    );
}

/// A tessellation draw: patches by instances, `threads_per_patch` by `patch_per_group` threads.
fn tessellation_draw(
    encoder: &Encoder,
    patch_per_mesh_instance: u32,
    instance_count: u32,
    threads_per_patch: u32,
    patch_per_group: u32,
) {
    encoder.drawMeshThreadgroups_threadsPerObjectThreadgroup_threadsPerMeshThreadgroup(
        threads(patch_per_mesh_instance as usize, instance_count as usize),
        threads(threads_per_patch as usize, patch_per_group as usize),
        threads(32, 1),
    );
}

/// A `width` by `height` by 1 size.
const fn threads(width: usize, height: usize) -> MTLSize {
    MTLSize {
        width,
        height,
        depth: 1,
    }
}
