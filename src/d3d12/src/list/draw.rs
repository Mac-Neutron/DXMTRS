//! `PreDraw`, `PreDispatch`, draw/dispatch commands, and raytracing/mesh stubs
//!
//! (`d3d12_command_list.cpp`).

use core::{
    ffi::c_void,
    mem::{align_of, size_of},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use winemetal_sys::{
    Obj,
    sm50::{
        SM50_INDEX_BUFFER_FORMAT_NONE, SM50_INDEX_BUFFER_FORMAT_UINT16,
        SM50_INDEX_BUFFER_FORMAT_UINT32,
    },
};
use winemetal_wire::{
    MemPtr, SM50_BINDING_INDEX_DRAW_ARGUMENTS, SM50_BINDING_INDEX_ROOT_ARGUMENTS,
    SM50_BINDING_INDEX_STATIC_SAMPLERS, SM50_BINDING_INDEX_VERTEX_BUFFER,
    WMT_COMPUTE_COMMAND_DISPATCH, WMT_COMPUTE_COMMAND_NOP, WMT_COMPUTE_COMMAND_SET_BUFFER,
    WMT_COMPUTE_COMMAND_SET_PSO, WMT_INDEX_TYPE_U_INT32, WMT_PRIMITIVE_TYPE_LINE,
    WMT_PRIMITIVE_TYPE_LINE_STRIP, WMT_PRIMITIVE_TYPE_POINT, WMT_PRIMITIVE_TYPE_TRIANGLE,
    WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
    WMT_RENDER_COMMAND_NOP, WMT_RENDER_COMMAND_SET_BLEND_FACTOR,
    WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
    WMT_RENDER_COMMAND_SET_MESH_BUFFER, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER,
    WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
    WMT_RENDER_COMMAND_SET_SCISSOR_RECTS, WMT_RENDER_COMMAND_SET_STENCIL_REF,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER, WMT_RENDER_COMMAND_SET_VIEWPORTS,
    WMT_RENDER_COMMAND_SET_VISIBILITY_MODE, WMT_VISIBILITY_RESULT_MODE_BOOLEAN,
    WMT_VISIBILITY_RESULT_MODE_COUNTING, WMT_VISIBILITY_RESULT_MODE_DISABLED, WMTScissorRect,
    WMTSize, WMTViewport, WmtcmdBase, WmtcmdComputeDispatch, WmtcmdComputeNop,
    WmtcmdComputeSetbuffer, WmtcmdComputeSetpso, WmtcmdRenderDraw, WmtcmdRenderDrawIndexed,
    WmtcmdRenderDxmtGeometryDraw, WmtcmdRenderDxmtGeometryDrawIndexed, WmtcmdRenderNop,
    WmtcmdRenderSetblendcolor, WmtcmdRenderSetbuffer, WmtcmdRenderSetdepthstencilstate,
    WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetscissorrects,
    WmtcmdRenderSetstencilref, WmtcmdRenderSetviewports, WmtcmdRenderSetvisibilitymode,
};

use crate::{
    abi::list_state::{
        D3D_PRIMITIVE_TOPOLOGY_1_CONTROL_POINT_PATCHLIST,
        D3D_PRIMITIVE_TOPOLOGY_32_CONTROL_POINT_PATCHLIST, D3D_PRIMITIVE_TOPOLOGY_LINELIST,
        D3D_PRIMITIVE_TOPOLOGY_LINELIST_ADJ, D3D_PRIMITIVE_TOPOLOGY_LINESTRIP,
        D3D_PRIMITIVE_TOPOLOGY_LINESTRIP_ADJ, D3D_PRIMITIVE_TOPOLOGY_POINTLIST,
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST_ADJ,
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP, D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP_ADJ,
        D3d12DrawArguments, D3d12DrawIndexedArguments,
    },
    allocator::{Allocator, CPU_HEAP_SIZE, GPU_HEAP_SIZE},
    device::null,
    encoder::{ComputeEncoderData, EncoderData, EncoderType, RenderEncoderData},
    list::{DirtyFlags, DrawCallStatus, List},
    pipeline::Graphics,
    root_signature::object::RootSignature,
};

const MAX_SCISSOR_DIM: u64 = 16384;

/// Entry in the vertex buffer binding table uploaded to the GPU heap (`VERTEX_BUFFER_ENTRY`,
///
/// `d3d12_command_list.cpp:379-383`).
#[repr(C)]
struct VertexBufferEntry {
    buffer_handle: u64,
    stride: u32,
    length: u32,
}

#[cfg(target_pointer_width = "64")]
const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0 }
}

#[cfg(target_pointer_width = "32")]
const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0, high: 0 }
}

#[cfg(target_pointer_width = "64")]
fn ptr_to_mem_ptr(ptr: *mut u8) -> MemPtr {
    MemPtr {
        ptr: ptr.expose_provenance() as u64,
    }
}

