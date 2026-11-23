//! Unit tests for command-list state, root arguments, draws, and dispatches.

use core::{alloc::Layout, ffi::c_void, mem::ManuallyDrop, ptr, sync::atomic::AtomicBool};
use std::{
    alloc,
    sync::{Arc, Mutex},
};

use dxmt_core::{
    resource::{Texture, TextureAllocationFlags, TextureViewDescriptor},
    topology,
};
use dxmt_util::{
    com::{ComObject, S_OK},
    private_data::PrivateData,
};
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_LOAD_ACTION_DONT_CARE, WMT_LOAD_ACTION_LOAD, WMT_RENDER_COMMAND_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED, WMT_RENDER_COMMAND_SET_BLEND_FACTOR,
    WMT_TEXTURE_TYPE2D, WMTTextureInfo, WmtcmdBase, WmtcmdRenderDraw,
    WmtcmdRenderDxmtTessellationMeshDraw, WmtcmdRenderDxmtTessellationMeshDrawIndexed,
    WmtcmdRenderSetblendcolor,
};

use super::{
    draw_indexed_instanced, draw_instanced, geometry_warp, patch_control_point_count,
    pre_draw_with_options,
};
use crate::{
    abi::{
        list_ops::{
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD,
            D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_PRESERVE, RenderPassBeginningAccess,
            RenderPassBeginningAccessBody, RenderPassEndingAccess, RenderPassEndingAccessBody,
            RenderPassRenderTargetDesc,
        },
        list_state::{
            D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D12_COMMAND_LIST_TYPE_COMPUTE,
            D3D12_COMMAND_LIST_TYPE_DIRECT, D3d12Rect, D3d12Viewport,
        },
        objects::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        pipeline::SampleDesc,
        resource::{D3D12_RESOURCE_DIMENSION_TEXTURE2D, HeapProperties, ResourceDesc},
        vtbl::{
            D3d12CpuDescriptorHandle, ID3D12CommandAllocatorVtbl, ID3D12DescriptorHeapVtbl,
            ID3D12PipelineStateVtbl, ID3D12Resource2Vtbl,
        },
    },
    allocator::{Allocator, CPU_HEAP_SIZE, GPU_HEAP_SIZE},
    com::{IID_ID3D12_GRAPHICS_COMMAND_LIST, release_com},
    descriptor_heap::{EmbeddedDescriptorHandle, Heap, RenderTargetDesc},
    encoder::{EncoderType, RenderEncoderData},
    list::{
        DrawCallStatus, LIST_VTBL, List,
        clear::{begin_render_pass, discard_resource, end_render_pass},
        state::{
            close, create_command_list, get_type, ia_set_primitive_topology, om_set_blend_factor,
            om_set_render_targets, om_set_stencil_ref, reset, rs_set_scissor_rects,
            rs_set_viewports,
        },
    },
    pipeline::Graphics,
    resource::Resource,
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
        discard_tracking: false,
        discard_purges: Vec::new(),
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

#[test]
fn patch_control_point_count_covers_1_to_32() {
    for cp in 1..=32 {
        let topo = topology::PATCHLIST_1 + cp - 1;
        assert_eq!(patch_control_point_count(topo), Some(cp));
    }
    assert_eq!(patch_control_point_count(0), None);
    assert_eq!(
        patch_control_point_count(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST),
        None
    );
    assert_eq!(patch_control_point_count(topology::PATCHLIST_1 - 1), None);
    assert_eq!(patch_control_point_count(topology::PATCHLIST_32 + 1), None);
}

#[test]
fn tessellation_draws_emit_tessellation_mesh_commands() {
    let alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_DIRECT);
    let mut pso = new_test_graphics_pso();
    pso.tessellation = true;
    pso.threads_per_patch = 4;
    pso.patch_per_group = 8;
    {
        let mut variants = dxmt_util::sync::lock(&pso.tessellation_variants);
        // SAFETY: fake retained object handle; `crate::tests` fakes `NSObject_retain`/`_release`.
        variants[0] = unsafe { Obj::adopt(0x7701) };
        // SAFETY: fake retained object handle.
        variants[1] = unsafe { Obj::adopt(0x7702) };
    }
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
    // SAFETY: `raw_list` is a live `List` with a 4-CP patch list topology.
    unsafe { ia_set_primitive_topology(raw_list, topology::PATCHLIST_1 + 3) };
    // SAFETY: `raw_list` is a valid recording command list.
    unsafe { draw_instanced(raw_list, 40, 2, 0, 0) };
    // Null index buffer must return early without encoding an indexed draw.
    // SAFETY: `raw_list` is a valid recording command list.
    unsafe { draw_indexed_instanced(raw_list, 36, 3, 4, 1, 2) };
    // SAFETY: `raw_list` is a live `List`; bind a non-zero test index buffer handle.
    unsafe { (*raw_list.cast::<List>()).index_buffer = 0x8800 };
    // SAFETY: `raw_list` is a valid recording command list with a bound index buffer.
    unsafe { draw_indexed_instanced(raw_list, 36, 3, 4, 1, 2) };

    // SAFETY: `alloc` holds the active `RenderEncoderData` in `encoder_current`.
    let render_ptr = unsafe { (*alloc).encoder_current }.cast::<RenderEncoderData>();
    // SAFETY: `render_ptr` points to a valid `RenderEncoderData` in the CPU heap.
    let render_data = unsafe { &*render_ptr };
    assert!(render_data.use_tessellation);
    assert!(!render_data.use_geometry);

    let mut found_draw = None;
    let mut found_indexed = None;
    let mut cursor = ptr::with_exposed_provenance::<WmtcmdBase>(
        usize::try_from(render_data.cmd_head.next.ptr).unwrap_or(0),
    );
    while !cursor.is_null() {
        // SAFETY: `cursor` walks the linked render command list allocated in `alloc.cpu_heap`.
        let header = unsafe { *cursor };
        if header.r#type == WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW {
            // SAFETY: `header.r#type` identifies `WmtcmdRenderDxmtTessellationMeshDraw`.
            let c = unsafe { &*cursor.cast::<WmtcmdRenderDxmtTessellationMeshDraw>() };
            found_draw = Some((
                c.instance_count,
                c.threads_per_patch,
                c.patch_per_group,
                c.patch_per_mesh_instance,
            ));
        } else if header.r#type == WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED {
            // SAFETY: `header.r#type` identifies `WmtcmdRenderDxmtTessellationMeshDrawIndexed`.
            let c = unsafe { &*cursor.cast::<WmtcmdRenderDxmtTessellationMeshDrawIndexed>() };
            found_indexed = Some((
                c.instance_count,
                c.threads_per_patch,
                c.patch_per_group,
                c.patch_per_mesh_instance,
            ));
        }
        cursor = ptr::with_exposed_provenance::<WmtcmdBase>(
            usize::try_from(header.next.ptr).unwrap_or(0),
        );
    }
    // 40 verts / 4 CP = 10 patches; div_ceil(8) = 2 groups.
    assert_eq!(found_draw, Some((2, 4, 8, 2)));
    // 36 indices / 4 CP = 9 patches; div_ceil(8) = 2 groups.
    assert_eq!(found_indexed, Some((3, 4, 8, 2)));

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}

