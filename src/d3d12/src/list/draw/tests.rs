//! Unit tests for command-list state, root arguments, draws, and dispatches.

use core::{alloc::Layout, ffi::c_void, ptr};
use std::alloc;

use dxmt_core::topology;
use dxmt_util::{
    com::{ComObject, S_OK},
    private_data::PrivateData,
};
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_SET_BLEND_FACTOR, WmtcmdBase, WmtcmdRenderDraw,
    WmtcmdRenderSetblendcolor,
};

use super::{draw_instanced, geometry_warp};
use crate::{
    abi::{
        list_state::{
            D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D12_COMMAND_LIST_TYPE_COMPUTE,
            D3D12_COMMAND_LIST_TYPE_DIRECT, D3d12Rect, D3d12Viewport,
        },
        vtbl::{ID3D12CommandAllocatorVtbl, ID3D12PipelineStateVtbl},
    },
    allocator::{Allocator, CPU_HEAP_SIZE, GPU_HEAP_SIZE},
    com::{IID_ID3D12_GRAPHICS_COMMAND_LIST, release_com},
    encoder::{EncoderType, RenderEncoderData},
    list::{
        LIST_VTBL, List,
        state::{
            close, create_command_list, get_type, ia_set_primitive_topology, om_set_blend_factor,
            om_set_stencil_ref, reset, rs_set_scissor_rects, rs_set_viewports,
        },
    },
    pipeline::Graphics,
    tests::count_allocs,
};

const CPU_ALIGN: usize = 4096;
const GPU_ALIGN: usize = 16384;

fn new_test_graphics_pso() -> Box<Graphics> {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12PipelineStateVtbl` and consists of non-null
    // function pointers; tests do not invoke PSO vtable slots directly.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12PipelineStateVtbl>() };
    // SAFETY: `crate::tests` provides no-op `NSObject_retain`/`NSObject_release` for unit tests.
    let pso = unsafe { Obj::adopt(1) };
    Graphics::new_dummy(vtbl, pso)
}

fn new_test_allocator(list_type: u32) -> *mut Allocator {
    let cpu_layout = Layout::from_size_align(CPU_HEAP_SIZE, CPU_ALIGN).expect("valid CPU layout");
    let gpu_layout = Layout::from_size_align(GPU_HEAP_SIZE, GPU_ALIGN).expect("valid GPU layout");
    // SAFETY: `cpu_layout` has non-zero size.
    let cpu_heap = unsafe { alloc::alloc(cpu_layout) };
    // SAFETY: `gpu_layout` has non-zero size.
    let gpu_heap = unsafe { alloc::alloc(gpu_layout) };
    assert!(!cpu_heap.is_null());
    assert!(!gpu_heap.is_null());

    // SAFETY: `LIST_VTBL` is larger than `ID3D12CommandAllocatorVtbl` and consists of non-null
    // function pointers; tests do not invoke allocator vtable slots directly.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12CommandAllocatorVtbl>() };

    let raw = Box::into_raw(Box::new(Allocator {
        vtbl,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        list_type,
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
    unsafe { crate::com::add_ref_com(raw) };
    raw
}

unsafe fn free_test_allocator(alloc_ptr: *mut Allocator) {
    // SAFETY: `alloc_ptr` was allocated by `new_test_allocator`.
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
    unsafe { crate::com::release_com(alloc_ptr) };
}

#[test]
fn get_type_is_direct() {
    let alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_DIRECT);
    let iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();

    // SAFETY: `alloc`, `iid`, and `raw_list` are valid pointers for `CreateCommandList`.
    let hr = unsafe {
        create_command_list(
            ptr::null_mut(),
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            alloc.cast(),
            ptr::null_mut(),
            (&raw const iid).cast(),
            &raw mut raw_list,
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!raw_list.is_null());

    // SAFETY: `raw_list` is a valid command list COM pointer.
    let list_type = unsafe { get_type(raw_list) };
    assert_eq!(list_type, D3D12_COMMAND_LIST_TYPE_DIRECT);

    // SAFETY: `raw_list` is recording on `alloc`.
    let close_hr = unsafe { close(raw_list) };
    assert_eq!(close_hr, S_OK);

    let compute_alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_COMPUTE);
    let mut raw_compute: *mut c_void = ptr::null_mut();
    // SAFETY: `compute_alloc`, `iid`, and `raw_compute` are valid pointers.
    let hr_compute = unsafe {
        create_command_list(
            ptr::null_mut(),
            0,
            D3D12_COMMAND_LIST_TYPE_COMPUTE,
            compute_alloc.cast(),
            ptr::null_mut(),
            (&raw const iid).cast(),
            &raw mut raw_compute,
        )
    };
    assert_eq!(hr_compute, S_OK);
    assert!(!raw_compute.is_null());
    // SAFETY: `raw_compute` is a valid command list COM pointer; `GetType` in DXMT always returns
    // `D3D12_COMMAND_LIST_TYPE_DIRECT` (`d3d12_command_list.cpp:344`).
    let compute_type = unsafe { get_type(raw_compute) };
    assert_eq!(compute_type, D3D12_COMMAND_LIST_TYPE_DIRECT);
    // SAFETY: `raw_compute` is recording on `compute_alloc`.
    let close_compute_hr = unsafe { close(raw_compute) };
    assert_eq!(close_compute_hr, S_OK);

    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `raw_compute` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_compute.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
    // SAFETY: `compute_alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(compute_alloc) };
}