#[cfg(target_pointer_width = "32")]
fn ptr_to_mem_ptr(ptr: *mut u8) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(ptr.expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

/// Inline bump allocation from `alloc`'s CPU command arena (`AllocateCPUHeap`,
///
/// `d3d12_command_allocator.hpp:273`).
pub fn alloc_cpu_heap(alloc: &mut Allocator, length: usize, alignment: usize) -> *mut u8 {
    let align = alignment.max(1);
    if !alloc.cpu_heap.is_null() {
        let aligned = alloc.cpu_heap_offset.next_multiple_of(align);
        let end = aligned.saturating_add(length);
        if end < CPU_HEAP_SIZE {
            alloc.cpu_heap_offset = end;
            // SAFETY: `end < CPU_HEAP_SIZE`, within the live `alloc.cpu_heap` block.
            return unsafe { alloc.cpu_heap.add(aligned) };
        }
    }
    alloc.allocate_cpu_heap(length, alignment)
}

/// Inline bump allocation from `alloc`'s GPU argument/constant arena (`AllocateGPUHeap`,
///
/// `d3d12_command_allocator.hpp:461`).
pub fn alloc_gpu_heap(alloc: &mut Allocator, length: usize, alignment: usize) -> (*mut u8, usize) {
    if length == 0 {
        return (ptr::null_mut(), 0);
    }
    let align = alignment.max(1);
    if !alloc.gpu_heap.is_null() {
        let aligned = alloc.gpu_heap_offset.next_multiple_of(align);
        let end = aligned.saturating_add(length);
        if end < GPU_HEAP_SIZE {
            alloc.gpu_heap_offset = end;
            // SAFETY: `end < GPU_HEAP_SIZE`, within the live `alloc.gpu_heap` block.
            let ptr = unsafe { alloc.gpu_heap.add(aligned) };
            return (ptr, aligned);
        }
    }
    alloc.allocate_gpu_heap(length, alignment)
}

fn alloc_writes(alloc: &mut Allocator, resource: *const c_void) {
    if alloc.encoder_current.is_null() {
        return;
    }
    // SAFETY: `alloc.encoder_current` is a live `EncoderData` node.
    let enc = unsafe { &mut *alloc.encoder_current };
    let count = usize::from(enc.write_count);
    if resource.is_null() || count == enc.writes.len() {
        enc.writes_unknown = true;
        return;
    }
    if enc.writes[..count].contains(&resource) {
        return;
    }
    enc.writes[count] = resource;
    enc.write_count += 1;
}

/// Appends `cmd` to the active [`RenderEncoderData`]'s command chain (`EncodeRenderCommand`,
///
/// `d3d12_command_allocator.hpp:418`).
pub fn encode_render_cmd<T>(alloc: &mut Allocator, cmd: T) -> *mut T {
    let ptr = alloc_cpu_heap(alloc, size_of::<T>(), align_of::<T>())
        .cast::<c_void>()
        .cast::<T>();
    // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `T`.
    unsafe { ptr::write(ptr, cmd) };
    let base_ptr = ptr.cast::<WmtcmdBase>();
    // SAFETY: `T` is a `WmtcmdRender*` struct with `WmtcmdBase` at offset 0.
    unsafe { (*base_ptr).next = null_mem_ptr() };
    // SAFETY: `alloc.encoder_current` is a live `RenderEncoderData`.
    let render = unsafe { &mut *alloc.encoder_current.cast::<RenderEncoderData>() };
    // SAFETY: `render.cmd_tail` points to the last command in `render`'s chain.
    unsafe { (*render.cmd_tail).next = ptr_to_mem_ptr(base_ptr.cast::<u8>()) };
    render.cmd_tail = base_ptr;
    render.base.barriers_last = alloc.barriers;
    ptr
}

/// Appends `cmd` to the active [`ComputeEncoderData`]'s command chain (`EncodeComputeCommand`,
///
/// `d3d12_command_allocator.hpp:391`).
pub fn encode_compute_cmd<T>(alloc: &mut Allocator, cmd: T) -> *mut T {
    let ptr = alloc_cpu_heap(alloc, size_of::<T>(), align_of::<T>())
        .cast::<c_void>()
        .cast::<T>();
    // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `T`.
    unsafe { ptr::write(ptr, cmd) };
    let base_ptr = ptr.cast::<WmtcmdBase>();
    // SAFETY: `T` is a `WmtcmdCompute*` struct with `WmtcmdBase` at offset 0.
    unsafe { (*base_ptr).next = null_mem_ptr() };
    // SAFETY: `alloc.encoder_current` is a live `ComputeEncoderData`.
    let compute = unsafe { &mut *alloc.encoder_current.cast::<ComputeEncoderData>() };
    // SAFETY: `compute.cmd_tail` points to the last command in `compute`'s chain.
    unsafe { (*compute.cmd_tail).next = ptr_to_mem_ptr(base_ptr.cast::<u8>()) };
    compute.cmd_tail = base_ptr;
    compute.base.barriers_last = alloc.barriers;
    ptr
}

/// Maps a `D3D12_PRIMITIVE_TOPOLOGY` to `(WMTPrimitiveType, control_point_count)`
///
/// (`to_metal_primitive_type`, `d3d12_command_list.cpp:71`).
#[must_use]
pub const fn to_metal_primitive_type(topo: u32) -> Option<(u8, u32)> {
    match topo {
        D3D_PRIMITIVE_TOPOLOGY_LINELIST => Some((WMT_PRIMITIVE_TYPE_LINE, 0)),
        D3D_PRIMITIVE_TOPOLOGY_LINESTRIP => Some((WMT_PRIMITIVE_TYPE_LINE_STRIP, 0)),
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST => Some((WMT_PRIMITIVE_TYPE_TRIANGLE, 0)),
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP => Some((WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP, 0)),
        D3D_PRIMITIVE_TOPOLOGY_POINTLIST
        | D3D_PRIMITIVE_TOPOLOGY_LINELIST_ADJ
        | D3D_PRIMITIVE_TOPOLOGY_LINESTRIP_ADJ
        | D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST_ADJ
        | D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP_ADJ => Some((WMT_PRIMITIVE_TYPE_POINT, 0)),
        t if t >= D3D_PRIMITIVE_TOPOLOGY_1_CONTROL_POINT_PATCHLIST
            && t <= D3D_PRIMITIVE_TOPOLOGY_32_CONTROL_POINT_PATCHLIST =>
        {
            Some((WMT_PRIMITIVE_TYPE_POINT, t - 32))
        }
        _ => None,
    }
}

/// Returns `true` when `topo` is a line-strip or triangle-strip topology (`is_strip_topology`,
///
/// `d3d12_command_list.cpp:65`).
#[must_use]
pub const fn is_strip_topology(topo: u32) -> bool {
    matches!(
        topo,
        D3D_PRIMITIVE_TOPOLOGY_LINESTRIP
            | D3D_PRIMITIVE_TOPOLOGY_LINESTRIP_ADJ
            | D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP
            | D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP_ADJ
    )
}

/// Object threadgroup size and vertex step for geometry shader draws (`geometry_warp`,
///
/// `d3d12_command_list.cpp:49`).
#[must_use]
pub const fn geometry_warp(topo: u32) -> (u32, u32) {
    match topo {
        D3D_PRIMITIVE_TOPOLOGY_POINTLIST
        | D3D_PRIMITIVE_TOPOLOGY_LINELIST
        | D3D_PRIMITIVE_TOPOLOGY_LINELIST_ADJ => (32, 32),
        D3D_PRIMITIVE_TOPOLOGY_LINESTRIP => (32, 31),
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST | D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST_ADJ => (30, 30),
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP => (32, 30),
        D3D_PRIMITIVE_TOPOLOGY_LINESTRIP_ADJ => (32, 29),
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP_ADJ => (32, 28),
        _ => (0, 0),
    }
}

fn get_depth_stencil_state(pso: &Graphics, dsv_planar: u32, dsv_readonly_flags: u32) -> u64 {
    if dsv_planar == 0 {
        if pso.device.is_null() {
            return 0;
        }
        // SAFETY: `pso.device` is a valid `Device`.
        let dev = unsafe { &*pso.device };
        return dev
            .default_depth_stencil_state
            .as_ref()
            .map_or(0, Obj::handle);
    }
    if dsv_planar == 1 {
        let idx = if (dsv_readonly_flags & 1) != 0 { 5 } else { 4 };
        return pso.depth_stencil_states[idx]
            .as_ref()
            .map_or(0, Obj::handle);
    }
    let idx = match dsv_readonly_flags {
        3 => 3,
        2 => 2,
        1 => 1,
        _ => 0,
    };
    pso.depth_stencil_states[idx]
        .as_ref()
        .map_or(0, Obj::handle)
}

/// Uploads `count` copies of `staging` (`root_sig.upload_qwords()` qwords each) to the GPU heap
///
/// (`EncodeRootArgument`, `d3d12_command_list.cpp:738`).
pub fn encode_root_argument(
    alloc: &mut Allocator,
    root_sig: &RootSignature,
    staging: &[u64; 64],
    count: usize,
) -> usize {
    let qwords = (root_sig.upload_qwords() as usize).min(staging.len());
    let byte_per_copy = qwords * size_of::<u64>();
    let total_bytes = byte_per_copy.saturating_mul(count);
    let (ptr, offset) = alloc_gpu_heap(alloc, total_bytes, 64);
    if !ptr.is_null() && qwords > 0 {
        let dst_u64 = ptr.cast::<c_void>().cast::<u64>();
        for i in 0..count {
            // SAFETY: `dst_u64` has space for `count * qwords` `u64` elements in `gpu_heap`.
            let dst = unsafe { dst_u64.add(i * qwords) };
            // SAFETY: `staging` has at least `qwords` elements and does not overlap `gpu_heap`.
            unsafe { ptr::copy_nonoverlapping(staging.as_ptr(), dst, qwords) };
        }
    }
    offset
}

/// Uploads `root_sig`'s encoded static samplers to the GPU heap (`EncodeStaticSamplers`,
///
/// `d3d12_command_list.cpp:749`).
pub fn encode_static_samplers(alloc: &mut Allocator, root_sig: &RootSignature) -> usize {
    let samplers = root_sig.encoded_static_samplers();
    let byte_len = core::mem::size_of_val(samplers);
    let (ptr, offset) = alloc_gpu_heap(alloc, byte_len, 64);
    if !ptr.is_null() && !samplers.is_empty() {
        // SAFETY: `ptr` has `byte_len` writable bytes in `gpu_heap`.
        unsafe {
            ptr::copy_nonoverlapping(samplers.as_ptr().cast::<u8>(), ptr, byte_len);
        }
    }
    offset
}

/// Builds `count` vertex buffer binding tables in the GPU heap (`PopulateVertexBufferTable`,
///
/// `d3d12_command_list.cpp:374`).
pub fn populate_vertex_buffer_table(
    list: &mut List,
    count: usize,
    mut null_vb: u64,
) -> (usize, u32) {
    if list.pso_graphics.is_null() || list.allocator.is_null() {
        return (0, 0);
    }
    // SAFETY: `list.pso_graphics` is a valid `Graphics`.
    let slot_mask = unsafe { (*list.pso_graphics).slot_mask };
    if slot_mask == 0 {
        return (0, 0);
    }
    let max_slot = (32 - slot_mask.leading_zeros()) as usize;
    let stride = (size_of::<VertexBufferEntry>() * max_slot).next_multiple_of(16);
    let total_bytes = stride.saturating_mul(count);
    // SAFETY: `list.allocator` is valid while recording.
    let alloc = unsafe { &mut *list.allocator };
    let (mapped, offset) = alloc_gpu_heap(alloc, total_bytes, 16);
    if mapped.is_null() {
        return (0, 0);
    }
    for i in 0..count {
        // SAFETY: `i * stride` is within the `total_bytes` slice allocated from `gpu_heap`.
        let entries = unsafe { mapped.add(i * stride) }
            .cast::<c_void>()
            .cast::<VertexBufferEntry>();
        let mut index = 0usize;
        for slot in 0..max_slot {
            if (slot_mask & (1u32 << slot)) == 0 {
                continue;
            }
            let (loc, vb_stride, size) = list.vertex_buffers[slot];
            let entry = if loc == 0 {
                if null_vb == 0 && !list.device.is_null() {
                    // SAFETY: `list.device` is a valid `Device`.
                    null_vb = unsafe { null::null_vertex_buffer(&*list.device) };
                }
                VertexBufferEntry {
                    buffer_handle: null_vb,
                    stride: 0,
                    length: 0,
                }
            } else {
                VertexBufferEntry {
                    buffer_handle: loc,
                    stride: vb_stride,
                    length: size,
                }
            };
            // SAFETY: `index < max_slot`, within the `i`-th table of `entries`.
            let dst = unsafe { entries.add(index) };
            // SAFETY: `dst` points to valid aligned `VertexBufferEntry` storage in `gpu_heap`.
            unsafe { ptr::write(dst, entry) };
            index += 1;
        }
    }
    (offset, u32::try_from(stride).unwrap_or(0))
}

fn bind_render_heap_buffer(
    alloc: &mut Allocator,
    geometry: bool,
    index: u8,
    offset: u64,
    vertex_input: bool,
) {
    let buffer = alloc.gpu_heap_buffer.as_ref().map_or(0, Obj::handle);
    let first_stage = if geometry {
        WMT_RENDER_COMMAND_SET_OBJECT_BUFFER
    } else {
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER
    };
    encode_render_cmd(
        alloc,
        WmtcmdRenderSetbuffer {
            r#type: first_stage,
            reserved: [0; 3],
            next: null_mem_ptr(),
            buffer,
            offset,
            index,
        },
    );
    if vertex_input {
        return;
    }
    if geometry {
        encode_render_cmd(
            alloc,
            WmtcmdRenderSetbuffer {
                r#type: WMT_RENDER_COMMAND_SET_MESH_BUFFER,
                reserved: [0; 3],
                next: null_mem_ptr(),
                buffer,
                offset,
                index,
            },
        );
    }
    encode_render_cmd(
        alloc,
        WmtcmdRenderSetbuffer {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            buffer,
            offset,
            index,
        },
    );
}

/// Core `PreDraw` implementation (`d3d12_command_list.cpp:410`).
pub fn pre_draw_with_options(
    list: &mut List,
    skip_resource_binding: bool,
    indexed: bool,
) -> DrawCallStatus {
    if list.allocator.is_null() {
        return DrawCallStatus::Invalid;
    }
    // SAFETY: `list.allocator` is valid while recording.
    let alloc = unsafe { &mut *list.allocator };
    let current = alloc.encoder_current;
    let other_query_heap = list.active_visibility_offset != u64::MAX
        && list.render_pass_visibility_buffer != 0
        && !current.is_null()
        // SAFETY: `current` is non-null.
        && unsafe { (*current).kind == EncoderType::Render }
        // SAFETY: `current` is a `RenderEncoderData`.
        && unsafe { (*current.cast::<RenderEncoderData>()).visibility_buffer } != 0
        // SAFETY: `current` is a `RenderEncoderData`.
        && unsafe { (*current.cast::<RenderEncoderData>()).visibility_buffer }
            != list.render_pass_visibility_buffer;

    let need_new_pass = if current.is_null() || other_query_heap {
        true
    } else {
        // SAFETY: `current` is non-null.
        unsafe { (*current).kind != EncoderType::Render }
    };

    if need_new_pass {
        if !alloc.encoder_current.is_null() {
            alloc.invalidate_current_pass();
        }
        let render_ptr = alloc_cpu_heap(
            alloc,
            size_of::<RenderEncoderData>(),
            align_of::<RenderEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<RenderEncoderData>();
        if render_ptr.is_null() {
            return DrawCallStatus::Invalid;
        }
        let mut pass = RenderEncoderData {
            base: EncoderData::new(EncoderType::Render, 0),
            colors: list.render_pass.colors.clone(),
            depth: list.render_pass.depth.clone(),
            stencil: list.render_pass.stencil.clone(),
            default_raster_sample_count: 0,
            render_target_array_length: list.render_pass.render_target_array_length,
            render_target_height: list.render_pass.render_target_height,
            render_target_width: list.render_pass.render_target_width,
            cmd_head: WmtcmdRenderNop {
                r#type: WMT_RENDER_COMMAND_NOP,
                reserved: [0; 3],
                next: null_mem_ptr(),
            },
            cmd_tail: ptr::null_mut(),
            dsv_planar_flags: list.render_pass.dsv_planar_flags,
            dsv_readonly_flags: list.render_pass.dsv_readonly_flags,
            render_target_count: list.render_pass.render_target_count,
            use_visibility_result: false,
            visibility_buffer: 0,
            use_tessellation: false,
            use_geometry: false,
            uav_writes: false,
            pre_head: WmtcmdComputeNop {
                r#type: WMT_COMPUTE_COMMAND_NOP,
                reserved: [0; 3],
                next: null_mem_ptr(),
            },
            pre_tail: ptr::null_mut(),
        };
        pass.base.barriers = alloc.barriers;
        pass.base.barriers_last = alloc.barriers;
        pass.base.writes_unknown = list.render_pass.base.writes_unknown;
        pass.base.writes = list.render_pass.base.writes;
        pass.base.write_count = list.render_pass.base.write_count;
        // SAFETY: `render_ptr` points to freshly allocated aligned CPU heap storage.
        unsafe { ptr::write(render_ptr, pass) };
        // SAFETY: `render_ptr` was just initialized.
        let render = unsafe { &mut *render_ptr };
        render.cmd_tail = (&raw mut render.cmd_head).cast::<WmtcmdBase>();
        alloc.encoder_current = render_ptr.cast::<EncoderData>();
        list.inside_render_pass = true;
        list.bound_pso = 0;
        list.bound_dss = u64::MAX;
        list.bound_rasterizer = [u32::MAX; 7];
        list.bound_vb_slot_mask = u32::MAX;
        list.bound_geometry = false;

        if !alloc.encoder_last.is_null() && !alloc.clears_from.is_null() {
            let folded = alloc.fold_clears(render);
            if folded > 0 {
                crate::count!("#clears folded", u64::from(folded));
            }
        }
        if render.base.write_count == 0 {
            let forced = if list.pso_graphics.is_null() {
                1
            } else {
                // SAFETY: `list.pso_graphics` is a valid `Graphics`.
                unsafe { (*list.pso_graphics).forced_sample_count }.max(1)
            };
            render.default_raster_sample_count = u8::try_from(forced).unwrap_or(1);
        }
        list.dirty_state.insert(
            DirtyFlags::VERTEX_BUFFERS
                | DirtyFlags::ROOT_ARGS
                | DirtyFlags::STATIC_SAMPLERS
                | DirtyFlags::VIEWPORT
                | DirtyFlags::SCISSOR
                | DirtyFlags::BLEND_FACTOR
                | DirtyFlags::STENCIL_REF
                | DirtyFlags::PSO,
        );
        if list.active_visibility_offset != u64::MAX {
            list.dirty_state.insert(DirtyFlags::VISIBILITY_BUFFER);
        }
    }

    if list.dirty_state.contains(DirtyFlags::VISIBILITY_BUFFER) {
        // SAFETY: `alloc.encoder_current` is the active `RenderEncoderData`.
        let render = unsafe { &mut *alloc.encoder_current.cast::<RenderEncoderData>() };
        if list.active_visibility_offset != u64::MAX
            && list.render_pass_visibility_buffer != 0
            && render.visibility_buffer == 0
        {
            render.visibility_buffer = list.render_pass_visibility_buffer;
            render.use_visibility_result = true;
            alloc_writes(
                alloc,
                ptr::with_exposed_provenance(
                    usize::try_from(render.visibility_buffer).unwrap_or(0),
                ),
            );
        }
        if render.visibility_buffer != 0 {
            let (mode, offset) = if list.active_visibility_offset == u64::MAX {
                (WMT_VISIBILITY_RESULT_MODE_DISABLED, 0)
            } else {
                let is_boolean = (list.active_visibility_offset & 1) != 0;
                let mode = if is_boolean {
                    WMT_VISIBILITY_RESULT_MODE_BOOLEAN
                } else {
                    WMT_VISIBILITY_RESULT_MODE_COUNTING
                };
                (mode, list.active_visibility_offset & !1u64)
            };
            encode_render_cmd(
                alloc,
                WmtcmdRenderSetvisibilitymode {
                    r#type: WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    offset,
                    mode,
                },
            );
        }
        list.dirty_state.remove(DirtyFlags::VISIBILITY_BUFFER);
    }

    if list.pso_graphics.is_null() {
        return DrawCallStatus::Invalid;
    }
    // SAFETY: `list.pso_graphics` is a valid `Graphics`.
    let pso_gfx = unsafe { &*list.pso_graphics };
    let geometry = pso_gfx.geometry;
    let pso_handle = if geometry {
        let format = if !indexed {
            SM50_INDEX_BUFFER_FORMAT_NONE
        } else if list.index_buffer_format == u32::from(WMT_INDEX_TYPE_U_INT32) {
            SM50_INDEX_BUFFER_FORMAT_UINT32
        } else {
            SM50_INDEX_BUFFER_FORMAT_UINT16
        };
        let handle = pso_gfx.geometry_pipeline(is_strip_topology(list.primitive_topology), format);
        if handle == 0 {
            return DrawCallStatus::Invalid;
        }
        // SAFETY: `alloc.encoder_current` is the active `RenderEncoderData`.
        unsafe { (*alloc.encoder_current.cast::<RenderEncoderData>()).use_geometry = true };
        handle
    } else {
        pso_gfx.pso.as_ref().map_or(0, Obj::handle)
    };
    if pso_gfx.writes_uav {
        // SAFETY: `alloc.encoder_current` is the active `RenderEncoderData`.
        unsafe { (*alloc.encoder_current.cast::<RenderEncoderData>()).uav_writes = true };
    }
    if geometry != list.bound_geometry {
        list.bound_vb_slot_mask = u32::MAX;
        list.dirty_state.insert(
            DirtyFlags::VERTEX_BUFFERS | DirtyFlags::ROOT_ARGS | DirtyFlags::STATIC_SAMPLERS,
        );
        list.bound_geometry = geometry;
    }
    if pso_handle != list.bound_pso {
        list.dirty_state.insert(DirtyFlags::PSO);
    }

    if list.dirty_state.contains(DirtyFlags::PSO) {
        encode_render_cmd(
            alloc,
            WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: null_mem_ptr(),
                pso: pso_handle,
            },
        );
        list.bound_pso = pso_handle;
        if geometry {
            let buffer = alloc.gpu_heap_buffer.as_ref().map_or(0, Obj::handle);
            encode_render_cmd(
                alloc,
                WmtcmdRenderSetbuffer {
                    r#type: WMT_RENDER_COMMAND_SET_OBJECT_BUFFER,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    buffer,
                    offset: 0,
                    index: u8::try_from(SM50_BINDING_INDEX_DRAW_ARGUMENTS).unwrap_or(0),
                },
            );
        }
        // SAFETY: `alloc.encoder_current` is the active `RenderEncoderData`.
        let (dsv_planar, dsv_ro) = unsafe {
            let r = &*alloc.encoder_current.cast::<RenderEncoderData>();
            (
                u32::from(r.dsv_planar_flags),
                u32::from(r.dsv_readonly_flags),
            )
        };
        let dss_handle = get_depth_stencil_state(pso_gfx, dsv_planar, dsv_ro);
        if dss_handle != list.bound_dss {
            encode_render_cmd(
                alloc,
                WmtcmdRenderSetdepthstencilstate {
                    r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    depth_stencil_state: dss_handle,
                },
            );
            list.bound_dss = dss_handle;
        }
        let rast_key = [
            pso_gfx.fill_mode,
            pso_gfx.cull_mode,
            pso_gfx.depth_clip_mode,
            pso_gfx.winding,
            pso_gfx.depth_bias.to_bits(),
            pso_gfx.slope_scale.to_bits(),
            pso_gfx.depth_bias_clamp.to_bits(),
        ];
        if rast_key != list.bound_rasterizer {
            encode_render_cmd(
                alloc,
                WmtcmdRenderSetrasterizerstate {
                    r#type: WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    fill_mode: u8::try_from(pso_gfx.fill_mode).unwrap_or(0),
                    cull_mode: u8::try_from(pso_gfx.cull_mode).unwrap_or(0),
                    depth_clip_mode: u8::try_from(pso_gfx.depth_clip_mode).unwrap_or(0),
                    winding: u8::try_from(pso_gfx.winding).unwrap_or(0),
                    depth_bias: pso_gfx.depth_bias,
                    scole_scale: pso_gfx.slope_scale,
                    depth_bias_clamp: pso_gfx.depth_bias_clamp,
                },
            );
            list.bound_rasterizer = rast_key;
        }
        list.dirty_state.remove(DirtyFlags::PSO);
    }

    if list.dirty_state.contains(DirtyFlags::VERTEX_BUFFERS) {
        let slot_mask = pso_gfx.slot_mask;
        let (offset, stride) = populate_vertex_buffer_table(list, 1, 0);
        if stride != 0 {
            bind_render_heap_buffer(
                alloc,
                geometry,
                u8::try_from(SM50_BINDING_INDEX_VERTEX_BUFFER).unwrap_or(0),
                offset as u64,
                true,
            );
        }
        list.bound_vb_slot_mask = slot_mask;
        list.dirty_state.remove(DirtyFlags::VERTEX_BUFFERS);
    }

    if list.dirty_state.contains(DirtyFlags::ROOT_ARGS) && !skip_resource_binding {
        if !list.root_sig_graphics.is_null() {
            // SAFETY: `list.root_sig_graphics` is a valid `RootSignature`.
            let rs = unsafe { &*list.root_sig_graphics };
            let offset = encode_root_argument(alloc, rs, &list.root_args_graphics, 1);
            bind_render_heap_buffer(
                alloc,
                geometry,
                u8::try_from(SM50_BINDING_INDEX_ROOT_ARGUMENTS).unwrap_or(0),
                offset as u64,
                false,
            );
        }
        list.dirty_state.remove(DirtyFlags::ROOT_ARGS);
    }

    if list.dirty_state.contains(DirtyFlags::STATIC_SAMPLERS) && !skip_resource_binding {
        if !list.root_sig_graphics.is_null() {
            // SAFETY: `list.root_sig_graphics` is a valid `RootSignature`.
            let rs = unsafe { &*list.root_sig_graphics };
            if !rs.encoded_static_samplers().is_empty() {
                let offset = encode_static_samplers(alloc, rs);
                bind_render_heap_buffer(
                    alloc,
                    geometry,
                    u8::try_from(SM50_BINDING_INDEX_STATIC_SAMPLERS).unwrap_or(0),
                    offset as u64,
                    false,
                );
            }
        }
        list.dirty_state.remove(DirtyFlags::STATIC_SAMPLERS);
    }

    if list.dirty_state.contains(DirtyFlags::VIEWPORT) {
        let vp_count = usize::from(list.viewport_count);
        let vp_bytes = vp_count * size_of::<WMTViewport>();
        let vp_ptr = if vp_count > 0 {
            let p = alloc_cpu_heap(alloc, vp_bytes, align_of::<WMTViewport>())
                .cast::<c_void>()
                .cast::<WMTViewport>();
            if !p.is_null() {
                // SAFETY: `p` has space for `vp_count` `WMTViewport` elements in `cpu_heap`.
                unsafe { ptr::copy_nonoverlapping(list.viewports.as_ptr(), p, vp_count) };
            }
            p.cast::<u8>()
        } else {
            ptr::null_mut()
        };
        encode_render_cmd(
            alloc,
            WmtcmdRenderSetviewports {
                r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
                reserved: [0; 3],
                next: null_mem_ptr(),
                viewports: ptr_to_mem_ptr(vp_ptr),
                viewport_count: list.viewport_count,
            },
        );
        list.dirty_state.remove(DirtyFlags::VIEWPORT);
    }

    if list.dirty_state.contains(DirtyFlags::SCISSOR) {
        let vp_count = usize::from(list.viewport_count);
        let sc_count = usize::from(list.scissor_rect_count);
        let sc_bytes = vp_count * size_of::<WMTScissorRect>();
        let sc_ptr = if vp_count > 0 {
            let p = alloc_cpu_heap(alloc, sc_bytes, align_of::<WMTScissorRect>())
                .cast::<c_void>()
                .cast::<WMTScissorRect>();
            if !p.is_null() {
                for i in 0..vp_count {
                    let rect = if i < sc_count {
                        list.scissor_rects[i]
                    } else {
                        WMTScissorRect {
                            x: 0,
                            y: 0,
                            width: MAX_SCISSOR_DIM,
                            height: MAX_SCISSOR_DIM,
                        }
                    };
                    // SAFETY: `i < vp_count` within `p`.
                    let dst = unsafe { p.add(i) };
                    // SAFETY: `dst` points to valid aligned `WMTScissorRect` storage in `cpu_heap`.
                    unsafe { ptr::write(dst, rect) };
                }
            }
            p.cast::<u8>()
        } else {
            ptr::null_mut()
        };
        encode_render_cmd(
            alloc,
            WmtcmdRenderSetscissorrects {
                r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
                reserved: [0; 3],
                next: null_mem_ptr(),
                scissor_rects: ptr_to_mem_ptr(sc_ptr),
                rect_count: list.viewport_count,
            },
        );
        list.dirty_state.remove(DirtyFlags::SCISSOR);
    }

    if list.dirty_state.contains(DirtyFlags::BLEND_FACTOR) {
        encode_render_cmd(
            alloc,
            WmtcmdRenderSetblendcolor {
                r#type: WMT_RENDER_COMMAND_SET_BLEND_FACTOR,
                reserved: [0; 3],
                next: null_mem_ptr(),
                red: list.blend_factor[0],
                green: list.blend_factor[1],
                blue: list.blend_factor[2],
                alpha: list.blend_factor[3],
                stencil_ref: 0,
            },
        );
        list.dirty_state.remove(DirtyFlags::BLEND_FACTOR);
    }

    if list.dirty_state.contains(DirtyFlags::STENCIL_REF) {
        encode_render_cmd(
            alloc,
            WmtcmdRenderSetstencilref {
                r#type: WMT_RENDER_COMMAND_SET_STENCIL_REF,
                reserved: [0; 3],
                next: null_mem_ptr(),
                stencil_ref: u8::try_from(list.stencil_ref & 0xff).unwrap_or(0),
            },
        );
        list.dirty_state.remove(DirtyFlags::STENCIL_REF);
    }

    if geometry {
        DrawCallStatus::Geometry
    } else {
        DrawCallStatus::Valid
    }
}

/// Flushes dirty graphics state into the active [`RenderEncoderData`] (`PreDraw`,
///
/// `d3d12_command_list.cpp:410`).
pub fn pre_draw(list: &mut List) -> DrawCallStatus {
    pre_draw_with_options(list, false, false)
}

/// Core `PreDispatch` implementation (`d3d12_command_list.cpp:757`).
pub fn pre_dispatch_with_options(list: &mut List, skip_resource_binding: bool) -> DrawCallStatus {
    if list.allocator.is_null() {
        return DrawCallStatus::Invalid;
    }
    // SAFETY: `list.allocator` is valid while recording.
    let alloc = unsafe { &mut *list.allocator };
    let need_new_pass = if alloc.encoder_current.is_null() {
        true
    } else {
        // SAFETY: `alloc.encoder_current` is non-null.
        unsafe { (*alloc.encoder_current).kind != EncoderType::Compute }
    };
    if need_new_pass {
        if !alloc.encoder_current.is_null() {
            alloc.invalidate_current_pass();
        }
        list.inside_render_pass = false;
        let compute_ptr = alloc_cpu_heap(
            alloc,
            size_of::<ComputeEncoderData>(),
            align_of::<ComputeEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<ComputeEncoderData>();
        if compute_ptr.is_null() {
            return DrawCallStatus::Invalid;
        }
        let mut base = EncoderData::new(EncoderType::Compute, 0);
        base.barriers = alloc.barriers;
        base.barriers_last = alloc.barriers;
        base.writes_unknown = false;
        let pass = ComputeEncoderData {
            base,
            cmd_head: WmtcmdComputeNop {
                r#type: WMT_COMPUTE_COMMAND_NOP,
                reserved: [0; 3],
                next: null_mem_ptr(),
            },
            cmd_tail: ptr::null_mut(),
        };
        // SAFETY: `compute_ptr` points to freshly allocated aligned CPU heap storage.
        unsafe { ptr::write(compute_ptr, pass) };
        // SAFETY: `compute_ptr` was just initialized.
        let compute = unsafe { &mut *compute_ptr };
        compute.cmd_tail = (&raw mut compute.cmd_head).cast::<WmtcmdBase>();
        alloc.encoder_current = compute_ptr.cast::<EncoderData>();
        list.dirty_state.insert(
            DirtyFlags::COMPUTE_ROOT_ARGS | DirtyFlags::COMPUTE_STATIC_SAMPLERS | DirtyFlags::PSO,
        );
    }

    if list.pso_compute.is_null() {
        return DrawCallStatus::Invalid;
    }
    // SAFETY: `list.pso_compute` is a valid `Compute`.
    let pso_comp = unsafe { &*list.pso_compute };

    if list.dirty_state.contains(DirtyFlags::PSO) {
        encode_compute_cmd(
            alloc,
            WmtcmdComputeSetpso {
                r#type: WMT_COMPUTE_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: null_mem_ptr(),
                pso: pso_comp.pso.as_ref().map_or(0, Obj::handle),
                threadgroup_size: pso_comp.threadgroup_size,
            },
        );
        list.dirty_state.remove(DirtyFlags::PSO);
    }

    let gpu_buf = alloc.gpu_heap_buffer.as_ref().map_or(0, Obj::handle);

    if list.dirty_state.contains(DirtyFlags::COMPUTE_ROOT_ARGS) && !skip_resource_binding {
        if !list.root_sig_compute.is_null() {
            // SAFETY: `list.root_sig_compute` is a valid `RootSignature`.
            let rs = unsafe { &*list.root_sig_compute };
            let offset = encode_root_argument(alloc, rs, &list.root_args_compute, 1);
            encode_compute_cmd(
                alloc,
                WmtcmdComputeSetbuffer {
                    r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    buffer: gpu_buf,
                    offset: offset as u64,
                    index: u8::try_from(SM50_BINDING_INDEX_ROOT_ARGUMENTS).unwrap_or(0),
                },
            );
        }
        list.dirty_state.remove(DirtyFlags::COMPUTE_ROOT_ARGS);
    }

    if list
        .dirty_state
        .contains(DirtyFlags::COMPUTE_STATIC_SAMPLERS)
        && !skip_resource_binding
    {
        if !list.root_sig_compute.is_null() {
            // SAFETY: `list.root_sig_compute` is a valid `RootSignature`.
            let rs = unsafe { &*list.root_sig_compute };
            if !rs.encoded_static_samplers().is_empty() {
                let offset = encode_static_samplers(alloc, rs);
                encode_compute_cmd(
                    alloc,
                    WmtcmdComputeSetbuffer {
                        r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
                        reserved: [0; 3],
                        next: null_mem_ptr(),
                        buffer: gpu_buf,
                        offset: offset as u64,
                        index: u8::try_from(SM50_BINDING_INDEX_STATIC_SAMPLERS).unwrap_or(0),
                    },
                );
            }
        }
        list.dirty_state.remove(DirtyFlags::COMPUTE_STATIC_SAMPLERS);
    }

    DrawCallStatus::Valid
}

/// Flushes dirty compute state into the active [`ComputeEncoderData`] (`PreDispatch`,
///
/// `d3d12_command_list.cpp:757`).
pub fn pre_dispatch(list: &mut List) -> DrawCallStatus {
    pre_dispatch_with_options(list, false)
}

/// `ID3D12GraphicsCommandList::DrawInstanced` (`d3d12_command_list.cpp:661`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn draw_instanced(
    this: *mut c_void,
    vertex_count_per_instance: u32,
    instance_count: u32,
    start_vertex_location: u32,
    start_instance_location: u32,
) {
    crate::scope!("list.DrawInstanced");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some((primitive_type, _cp_count)) = to_metal_primitive_type(list.primitive_topology) else {
        return;
    };
    let status = pre_draw_with_options(list, false, false);
    if status == DrawCallStatus::Invalid || vertex_count_per_instance == 0 || instance_count == 0 {
        return;
    }
    // SAFETY: `list.allocator` is valid after `pre_draw_with_options` succeeded.
    let alloc = unsafe { &mut *list.allocator };
    if status == DrawCallStatus::Geometry {
        let (vertex_per_warp, increment) = geometry_warp(list.primitive_topology);
        if increment == 0 {
            return;
        }
        let (args_ptr, offset) = alloc_gpu_heap(alloc, size_of::<D3d12DrawArguments>(), 32);
        if args_ptr.is_null() {
            return;
        }
        let dst = args_ptr.cast::<c_void>().cast::<D3d12DrawArguments>();
        // SAFETY: `dst` points to 32-byte aligned storage for `D3d12DrawArguments` in `gpu_heap`.
        unsafe {
            ptr::write(
                dst,
                D3d12DrawArguments {
                    vertex_count_per_instance,
                    instance_count,
                    start_vertex_location,
                    start_instance_location,
                },
            );
        }
        encode_render_cmd(
            alloc,
            WmtcmdRenderDxmtGeometryDraw {
                r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW,
                reserved: [0; 3],
                next: null_mem_ptr(),
                draw_arguments_offset: offset as u64,
                warp_count: (vertex_count_per_instance - 1) / increment + 1,
                instance_count,
                vertex_per_warp,
            },
        );
        return;
    }

    encode_render_cmd(
        alloc,
        WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: null_mem_ptr(),
            primitive_type,
            vertex_start: u64::from(start_vertex_location),
            vertex_count: u64::from(vertex_count_per_instance),
            instance_count,
            base_instance: start_instance_location,
        },
    );
}