#[test]
fn tessellation_topology_mismatch_is_rejected_before_encoder_mutation() {
    let alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_DIRECT);
    let mut tess_pso = new_test_graphics_pso();
    tess_pso.tessellation = true;
    tess_pso.threads_per_patch = 4;
    tess_pso.patch_per_group = 8;
    {
        let mut variants = dxmt_util::sync::lock(&tess_pso.tessellation_variants);
        // SAFETY: fake retained object handle; `crate::tests` fakes `NSObject_retain`/`_release`.
        variants[0] = unsafe { Obj::adopt(0x7703) };
    }
    let plain_pso = new_test_graphics_pso();
    let iid = IID_ID3D12_GRAPHICS_COMMAND_LIST;
    let mut raw_list: *mut c_void = ptr::null_mut();

    let tess_ptr: *mut c_void = (&raw mut *tess_pso).cast();
    // SAFETY: `alloc`, `tess_ptr`, `iid`, and `raw_list` are valid test pointers.
    let hr = unsafe {
        create_command_list(
            ptr::null_mut(),
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            alloc.cast(),
            tess_ptr,
            (&raw const iid).cast(),
            &raw mut raw_list,
        )
    };
    assert_eq!(hr, S_OK);

    // 1. Tessellation PSO with non-patch topology must be rejected before opening a render pass.
    // SAFETY: `raw_list` is a live `List`.
    unsafe { ia_set_primitive_topology(raw_list, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST) };
    // SAFETY: `raw_list` is a live `List`.
    let status_tess = pre_draw_with_options(unsafe { &mut *raw_list.cast::<List>() }, false, false);
    assert_eq!(status_tess, DrawCallStatus::Invalid);
    // SAFETY: `alloc` is valid.
    assert!(unsafe { (*alloc).encoder_current }.is_null());

    // 2. Non-tessellation PSO with patch-list topology must also be rejected up front.
    // SAFETY: `raw_list` is a live `List`.
    let orig_pso = unsafe { (*raw_list.cast::<List>()).pso_graphics };
    // SAFETY: `raw_list` is a live `List`; `plain_pso` outlives this scope.
    unsafe { (*raw_list.cast::<List>()).pso_graphics = (&raw const *plain_pso).cast() };
    // SAFETY: `raw_list` is a live `List`.
    unsafe { ia_set_primitive_topology(raw_list, topology::PATCHLIST_1 + 3) };
    // SAFETY: `raw_list` is a live `List`.
    let status_plain =
        pre_draw_with_options(unsafe { &mut *raw_list.cast::<List>() }, false, false);
    // Restore `orig_pso` so COM refcounts stay balanced across `List::drop` and `Box::drop`.
    // SAFETY: `raw_list` is a live `List`.
    unsafe { (*raw_list.cast::<List>()).pso_graphics = orig_pso };
    assert_eq!(status_plain, DrawCallStatus::Invalid);
    // SAFETY: `alloc` is valid.
    assert!(unsafe { (*alloc).encoder_current }.is_null());

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}

