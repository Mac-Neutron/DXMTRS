use core::{alloc::Layout, ffi::c_void, ptr};
use std::{alloc, sync::Arc};

use dxmt_core::{
    resource::{Buffer, BufferAllocationFlags},
    settings::{Flags, settings},
};
use dxmt_util::{
    com::{ComObject, S_OK},
    private_data::PrivateData,
};
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT, WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER,
    WMT_COMPUTE_COMMAND_SET_PSO, WMTSize, WmtcmdBase, WmtcmdComputeDispatchIndirect,
    WmtcmdComputeSetpso,
};

use super::*;
use crate::{
    abi::{
        list_state::D3D12_COMMAND_LIST_TYPE_DIRECT,
        pipeline::SampleDesc,
        resource::{D3D12_RESOURCE_DIMENSION_BUFFER, HeapProperties, ResourceDesc},
        vtbl::{
            ID3D12CommandAllocatorVtbl, ID3D12CommandSignatureVtbl, ID3D12PipelineStateVtbl,
            ID3D12Resource2Vtbl,
        },
    },
    allocator::{Allocator, CPU_HEAP_SIZE, GPU_HEAP_SIZE},
    com::{IID_ID3D12_GRAPHICS_COMMAND_LIST, add_ref_com, release_com},
    command_signature::CommandSignature,
    encoder::{ComputeEncoderData, EncoderType},
    list::{
        LIST_VTBL, List,
        state::{close, create_command_list},
    },
    pipeline::Compute,
    resource::Resource,
};

const CPU_ALIGN: usize = 4096;
const GPU_ALIGN: usize = 16384;

fn new_test_allocator() -> *mut Allocator {
    let cpu_layout = Layout::from_size_align(CPU_HEAP_SIZE, CPU_ALIGN).expect("valid CPU layout");
    let gpu_layout = Layout::from_size_align(GPU_HEAP_SIZE, GPU_ALIGN).expect("valid GPU layout");
    // SAFETY: `cpu_layout` has non-zero size.
    let cpu_heap = unsafe { alloc::alloc(cpu_layout) };
    // SAFETY: `gpu_layout` has non-zero size.
    let gpu_heap = unsafe { alloc::alloc(gpu_layout) };
    assert!(!cpu_heap.is_null());
    assert!(!gpu_heap.is_null());

    // SAFETY: `LIST_VTBL` is larger than `ID3D12CommandAllocatorVtbl` and consists of non-null
    // function pointers; tests never dispatch through allocator vtable slots.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12CommandAllocatorVtbl>() };
    let raw = Box::into_raw(Box::new(Allocator {
        vtbl,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        list_type: D3D12_COMMAND_LIST_TYPE_DIRECT,
        spilled_cpu_heap: Vec::with_capacity(4),
        cpu_heap,
        cpu_heap_offset: 0,
        gpu_heap_buffer: None,
        gpu_heap,
        gpu_heap_offset: 0,
        gpu_heap_buffer_address: 0x1000_0000,
        encoder_last: ptr::null_mut(),
        encoder_current: ptr::null_mut(),
        encoder_count: 0,
        barriers: 0,
        joins: Vec::with_capacity(16),
        group: Vec::with_capacity(16),
        deps: Vec::with_capacity(16),
        covered: Vec::with_capacity(16),
        transitions: Vec::with_capacity(16),
        clears_from: ptr::null_mut(),
        clears: Vec::with_capacity(8),
        kept: Vec::with_capacity(8),
        encoder_lists: Vec::with_capacity(4),
        icb: Vec::new(),
        icb_free: FxHashMap::default(),
        kept_textures: Vec::new(),
        clear_uav: None,
        clear_rtv: None,
        copy_temp_allocator: None,
        copy_temp_version: 0,
    }));
    // SAFETY: `raw` is a freshly allocated `Allocator`; retain the test caller's public reference.
    unsafe { add_ref_com(raw) };
    raw
}