/// `ID3D12GraphicsCommandList::DrawIndexedInstanced` (`d3d12_command_list.cpp:696`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn draw_indexed_instanced(
    this: *mut c_void,
    index_count_per_instance: u32,
    instance_count: u32,
    start_index_location: u32,
    base_vertex_location: i32,
    start_instance_location: u32,
) {
    crate::scope!("list.DrawIndexedInstanced");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some((primitive_type, _cp_count)) = to_metal_primitive_type(list.primitive_topology) else {
        return;
    };
    let status = pre_draw_with_options(list, false, true);
    if status == DrawCallStatus::Invalid || index_count_per_instance == 0 || instance_count == 0 {
        return;
    }
    // SAFETY: `list.allocator` is valid after `pre_draw_with_options` succeeded.
    let alloc = unsafe { &mut *list.allocator };
    if status == DrawCallStatus::Geometry {
        let (vertex_per_warp, increment) = geometry_warp(list.primitive_topology);
        if increment == 0 {
            return;
        }
        let (args_ptr, offset) = alloc_gpu_heap(alloc, size_of::<D3d12DrawIndexedArguments>(), 32);
        if args_ptr.is_null() {
            return;
        }
        let dst = args_ptr
            .cast::<c_void>()
            .cast::<D3d12DrawIndexedArguments>();
        // SAFETY: `dst` points to 32-byte aligned storage for `D3d12DrawIndexedArguments`.
        unsafe {
            ptr::write(
                dst,
                D3d12DrawIndexedArguments {
                    index_count_per_instance,
                    instance_count,
                    start_index_location,
                    base_vertex_location,
                    start_instance_location,
                },
            );
        }
        encode_render_cmd(
            alloc,
            WmtcmdRenderDxmtGeometryDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
                reserved: [0; 3],
                next: null_mem_ptr(),
                draw_arguments_offset: offset as u64,
                index_buffer: list.index_buffer,
                index_buffer_offset: list.index_buffer_offset,
                warp_count: (index_count_per_instance - 1) / increment + 1,
                instance_count,
                vertex_per_warp,
            },
        );
        return;
    }

    let index_stride: u64 = if list.index_buffer_format == u32::from(WMT_INDEX_TYPE_U_INT32) {
        4
    } else {
        2
    };
    encode_render_cmd(
        alloc,
        WmtcmdRenderDrawIndexed {
            r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
            reserved: [0; 3],
            next: null_mem_ptr(),
            primitive_type,
            index_type: u8::try_from(list.index_buffer_format).unwrap_or(0),
            index_count: u64::from(index_count_per_instance),
            index_buffer: list.index_buffer,
            index_buffer_offset: list
                .index_buffer_offset
                .saturating_add(u64::from(start_index_location) * index_stride),
            instance_count,
            base_vertex: base_vertex_location,
            base_instance: start_instance_location,
        },
    );
}