fn make_draw_test_texture(pixel_format: u32) -> Arc<Texture> {
    let info = WMTTextureInfo {
        pixel_format,
        width: 64,
        height: 64,
        depth: 1,
        array_length: 1,
        bits: (WMT_TEXTURE_TYPE2D & 0xff) | (1 << 8) | (1 << 16),
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    // SAFETY: `crate::tests` stubs `NSObject_retain`/`NSObject_release` for test `Obj` handles.
    let obj = unsafe { Obj::adopt(1) }.expect("non-zero fake handle");
    let tex = Arc::new(Texture::new(info, &obj));
    let alloc = tex.allocate(TextureAllocationFlags::SHARED);
    let _ = tex.rename(alloc);
    tex
}

fn make_draw_test_heap(entries: Vec<RenderTargetDesc>) -> Box<Heap> {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12DescriptorHeapVtbl` and consists of non-null
    // function pointers; tests never dispatch through heap vtable slots.
    let vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12DescriptorHeapVtbl>() };
    Box::new(Heap {
        vtbl,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        heap_type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        num_descriptors: u32::try_from(entries.len()).unwrap_or(0),
        flags: 0,
        gpu_buffer: None,
        gpu_mapped_ptr: ptr::null_mut(),
        gpu_base_address: 0,
        cpu_words_ptr: ptr::null_mut(),
        has_cpu_descriptors: AtomicBool::new(false),
        cpu_descriptors: Mutex::new(Vec::new()),
        samplers: Mutex::new(Vec::new()),
        render_targets: Mutex::new(entries),
    })
}

#[test]
fn render_pass_discard_applies_only_to_first_encoder_across_mid_pass_split() {
    let alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_DIRECT);
    let mut pso = new_test_graphics_pso();
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
    // SAFETY: `raw_list` is a valid `List` pointer.
    unsafe { ia_set_primitive_topology(raw_list, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST) };

    let tex = make_draw_test_texture(70);
    let view = TextureViewDescriptor {
        format: 70,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();
    let heap = make_draw_test_heap(vec![RenderTargetDesc {
        texture: Some(tex),
        view,
        width: 64,
        height: 64,
        depth_plane: 0,
        render_target_array_length: 1,
        flags: 0,
    }]);
    let rtv: D3d12CpuDescriptorHandle = EmbeddedDescriptorHandle::pack(&raw const *heap, 0);

    let rt_desc = RenderPassRenderTargetDesc {
        cpu_descriptor: rtv,
        beginning_access: RenderPassBeginningAccess {
            r#type: D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD,
            access: RenderPassBeginningAccessBody {
                // SAFETY: zeroed union is valid when `r#type == DISCARD`.
                clear: ManuallyDrop::new(unsafe { core::mem::zeroed() }),
            },
        },
        ending_access: RenderPassEndingAccess {
            r#type: D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_PRESERVE,
            access: RenderPassEndingAccessBody {
                // SAFETY: zeroed union is valid when `r#type == PRESERVE`.
                resolve: ManuallyDrop::new(unsafe { core::mem::zeroed() }),
            },
        },
    };
    // SAFETY: `raw_list` and `&rt_desc` are valid test pointers.
    unsafe { begin_render_pass(raw_list, 1, (&raw const rt_desc).cast(), ptr::null(), 0) };

    // SAFETY: `raw_list` is a live `List`.
    let list = unsafe { &mut *raw_list.cast::<List>() };
    assert_eq!(
        pre_draw_with_options(list, false, false),
        DrawCallStatus::Valid
    );
    // SAFETY: `alloc` is valid and `encoder_current` was just opened by `pre_draw_with_options`.
    let first_enc = unsafe { (*alloc).encoder_current.cast::<RenderEncoderData>() };
    assert!(!first_enc.is_null());
    // SAFETY: `first_enc` points to a valid `RenderEncoderData`.
    let first_load = unsafe { (*first_enc).colors[0].load_action };
    assert_eq!(first_load, WMT_LOAD_ACTION_DONT_CARE);

    // Simulate a mid-pass encoder split before a second draw inside the same render pass.
    // SAFETY: `alloc` is valid.
    unsafe { (*alloc).invalidate_current_pass() };
    list.inside_render_pass = false;

    assert_eq!(
        pre_draw_with_options(list, false, false),
        DrawCallStatus::Valid
    );
    // SAFETY: `alloc` is valid and `encoder_current` was opened by the second draw.
    let second_enc = unsafe { (*alloc).encoder_current.cast::<RenderEncoderData>() };
    assert!(!second_enc.is_null());
    assert_ne!(first_enc, second_enc);
    // SAFETY: `second_enc` points to a valid `RenderEncoderData`.
    let second_load = unsafe { (*second_enc).colors[0].load_action };
    assert_eq!(second_load, WMT_LOAD_ACTION_LOAD);

    // SAFETY: `raw_list` is a valid `List` pointer.
    unsafe { end_render_pass(raw_list) };
    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}