#[test]
fn blend_factor_reaches_the_encoder() {
    let alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_DIRECT);
    let mut pso = new_test_graphics_pso();
    let pso_ptr: *mut c_void = (&raw mut *pso).cast();
    let iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();

    // SAFETY: `alloc`, `pso_ptr`, `iid`, and `raw_list` are valid pointers for `CreateCommandList`.
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

    let factor: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
    // SAFETY: `raw_list` is a valid recording command list.
    unsafe { ia_set_primitive_topology(raw_list, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST) };
    // SAFETY: `raw_list` is valid and `factor` has 4 `f32` elements.
    unsafe { om_set_blend_factor(raw_list, factor.as_ptr()) };
    // SAFETY: `raw_list` is a valid recording command list.
    unsafe { draw_instanced(raw_list, 3, 1, 0, 0) };

    // SAFETY: `alloc` is valid and holds the active `RenderEncoderData` in `encoder_current`.
    let current = unsafe { (*alloc).encoder_current };
    assert!(!current.is_null());
    // SAFETY: `current` was created by `ensure_render_encoder`.
    let current_kind = unsafe { (*current).kind };
    assert_eq!(current_kind, EncoderType::Render);
    let render_ptr = current.cast::<RenderEncoderData>();

    let mut found_blend = false;
    let mut found_draw = false;
    // SAFETY: `render_ptr` points to a valid `RenderEncoderData` in the CPU heap.
    let head_next = unsafe { (*render_ptr).cmd_head.next.ptr };
    let mut cursor =
        ptr::with_exposed_provenance::<WmtcmdBase>(usize::try_from(head_next).unwrap_or(0));
    while !cursor.is_null() {
        // SAFETY: `cursor` points to a command node allocated in the CPU heap.
        let cmd_type = unsafe { (*cursor).r#type };
        if cmd_type == WMT_RENDER_COMMAND_SET_BLEND_FACTOR {
            // SAFETY: `cmd_type` matches `WmtcmdRenderSetblendcolor`.
            let blend = unsafe { &*cursor.cast::<WmtcmdRenderSetblendcolor>() };
            assert!((blend.red - 0.25).abs() < f32::EPSILON);
            assert!((blend.green - 0.5).abs() < f32::EPSILON);
            assert!((blend.blue - 0.75).abs() < f32::EPSILON);
            assert!((blend.alpha - 1.0).abs() < f32::EPSILON);
            found_blend = true;
        } else if cmd_type == WMT_RENDER_COMMAND_DRAW {
            // SAFETY: `cmd_type` matches `WmtcmdRenderDraw`.
            let draw = unsafe { &*cursor.cast::<WmtcmdRenderDraw>() };
            assert_eq!(draw.vertex_count, 3);
            assert_eq!(draw.instance_count, 1);
            found_draw = true;
        }
        // SAFETY: `cursor` is a valid `WmtcmdBase` node.
        let next_ptr = unsafe { (*cursor).next.ptr };
        cursor = ptr::with_exposed_provenance::<WmtcmdBase>(usize::try_from(next_ptr).unwrap_or(0));
    }

    assert!(
        found_blend,
        "expected WmtcmdRenderSetblendcolor in encoder stream"
    );
    assert!(found_draw, "expected WmtcmdRenderDraw in encoder stream");

    // SAFETY: `raw_list` is recording on `alloc`.
    let close_hr = unsafe { close(raw_list) };
    assert_eq!(close_hr, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}

#[test]
fn recording_allocates_only_in_the_heap() {
    let alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_DIRECT);
    let mut pso = new_test_graphics_pso();
    let pso_ptr: *mut c_void = (&raw mut *pso).cast();
    let iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();

    // SAFETY: `alloc`, `pso_ptr`, `iid`, and `raw_list` are valid pointers for `CreateCommandList`.
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

    let viewport = D3d12Viewport {
        top_left_x: 0.0,
        top_left_y: 0.0,
        width: 1920.0,
        height: 1080.0,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = D3d12Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    let allocs = count_allocs(|| {
        for i in 0..1000u32 {
            let blend = [0.1, 0.2, 0.3, 0.4];
            // SAFETY: `raw_list` is a valid recording command list.
            unsafe { ia_set_primitive_topology(raw_list, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST) };
            // SAFETY: `raw_list` and `viewport` are valid.
            unsafe { rs_set_viewports(raw_list, 1, (&raw const viewport).cast()) };
            // SAFETY: `raw_list` and `scissor` are valid.
            unsafe { rs_set_scissor_rects(raw_list, 1, (&raw const scissor).cast()) };
            // SAFETY: `raw_list` and `blend` are valid.
            unsafe { om_set_blend_factor(raw_list, blend.as_ptr()) };
            // SAFETY: `raw_list` is a valid recording command list.
            unsafe { om_set_stencil_ref(raw_list, i & 0xff) };
            // SAFETY: `raw_list` is a valid recording command list.
            unsafe { draw_instanced(raw_list, 3, 1, 0, 0) };
        }
    });
    assert_eq!(
        allocs, 0,
        "recording loop must not allocate on the global heap"
    );

    // Verify all 1,000 draws were encoded into the CPU heap.
    // SAFETY: `alloc` is valid and holds the active `RenderEncoderData` in `encoder_current`.
    let current = unsafe { (*alloc).encoder_current };
    assert!(!current.is_null());
    let render_ptr = current.cast::<RenderEncoderData>();
    let mut draw_count = 0usize;
    // SAFETY: `render_ptr` points to a valid `RenderEncoderData` in the CPU heap.
    let head_next = unsafe { (*render_ptr).cmd_head.next.ptr };
    let mut cursor =
        ptr::with_exposed_provenance::<WmtcmdBase>(usize::try_from(head_next).unwrap_or(0));
    while !cursor.is_null() {
        // SAFETY: `cursor` is a valid command node in the CPU heap.
        let cmd_type = unsafe { (*cursor).r#type };
        if cmd_type == WMT_RENDER_COMMAND_DRAW {
            draw_count += 1;
        }
        // SAFETY: `cursor` is a valid command node in the CPU heap.
        let next_ptr = unsafe { (*cursor).next.ptr };
        cursor = ptr::with_exposed_provenance::<WmtcmdBase>(usize::try_from(next_ptr).unwrap_or(0));
    }
    assert_eq!(draw_count, 1000);

    // Verify `Close` and `Reset` also work cleanly.
    // SAFETY: `raw_list` is recording on `alloc`.
    let close1_hr = unsafe { close(raw_list) };
    assert_eq!(close1_hr, S_OK);
    // SAFETY: `raw_list` is closed and `alloc` is free.
    let reset_hr = unsafe { reset(raw_list, alloc.cast(), pso_ptr) };
    assert_eq!(reset_hr, S_OK);
    // SAFETY: `raw_list` is recording on `alloc`.
    let close2_hr = unsafe { close(raw_list) };
    assert_eq!(close2_hr, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}

/// The fixed D3D12 geometry-shader table, and `(0, 0)` where no geometry form exists.
#[test]
fn geometry_warp_table_as_cpp() {
    let table = [
        (topology::POINTLIST, (32, 32)),
        (topology::LINELIST, (32, 32)),
        (topology::LINELIST_ADJ, (32, 32)),
        (topology::LINESTRIP, (32, 31)),
        (topology::TRIANGLELIST, (30, 30)),
        (topology::TRIANGLELIST_ADJ, (30, 30)),
        (topology::TRIANGLESTRIP, (32, 30)),
        (topology::LINESTRIP_ADJ, (32, 29)),
        (topology::TRIANGLESTRIP_ADJ, (32, 28)),
        (0, (0, 0)),
        (topology::PATCHLIST_1, (0, 0)),
        (topology::PATCHLIST_32, (0, 0)),
        (65, (0, 0)),
    ];
    for (topo, expected) in table {
        assert_eq!(geometry_warp(topo), expected, "topology {topo}");
    }
}

/// Every topology draws as the C++'s `to_metal_primitive_type` said, and strips as it said.
#[test]
fn draw_primitive_and_strip_as_cpp() {
    use winemetal_wire::{
        WMT_PRIMITIVE_TYPE_LINE, WMT_PRIMITIVE_TYPE_LINE_STRIP, WMT_PRIMITIVE_TYPE_POINT,
        WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP,
    };

    for (topo, expected) in [
        (0, None),
        (topology::POINTLIST, Some((WMT_PRIMITIVE_TYPE_POINT, 0))),
        (topology::LINELIST, Some((WMT_PRIMITIVE_TYPE_LINE, 0))),
        (
            topology::LINESTRIP,
            Some((WMT_PRIMITIVE_TYPE_LINE_STRIP, 0)),
        ),
        (
            topology::TRIANGLELIST,
            Some((WMT_PRIMITIVE_TYPE_TRIANGLE, 0)),
        ),
        (
            topology::TRIANGLESTRIP,
            Some((WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP, 0)),
        ),
        (6, None),
        (9, None),
        (topology::LINELIST_ADJ, Some((WMT_PRIMITIVE_TYPE_POINT, 0))),
        (
            topology::TRIANGLESTRIP_ADJ,
            Some((WMT_PRIMITIVE_TYPE_POINT, 0)),
        ),
        (14, None),
        (32, None),
        (topology::PATCHLIST_1, Some((WMT_PRIMITIVE_TYPE_POINT, 1))),
        (topology::PATCHLIST_32, Some((WMT_PRIMITIVE_TYPE_POINT, 32))),
        (65, None),
    ] {
        assert_eq!(topology::draw_primitive(topo), expected, "topology {topo}");
    }
    let strips = [
        topology::LINESTRIP,
        topology::LINESTRIP_ADJ,
        topology::TRIANGLESTRIP,
        topology::TRIANGLESTRIP_ADJ,
    ];
    for topo in 0..70 {
        assert_eq!(
            topology::is_strip(topo),
            strips.contains(&topo),
            "topology {topo}"
        );
    }
}
