//! `ExecuteIndirect` (`d3d12_command_list.cpp:1992`): fast direct path and ICB resolver path.

use core::{ffi::c_void, mem::size_of, ptr};
use std::sync::LazyLock;

use winemetal_wire::{
    MemPtr, WMT_INDEX_TYPE_U_INT32, WMT_PRIMITIVE_TYPE_LINE, WMT_PRIMITIVE_TYPE_LINE_STRIP,
    WMT_PRIMITIVE_TYPE_POINT, WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP,
    WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT, WMT_RENDER_COMMAND_DRAW_INDIRECT, WmtcmdBase,
    WmtcmdRenderDrawIndexedIndirect, WmtcmdRenderDrawIndirect,
};

use crate::{
    abi::list_ops::{
        D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH, D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED,
    },
    allocator::Allocator,
    command_signature::CommandSignature,
    device,
    encoder::RenderEncoderData,
    list::{DrawCallStatus, List},
    resource::Resource,
    root_signature::object::RootSignature,
};

#[cfg(target_pointer_width = "64")]
const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0 }
}

#[cfg(target_pointer_width = "32")]
const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0, high: 0 }
}

const DXGI_FORMAT_R32_UINT: u32 = 42;
const DXGI_FORMAT_R16_UINT: u32 = 57;

static NATIVE_INDIRECT: LazyLock<bool> =
    LazyLock::new(|| std::env::var("DXMT_D3D12_INDIRECT").as_deref() != Ok("icb"));

#[repr(C)]
struct VertexBufferEntry {
    buffer_handle: u64,
    stride: u32,
    length: u32,
}

#[cfg(target_pointer_width = "64")]
fn ptr_to_mem_ptr<T>(p: *mut T) -> MemPtr {
    MemPtr {
        ptr: p.expose_provenance() as u64,
    }
}