#[test]
fn discard_resource_consumes_on_draw_and_respects_write_purge() {
    let alloc = new_test_allocator(D3D12_COMMAND_LIST_TYPE_DIRECT);
    let mut pso = new_test_graphics_pso();
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
    // SAFETY: `raw_list` is a valid `List` pointer.
    unsafe { ia_set_primitive_topology(raw_list, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST) };

    let tex = make_draw_test_texture(70);
    let tex_ptr = Arc::as_ptr(&tex).cast::<c_void>();
    // SAFETY: `LIST_VTBL` is larger than `ID3D12Resource2Vtbl` and consists of non-null pointers.
    let res_vtbl = unsafe { &*(&raw const LIST_VTBL).cast::<ID3D12Resource2Vtbl>() };
    let mut res = Box::new(Resource {
        vtbl: res_vtbl,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        texture: Some(Arc::clone(&tex)),
        buffer: None,
        placed_heap: ptr::null_mut(),
        desc: ResourceDesc {
            dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            alignment: 0,
            width: 64,
            height: 64,
            depth_or_array_size: 1,
            mip_levels: 1,
            format: 28,
            sample_desc: SampleDesc {
                count: 1,
                quality: 0,
            },
            layout: 0,
            flags: 0,
        },
        heap_props: HeapProperties {
            r#type: 1,
            cpu_page_property: 0,
            memory_pool_preference: 0,
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
    });
    let view = TextureViewDescriptor {
        format: 70,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    }
    .key();
    let heap = make_draw_test_heap(vec![RenderTargetDesc {
        texture: Some(tex),
        view,
        width: 64,
        height: 64,
        depth_plane: 0,
        render_target_array_length: 1,
        flags: 0,
    }]);
    let rtv: D3d12CpuDescriptorHandle = EmbeddedDescriptorHandle::pack(&raw const *heap, 0);
    let res_ptr: *mut c_void = (&raw mut *res).cast();

    // 1. DiscardResource followed by OMSetRenderTargets + Draw consumes the discard to DONT_CARE.
    // SAFETY: `raw_list` and `res_ptr` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, ptr::null()) };
    // SAFETY: `raw_list` and `&rtv` are valid test pointers.
    unsafe { om_set_render_targets(raw_list, 1, (&raw const rtv).cast(), 0, ptr::null()) };
    // SAFETY: `raw_list` is a live `List`.
    let list = unsafe { &mut *raw_list.cast::<List>() };
    assert_eq!(
        pre_draw_with_options(list, false, false),
        DrawCallStatus::Valid
    );
    // SAFETY: `alloc` is valid and `encoder_current` was just opened by `pre_draw_with_options`.
    let first_enc = unsafe { (*alloc).encoder_current.cast::<RenderEncoderData>() };
    // SAFETY: `first_enc` points to a valid `RenderEncoderData`.
    let first_load = unsafe { (*first_enc).colors[0].load_action };
    assert_eq!(first_load, WMT_LOAD_ACTION_DONT_CARE);
    assert!(list.discarded_resources.is_empty());

    // 2. DiscardResource followed by an intervening `alloc.writes(tex_ptr)` is purged on next draw.
    // SAFETY: `raw_list` and `res_ptr` are valid test pointers.
    unsafe { discard_resource(raw_list, res_ptr, ptr::null()) };
    // SAFETY: `alloc` is valid and has `first_enc` active to record the write purge.
    unsafe { (*alloc).writes(tex_ptr) };
    // SAFETY: `alloc` is valid.
    unsafe { (*alloc).invalidate_current_pass() };
    // SAFETY: `raw_list` is a live `List`.
    let list2 = unsafe { &mut *raw_list.cast::<List>() };
    list2.inside_render_pass = false;
    assert_eq!(
        pre_draw_with_options(list2, false, false),
        DrawCallStatus::Valid
    );
    // SAFETY: `alloc` is valid and `encoder_current` was opened by the second draw.
    let second_enc = unsafe { (*alloc).encoder_current.cast::<RenderEncoderData>() };
    // SAFETY: `second_enc` points to a valid `RenderEncoderData`.
    let second_load = unsafe { (*second_enc).colors[0].load_action };
    assert_eq!(second_load, WMT_LOAD_ACTION_LOAD);
    assert!(list2.discarded_resources.is_empty());

    // SAFETY: `raw_list` is recording on `alloc`.
    assert_eq!(unsafe { close(raw_list) }, S_OK);
    // SAFETY: `raw_list` is a valid `List` COM pointer.
    unsafe { release_com::<List>(raw_list.cast()) };
    // SAFETY: `alloc` was created by `new_test_allocator`.
    unsafe { free_test_allocator(alloc) };
}