unsafe fn free_test_allocator(alloc_ptr: *mut Allocator) {
    // SAFETY: `alloc_ptr` was created by `new_test_allocator`.
    let alloc = unsafe { &mut *alloc_ptr };
    let cpu_layout = Layout::from_size_align(CPU_HEAP_SIZE, CPU_ALIGN).expect("valid CPU layout");
    let gpu_layout = Layout::from_size_align(GPU_HEAP_SIZE, GPU_ALIGN).expect("valid GPU layout");
    for spilled in alloc.spilled_cpu_heap.drain(..) {
        if !spilled.is_null() {
            // SAFETY: `spilled` was allocated with `cpu_layout`.
            unsafe { alloc::dealloc(spilled, cpu_layout) };
        }
    }
    if !alloc.cpu_heap.is_null() {
        // SAFETY: `alloc.cpu_heap` was allocated with `cpu_layout`.
        unsafe { alloc::dealloc(alloc.cpu_heap, cpu_layout) };
        alloc.cpu_heap = ptr::null_mut();
    }
    if !alloc.gpu_heap.is_null() {
        // SAFETY: `alloc.gpu_heap` was allocated with `gpu_layout`.
        unsafe { alloc::dealloc(alloc.gpu_heap, gpu_layout) };
        alloc.gpu_heap = ptr::null_mut();
    }
    alloc.encoder_lists.clear();
    // SAFETY: release the test caller's public reference acquired in `new_test_allocator`.
    unsafe { release_com(alloc_ptr) };
}

fn new_test_compute_pso() -> Compute {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12PipelineStateVtbl` and consists of non-null
    // function pointers; tests never dispatch through PSO vtable slots.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12PipelineStateVtbl>() };
    // SAFETY: `crate::tests` provides no-op `NSObject_retain`/`NSObject_release` stubs.
    let pso = unsafe { Obj::adopt(0x5500) };
    Compute {
        vtbl,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        is_compute: true,
        desc_hash: 0,
        pso,
        threadgroup_size: WMTSize {
            width: 4,
            height: 1,
            depth: 1,
        },
    }
}

fn new_test_dispatch_signature(byte_stride: u32, update_root_arguments: bool) -> CommandSignature {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12CommandSignatureVtbl` and consists of non-null
    // function pointers; tests never dispatch through command-signature vtable slots.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12CommandSignatureVtbl>() };
    CommandSignature {
        vtbl,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        command_type: D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH,
        update_root_arguments,
        update_vertex_buffers: false,
        update_index_buffer: false,
        byte_stride,
        render_resolver: None,
        compute_resolver: None,
    }
}