/// `ID3D12GraphicsCommandList::Dispatch` (`d3d12_command_list.cpp:809`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn dispatch(this: *mut c_void, x: u32, y: u32, z: u32) {
    crate::scope!("list.Dispatch");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    if pre_dispatch_with_options(list, false) == DrawCallStatus::Invalid {
        return;
    }
    // SAFETY: `list.allocator` is valid after `pre_dispatch_with_options` succeeded.
    let alloc = unsafe { &mut *list.allocator };
    encode_compute_cmd(
        alloc,
        WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH,
            reserved: [0; 3],
            next: null_mem_ptr(),
            size: WMTSize {
                width: u64::from(x),
                height: u64::from(y),
                depth: u64::from(z),
            },
        },
    );
}

/// `ID3D12GraphicsCommandList4::InitializeMetaCommand` (`d3d12_command_list.cpp:2292`).
///
/// # Safety
///
/// `this`, `meta_command`, and `initialization_parameters_data` are caller-supplied pointers.
pub unsafe fn initialize_meta_command(
    _this: *mut c_void,
    _meta_command: *mut c_void,
    _initialization_parameters_data: *const c_void,
    _initialization_parameters_data_size_in_bytes: usize,
) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.InitializeMetaCommand");
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!(
            "InitializeMetaCommand: meta commands not supported (EnumerateMetaCommands reports none)"
        );
    }
}