#[cfg(target_pointer_width = "32")]
fn ptr_to_mem_ptr<T>(p: *mut T) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(p.expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

fn encode_render_cmd<T>(alloc: &mut Allocator, mut cmd: T) {
    if alloc.encoder_current.is_null() {
        return;
    }
    // SAFETY: `alloc.encoder_current` is a `RenderEncoderData` ensured by `pre_draw`.
    let enc = unsafe { &mut *alloc.encoder_current.cast::<RenderEncoderData>() };
    let storage = alloc
        .allocate_cpu_heap(size_of::<T>(), 16)
        .cast::<c_void>()
        .cast::<T>();
    if storage.is_null() {
        return;
    }
    // SAFETY: `cmd` starts with `WmtcmdBase` at offset 0.
    let base = unsafe { &mut *(&raw mut cmd).cast::<WmtcmdBase>() };
    base.next = null_mem_ptr();
    // SAFETY: `storage` points to freshly allocated aligned CPU heap storage for `T`.
    unsafe { ptr::write(storage, cmd) };
    let storage_base = storage.cast::<WmtcmdBase>();
    // SAFETY: `enc.cmd_tail` points to a valid command node in the CPU heap.
    unsafe { (*enc.cmd_tail).next = ptr_to_mem_ptr(storage_base) };
    enc.cmd_tail = storage_base;
    enc.base.barriers_last = alloc.barriers;
}

const fn to_metal_primitive_type(topology: u32) -> Option<u8> {
    match topology {
        1 | 10..=13 | 33..=64 => Some(WMT_PRIMITIVE_TYPE_POINT),
        2 => Some(WMT_PRIMITIVE_TYPE_LINE),
        3 => Some(WMT_PRIMITIVE_TYPE_LINE_STRIP),
        4 => Some(WMT_PRIMITIVE_TYPE_TRIANGLE),
        5 => Some(WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP),
        _ => None,
    }
}

fn build_root_arg_staging(list: &List, root_sig: &RootSignature, args: &[u64; 64]) -> [u64; 67] {
    let mut staging = [0u64; 67];
    staging[..64].copy_from_slice(args);
    let heap_off = root_sig.heap_qword_offset();
    if heap_off != u32::MAX {
        let h = heap_off as usize;
        if h + 1 < staging.len() {
            if !list.bound_cbv_heap.is_null() {
                // SAFETY: `list.bound_cbv_heap` is a valid `Heap` pointer when non-null.
                staging[h] = unsafe { (*list.bound_cbv_heap).gpu_address() };
            }
            if !list.bound_sampler_heap.is_null() {
                // SAFETY: `list.bound_sampler_heap` is a valid `Heap` pointer when non-null.
                staging[h + 1] = unsafe { (*list.bound_sampler_heap).gpu_address() };
            }
        }
    }
    let lock_off = root_sig.lock_qword_offset() as usize;
    if lock_off < staging.len() && !list.device.is_null() {
        // SAFETY: `list.device` is a valid `Device` pointer when non-null.
        staging[lock_off] = unsafe { (*list.device).lock_table() };
    }
    staging
}

fn encode_root_argument(
    alloc: &mut Allocator,
    root_sig: &RootSignature,
    staging: &[u64; 67],
    count: u32,
) -> u64 {
    let qwords = (root_sig.upload_qwords() as usize).min(staging.len());
    let stride_bytes = qwords * size_of::<u64>();
    let total_bytes = stride_bytes.saturating_mul(count as usize);
    let (ptr, offset) = alloc.allocate_gpu_heap(total_bytes, 64);
    if !ptr.is_null() && stride_bytes > 0 {
        for i in 0..(count as usize) {
            // SAFETY: `ptr` has `total_bytes` writable bytes in the GPU heap.
            let dst = unsafe { ptr.add(i * stride_bytes) };
            // SAFETY: `staging` has at least `stride_bytes` readable bytes.
            unsafe {
                ptr::copy_nonoverlapping(staging.as_ptr().cast::<u8>(), dst, stride_bytes);
            }
        }
    }
    offset as u64
}

fn encode_static_samplers(list: &mut List, alloc: &mut Allocator, root_sig: &RootSignature) -> u64 {
    super::draw::encode_static_samplers_cached(list, alloc, root_sig) as u64
}

fn populate_vertex_buffer_table(
    list: &List,
    alloc: &mut Allocator,
    count: u32,
    mut null_vb: u64,
) -> (u64, u32) {
    let slot_mask = if list.pso_graphics.is_null() {
        0
    } else {
        // SAFETY: `list.pso_graphics` is a valid `Graphics` pointer when non-null.
        unsafe { (*list.pso_graphics).slot_mask }
    };
    if slot_mask == 0 {
        return (0, 0);
    }
    let max_slot = (32 - slot_mask.leading_zeros()) as usize;
    let stride = (size_of::<VertexBufferEntry>() * max_slot + 15) & !15;
    let total_bytes = stride.saturating_mul(count as usize);
    let (mapped, offset) = alloc.allocate_gpu_heap(total_bytes, 16);
    if !mapped.is_null() {
        for i in 0..(count as usize) {
            // SAFETY: `mapped` has `total_bytes` writable bytes in the GPU heap.
            let entries = unsafe {
                mapped
                    .add(i * stride)
                    .cast::<c_void>()
                    .cast::<VertexBufferEntry>()
            };
            let mut index = 0usize;
            for slot in 0..max_slot {
                if (slot_mask & (1u32 << slot)) == 0 {
                    continue;
                }
                let (buffer_location, vb_stride, vb_length) = list.vertex_buffers[slot];
                let entry = if buffer_location == 0 {
                    if null_vb == 0 && !list.device.is_null() {
                        // SAFETY: `list.device` is a valid `Device` pointer when non-null.
                        null_vb = device::null::null_vertex_buffer(unsafe { &*list.device });
                    }
                    VertexBufferEntry {
                        buffer_handle: null_vb,
                        stride: 0,
                        length: 0,
                    }
                } else {
                    VertexBufferEntry {
                        buffer_handle: buffer_location,
                        stride: vb_stride,
                        length: vb_length,
                    }
                };
                // SAFETY: `index < max_slot` so `entries.add(index)` is in-bounds.
                let dst = unsafe { entries.add(index) };
                // SAFETY: `dst` is aligned and writable in the GPU heap.
                unsafe { ptr::write(dst, entry) };
                index += 1;
            }
        }
    }
    (offset as u64, u32::try_from(stride).unwrap_or(0))
}

fn resolve_index_buffer_va(list: &List) -> u64 {
    if list.index_buffer == 0 {
        return 0;
    }
    if list.index_buffer_size > u64::from(u32::MAX) {
        return list.index_buffer_size;
    }
    if !list.device.is_null() {
        // SAFETY: `list.device` is a valid `Device` pointer when non-null.
        let dev = unsafe { &*list.device };
        if let Ok(map) = dev.residency.va_map.read() {
            for (&base_va, buffers) in map.iter() {
                for buf in buffers {
                    if let Some(alloc) = buf.current()
                        && alloc.buffer() == list.index_buffer
                    {
                        return base_va.wrapping_add(list.index_buffer_offset);
                    }
                }
            }
        }
    }
    list.index_buffer_size
}

/// `ID3D12GraphicsCommandList::ExecuteIndirect` (`d3d12_command_list.cpp:1992`).
///
/// # Safety
///
/// `this`, `command_signature`, `argument_buffer`, and `count_buffer` are caller-supplied pointers.
pub unsafe fn execute_indirect(
    this: *mut c_void,
    command_signature: *mut c_void,
    max_command_count: u32,
    argument_buffer: *mut c_void,
    argument_buffer_offset: u64,
    count_buffer: *mut c_void,
    count_buffer_offset: u64,
) {
    crate::scope!("list.ExecuteIndirect");
    if this.is_null() || command_signature.is_null() || argument_buffer.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `command_signature` was checked non-null above.
    let sig = unsafe { &*command_signature.cast::<CommandSignature>() };
    // SAFETY: `argument_buffer` was checked non-null above.
    let arg_res = unsafe { &*argument_buffer.cast::<Resource>() };
    let Some(arg_buf) = arg_res.buffer.as_ref() else {
        return;
    };
    let Some(arg_alloc) = arg_buf.current() else {
        return;
    };
    let arg_buffer_address = arg_alloc.gpu_address().wrapping_add(argument_buffer_offset);

    let count_buffer_address = if count_buffer.is_null() {
        0u64
    } else {
        // SAFETY: `count_buffer` was checked non-null above.
        let count_res = unsafe { &*count_buffer.cast::<Resource>() };
        let Some(cnt_buf) = count_res.buffer.as_ref() else {
            return;
        };
        let Some(cnt_alloc) = cnt_buf.current() else {
            return;
        };
        cnt_alloc.gpu_address().wrapping_add(count_buffer_offset)
    };

    if sig.command_type != D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH && !list.pso_graphics.is_null() {
        // SAFETY: `list.pso_graphics` is a valid `Graphics` pointer when non-null.
        if unsafe { (*list.pso_graphics).geometry } {
            crate::err!("ExecuteIndirect: draws with a geometry shader not supported");
            return;
        }
    }

    if sig.command_type == D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH {
        if super::draw::pre_dispatch_with_options(list, sig.update_root_arguments)
            == DrawCallStatus::Invalid
            || list.pso_compute.is_null()
        {
            return;
        }
        // SAFETY: `list.pso_compute` was checked non-null above.
        let pso_compute = unsafe { &*list.pso_compute };
        // SAFETY: `list.allocator` was checked non-null above.
        let alloc = unsafe { &mut *list.allocator };
        let cmd_ptr =
            alloc.encode_indirect_compute_command(sig, pso_compute, max_command_count as usize);
        if cmd_ptr.is_null() {
            return;
        }
        // SAFETY: `cmd_ptr` points to a valid `IndirectComputeCommandData` in the GPU heap.
        let cmd = unsafe { &mut *cmd_ptr };
        cmd.max_count_buffer = count_buffer_address;
        cmd.argument_buffer = arg_buffer_address;

        if sig.update_root_arguments && !list.root_sig_compute.is_null() {
            // SAFETY: `list.root_sig_compute` is a valid `RootSignature` pointer when non-null.
            let root_sig = unsafe { &*list.root_sig_compute };
            let staging = build_root_arg_staging(list, root_sig, &list.root_args_compute);
            let qwords_off = encode_root_argument(alloc, root_sig, &staging, max_command_count);
            cmd.rootsig_qwords = alloc.gpu_heap_buffer_address.wrapping_add(qwords_off);
            cmd.rootsig_qwords_stride = root_sig.upload_qwords();
            let samplers_off = encode_static_samplers(list, alloc, root_sig);
            cmd.static_samplers = alloc.gpu_heap_buffer_address.wrapping_add(samplers_off);
        }
        return;
    }

    let Some(primitive_type) = to_metal_primitive_type(list.primitive_topology) else {
        return;
    };
    let encode_binding =
        sig.update_root_arguments || sig.update_index_buffer || sig.update_vertex_buffers;
    let status = super::draw::pre_draw_with_options(list, encode_binding, false);
    if status == DrawCallStatus::Invalid {
        return;
    }
    if status != DrawCallStatus::Valid {
        crate::err!("ExecuteIndirect: draws with a geometry shader not supported");
        return;
    }

    if *NATIVE_INDIRECT && !encode_binding && count_buffer.is_null() {
        let indexed = sig.command_type == D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED;
        if indexed && list.index_buffer == 0 {
            return;
        }
        crate::count!("#ExecuteIndirect native", 1);
        let args = arg_alloc.buffer();
        // SAFETY: `list.allocator` was checked non-null above.
        let alloc = unsafe { &mut *list.allocator };
        for i in 0..max_command_count {
            let offset = argument_buffer_offset
                .wrapping_add(u64::from(i).wrapping_mul(u64::from(sig.byte_stride)));
            if indexed {
                encode_render_cmd(
                    alloc,
                    WmtcmdRenderDrawIndexedIndirect {
                        r#type: WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT,
                        reserved: [0; 3],
                        next: null_mem_ptr(),
                        primitive_type,
                        index_type: u8::try_from(list.index_buffer_format).unwrap_or(0),
                        index_buffer: list.index_buffer,
                        index_buffer_offset: list.index_buffer_offset,
                        indirect_args_buffer: args,
                        indirect_args_offset: offset,
                    },
                );
            } else {
                encode_render_cmd(
                    alloc,
                    WmtcmdRenderDrawIndirect {
                        r#type: WMT_RENDER_COMMAND_DRAW_INDIRECT,
                        reserved: [0; 3],
                        next: null_mem_ptr(),
                        primitive_type,
                        indirect_args_buffer: args,
                        indirect_args_offset: offset,
                    },
                );
            }
        }
        return;
    }

    if list.pso_graphics.is_null() {
        return;
    }
    // SAFETY: `list.pso_graphics` was checked non-null above.
    let pso_graphics = unsafe { &*list.pso_graphics };
    let index_va = resolve_index_buffer_va(list);
    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };
    let cmd_ptr =
        alloc.encode_indirect_render_command(sig, pso_graphics, max_command_count as usize);
    if cmd_ptr.is_null() {
        return;
    }
    // SAFETY: `cmd_ptr` points to a valid `IndirectRenderCommandData` in the GPU heap.
    let cmd = unsafe { &mut *cmd_ptr };
    cmd.max_count_buffer = count_buffer_address;
    cmd.argument_buffer = arg_buffer_address;
    cmd.primitive_type = u32::from(primitive_type);
    cmd.index_buffer = index_va;
    cmd.index_buffer_format = if list.index_buffer_format == u32::from(WMT_INDEX_TYPE_U_INT32) {
        DXGI_FORMAT_R32_UINT
    } else {
        DXGI_FORMAT_R16_UINT
    };
    if !encode_binding {
        return;
    }
    if !list.root_sig_graphics.is_null() {
        // SAFETY: `list.root_sig_graphics` is a valid `RootSignature` pointer when non-null.
        let root_sig = unsafe { &*list.root_sig_graphics };
        let staging = build_root_arg_staging(list, root_sig, &list.root_args_graphics);
        let qwords_off = encode_root_argument(alloc, root_sig, &staging, max_command_count);
        cmd.rootsig_qwords = alloc.gpu_heap_buffer_address.wrapping_add(qwords_off);
        cmd.rootsig_qwords_stride = root_sig.upload_qwords();
        let samplers_off = encode_static_samplers(list, alloc, root_sig);
        cmd.static_samplers = alloc.gpu_heap_buffer_address.wrapping_add(samplers_off);
    }
    cmd.null_vertex_buffer = if list.device.is_null() {
        0
    } else {
        // SAFETY: `list.device` is a valid `Device` pointer when non-null.
        device::null::null_vertex_buffer(unsafe { &*list.device })
    };
    let (vb_offset, vb_stride) =
        populate_vertex_buffer_table(list, alloc, max_command_count, cmd.null_vertex_buffer);
    cmd.vertex_buffer = alloc.gpu_heap_buffer_address.wrapping_add(vb_offset);
    cmd.vertex_argbuf_stride = vb_stride;
    cmd.vertex_slot_mask = pso_graphics.slot_mask;
}