fn new_test_buffer_resource(dev: &Obj, len: u64) -> Box<Resource> {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12Resource2Vtbl` and consists of non-null
    // function pointers; tests never dispatch through `Resource::vtbl`.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12Resource2Vtbl>() };
    let buf = Arc::new(Buffer::new(len, dev));
    let alloc = buf.allocate(BufferAllocationFlags::empty());
    let _ = buf.rename(alloc);
    Box::new(Resource {
        vtbl,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        texture: None,
        buffer: Some(buf),
        placed_heap: ptr::null_mut(),
        desc: ResourceDesc {
            dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            alignment: 0,
            width: len,
            height: 1,
            depth_or_array_size: 1,
            mip_levels: 1,
            format: 0,
            sample_desc: SampleDesc {
                count: 1,
                quality: 0,
            },
            layout: 1,
            flags: 0,
        },
        heap_props: HeapProperties {
            r#type: 1,
            cpu_page_property: 0,
            memory_pool_preference: 0,
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
    })
}

struct RecordedComputeCmds {
    set_psos: Vec<(u64, WMTSize)>,
    indirects: Vec<(u64, u64)>,
    icb_execs: usize,
}

fn collect_compute_cmds(alloc: *mut Allocator) -> RecordedComputeCmds {
    // SAFETY: `alloc` is valid for the test's lifetime.
    let current = unsafe { (*alloc).encoder_current };
    assert!(!current.is_null());
    // SAFETY: `current` points to a live `EncoderData`.
    assert_eq!(unsafe { (*current).kind }, EncoderType::Compute);
    let compute_ptr = current.cast::<ComputeEncoderData>();
    // SAFETY: `kind == EncoderType::Compute` guarantees `ComputeEncoderData`.
    let head_next = unsafe { (*compute_ptr).cmd_head.next.ptr };
    let mut set_psos = Vec::new();
    let mut indirects = Vec::new();
    let mut icb_execs = 0usize;
    let mut cursor =
        ptr::with_exposed_provenance::<WmtcmdBase>(usize::try_from(head_next).unwrap_or(0));
    while !cursor.is_null() {
        // SAFETY: `cursor` walks valid command nodes in `alloc`'s CPU heap.
        let header = unsafe { *cursor };
        match header.r#type {
            WMT_COMPUTE_COMMAND_SET_PSO => {
                // SAFETY: `r#type` identifies `WmtcmdComputeSetpso`.
                let c = unsafe { &*cursor.cast::<WmtcmdComputeSetpso>() };
                set_psos.push((c.pso, c.threadgroup_size));
            }
            WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT => {
                // SAFETY: `r#type` identifies `WmtcmdComputeDispatchIndirect`.
                let c = unsafe { &*cursor.cast::<WmtcmdComputeDispatchIndirect>() };
                indirects.push((c.indirect_args_buffer, c.indirect_args_offset));
            }
            WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER => {
                icb_execs += 1;
            }
            _ => {}
        }
        cursor = ptr::with_exposed_provenance::<WmtcmdBase>(
            usize::try_from(header.next.ptr).unwrap_or(0),
        );
    }
    RecordedComputeCmds {
        set_psos,
        indirects,
        icb_execs,
    }
}

#[test]
fn native_compute_dispatch_eligibility() {
    let sig = new_test_dispatch_signature(24, false);
    assert!(can_use_native_compute_dispatch(
        true, &sig, 4, 16, true, 100, 0x9000
    ));
    assert!(can_use_native_compute_dispatch(
        true, &sig, 1, 0, true, 12, 0x9000
    ));
    assert!(!can_use_native_compute_dispatch(
        false, &sig, 4, 16, true, 100, 0x9000
    ));
    assert!(!can_use_native_compute_dispatch(
        true, &sig, 4, 16, false, 100, 0x9000
    ));
    assert!(!can_use_native_compute_dispatch(
        true, &sig, 0, 16, true, 100, 0x9000
    ));
    assert!(!can_use_native_compute_dispatch(
        true, &sig, 4, 16, true, 100, 0
    ));
    assert!(!can_use_native_compute_dispatch(
        true, &sig, 4, 18, true, 128, 0x9000
    ));
    assert!(!can_use_native_compute_dispatch(
        true, &sig, 4, 16, true, 99, 0x9000
    ));
    assert!(!can_use_native_compute_dispatch(
        true,
        &sig,
        4,
        u64::MAX - 4,
        true,
        u64::MAX,
        0x9000
    ));

    let sig_unaligned_stride = new_test_dispatch_signature(14, false);
    assert!(!can_use_native_compute_dispatch(
        true,
        &sig_unaligned_stride,
        2,
        0,
        true,
        128,
        0x9000,
    ));

    let sig_root_args = new_test_dispatch_signature(16, true);
    assert!(!can_use_native_compute_dispatch(
        true,
        &sig_root_args,
        2,
        0,
        true,
        128,
        0x9000
    ));
}