/// `ID3D12GraphicsCommandList4::ExecuteMetaCommand` (`d3d12_command_list.cpp:2298`).
///
/// # Safety
///
/// `this`, `meta_command`, and `execution_parameters_data` are caller-supplied pointers.
pub unsafe fn execute_meta_command(
    _this: *mut c_void,
    _meta_command: *mut c_void,
    _execution_parameters_data: *const c_void,
    _execution_parameters_data_size_in_bytes: usize,
) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.ExecuteMetaCommand");
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!(
            "ExecuteMetaCommand: meta commands not supported (EnumerateMetaCommands reports none)"
        );
    }
}

/// `ID3D12GraphicsCommandList4::BuildRaytracingAccelerationStructure`
///
/// (`d3d12_command_list.cpp:2304`).
///
/// # Safety
///
/// `this`, `desc`, and `postbuild_info_descs` are caller-supplied pointers.
pub unsafe fn build_raytracing_acceleration_structure(
    _this: *mut c_void,
    _desc: *const c_void,
    _num_postbuild_info_descs: u32,
    _postbuild_info_descs: *const c_void,
) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.BuildRaytracingAccelerationStructure");
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!(
            "BuildRaytracingAccelerationStructure: ray tracing not supported (RaytracingTier NOT_SUPPORTED)"
        );
    }
}