#[test]
fn native_compute_execute_indirect_encodes_dispatch_indirect_commands() {
    let alloc = new_test_allocator();
    let mut pso = Box::new(new_test_compute_pso());
    let pso_ptr: *mut c_void = (&raw mut *pso).cast();
    let iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();

    // SAFETY: `alloc`, `pso_ptr`, `iid`, and `raw_list` are valid test pointers.
    let hr = unsafe {
        create_command_list(
            ptr::null_mut(),
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            alloc.cast(),
            pso_ptr,
            (&raw const iid).cast(),
            &raw mut raw_list,
        )
    };
    assert_eq!(hr, S_OK);

    // SAFETY: `crate::tests` provides no-op `NSObject_retain`/`NSObject_release` stubs.
    let dev = unsafe { Obj::adopt(1) }.expect("nonzero fake handle");
    let mut arg_res = new_test_buffer_resource(&dev, 4096);
    let expected_buf = arg_res
        .buffer
        .as_ref()
        .and_then(|b| b.current())
        .map_or(0, |a| a.buffer());
    assert_ne!(expected_buf, 0);

    let mut sig = Box::new(new_test_dispatch_signature(24, false));
    // SAFETY: `raw_list`, `sig`, and `arg_res` are valid for the duration of this call.
    unsafe {
        execute_indirect(
            raw_list,
            (&raw mut *sig).cast(),
            4,
            (&raw mut *arg_res).cast(),
            16,
            ptr::null_mut(),
            0,
        );
    }

    let cmds = collect_compute_cmds(alloc);
    assert_eq!(cmds.set_psos.len(), 1);
    assert_eq!(cmds.set_psos[0].0, 0x5500);
    assert_eq!(cmds.set_psos[0].1.width, 4);
    assert_eq!(cmds.set_psos[0].1.height, 1);
    assert_eq!(cmds.set_psos[0].1.depth, 1);

    if settings().enabled(Flags::INDIRECT_WITHOUT_ICB) {
        assert_eq!(cmds.icb_execs, 0);
        assert_eq!(
            cmds.indirects,
            vec![
                (expected_buf, 16),
                (expected_buf, 40),
                (expected_buf, 64),
                (expected_buf, 88),
            ]
        );
    } else {
        assert_eq!(cmds.icb_execs, 1);
        assert_eq!(cmds.indirects, []);
    }

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}

#[test]
fn compute_execute_indirect_falls_back_to_icb_when_unaligned_or_counted() {
    let alloc = new_test_allocator();
    let mut pso = Box::new(new_test_compute_pso());
    let pso_ptr: *mut c_void = (&raw mut *pso).cast();
    let iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();

    // SAFETY: `alloc`, `pso_ptr`, `iid`, and `raw_list` are valid test pointers.
    let hr = unsafe {
        create_command_list(
            ptr::null_mut(),
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            alloc.cast(),
            pso_ptr,
            (&raw const iid).cast(),
            &raw mut raw_list,
        )
    };
    assert_eq!(hr, S_OK);

    // SAFETY: `crate::tests` provides no-op `NSObject_retain`/`NSObject_release` stubs.
    let dev = unsafe { Obj::adopt(1) }.expect("nonzero fake handle");
    let mut arg_res = new_test_buffer_resource(&dev, 4096);
    let mut cnt_res = new_test_buffer_resource(&dev, 16);
    let mut sig = Box::new(new_test_dispatch_signature(24, false));

    // Counted compute ExecuteIndirect must fall back to the ICB resolver path.
    // SAFETY: `raw_list`, `sig`, `arg_res`, and `cnt_res` are valid for the call.
    unsafe {
        execute_indirect(
            raw_list,
            (&raw mut *sig).cast(),
            4,
            (&raw mut *arg_res).cast(),
            16,
            (&raw mut *cnt_res).cast(),
            0,
        );
    }
    // Unaligned offset must also fall back to the ICB resolver path.
    // SAFETY: `raw_list`, `sig`, and `arg_res` are valid for the call.
    unsafe {
        execute_indirect(
            raw_list,
            (&raw mut *sig).cast(),
            2,
            (&raw mut *arg_res).cast(),
            18,
            ptr::null_mut(),
            0,
        );
    }

    let cmds = collect_compute_cmds(alloc);
    assert_eq!(cmds.indirects, []);
    assert_eq!(cmds.icb_execs, 2);

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}