/// `ID3D12GraphicsCommandList4::EmitRaytracingAccelerationStructurePostbuildInfo`
///
/// (`d3d12_command_list.cpp:2313`).
///
/// # Safety
///
/// `this`, `desc`, and `source_acceleration_structure_data` are caller-supplied pointers.
pub unsafe fn emit_raytracing_acceleration_structure_postbuild_info(
    _this: *mut c_void,
    _desc: *const c_void,
    _num_source_acceleration_structures: u32,
    _source_acceleration_structure_data: *const u64,
) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.EmitRaytracingAccelerationStructurePostbuildInfo");
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!(
            "EmitRaytracingAccelerationStructurePostbuildInfo: ray tracing not supported (RaytracingTier NOT_SUPPORTED)"
        );
    }
}

/// `ID3D12GraphicsCommandList4::CopyRaytracingAccelerationStructure`
///
/// (`d3d12_command_list.cpp:2322`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn copy_raytracing_acceleration_structure(
    _this: *mut c_void,
    _dest_acceleration_structure_data: u64,
    _source_acceleration_structure_data: u64,
    _mode: u32,
) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.CopyRaytracingAccelerationStructure");
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!(
            "CopyRaytracingAccelerationStructure: ray tracing not supported (RaytracingTier NOT_SUPPORTED)"
        );
    }
}

/// `ID3D12GraphicsCommandList4::DispatchRays` (`d3d12_command_list.cpp:2337`).
///
/// # Safety
///
/// `this` and `desc` are caller-supplied pointers.
pub unsafe fn dispatch_rays(_this: *mut c_void, _desc: *const c_void) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.DispatchRays");
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!("DispatchRays: ray tracing not supported (RaytracingTier NOT_SUPPORTED)");
    }
}

/// `ID3D12GraphicsCommandList6::DispatchMesh` (`d3d12_command_list.cpp:2357`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn dispatch_mesh(_this: *mut c_void, _x: u32, _y: u32, _z: u32) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.DispatchMesh");
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!("DispatchMesh: mesh shaders not supported (MeshShaderTier NOT_SUPPORTED)");
    }
}

#[cfg(test)]
mod tests;
