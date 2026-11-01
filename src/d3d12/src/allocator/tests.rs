//! Unit tests for `ID3D12CommandAllocator` (`d3d12_command_allocator.{hpp,cpp}`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::resource::{Texture, TextureAllocationFlags, TextureViewDescriptor};
use dxmt_util::com;
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_DONT_CARE,
    WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE2D, WMTClearColor, WMTTextureInfo,
};

use super::{Allocator, CPU_HEAP_SIZE, cast_allocator, create_command_allocator};
use crate::{
    abi::vtbl::ID3D12CommandAllocatorVtbl,
    com::IID_ID3D12_COMMAND_ALLOCATOR,
    encoder::{
        ClearEncoderData, ComputeEncoderData, EncoderData, EncoderType,
        RenderEncoderColorAttachmentData, RenderEncoderData, RenderEncoderDepthAttachmentData,
        RenderEncoderStencilAttachmentData, TextureViewRef,
    },
};

#[unsafe(export_name = "MTLDevice_newSharedTexture")]
extern "C-unwind" fn fake_new_shared_texture(_device: u64, _info: *mut WMTTextureInfo) -> u64 {
    0
}

#[repr(C)]
struct RawAllocatorCom {
    vtbl: *const ID3D12CommandAllocatorVtbl,
}

fn new_test_allocator() -> (*mut c_void, *mut Allocator) {
    let iid = IID_ID3D12_COMMAND_ALLOCATOR;
    let mut raw: *mut c_void = ptr::null_mut();
    // SAFETY: `raw` is a valid out-pointer and `iid` is a valid stack GUID.
    let hr = unsafe {
        create_command_allocator(ptr::null_mut(), 0, (&raw const iid).cast(), &raw mut raw)
    };
    assert_eq!(hr, com::S_OK);
    assert!(!raw.is_null());
    // SAFETY: `raw` was just returned by `create_command_allocator`.
    let alloc = unsafe { cast_allocator(raw) }.expect("valid Allocator COM pointer");
    (raw, alloc)
}

unsafe fn release_test_allocator(raw: *mut c_void) {
    // SAFETY: `raw` points to a valid `Allocator` COM object.
    let vtbl = unsafe { (*raw.cast::<RawAllocatorCom>()).vtbl };
    // SAFETY: `vtbl` is a valid allocator vtable pointer.
    let release_fn = unsafe { (*vtbl).release };
    // SAFETY: `raw` is a valid `ID3D12CommandAllocator` pointer with refcount 1.
    let remaining = unsafe { release_fn(raw) };
    assert_eq!(remaining, 0);
}

unsafe fn alloc_compute_encoder(alloc: &mut Allocator) -> *mut EncoderData {
    let ptr = alloc
        .allocate_cpu_heap(
            size_of::<ComputeEncoderData>(),
            align_of::<ComputeEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<ComputeEncoderData>();
    assert!(!ptr.is_null());
    let mut base = EncoderData::new(EncoderType::Compute, 0);
    base.writes_unknown = false;
    base.barriers = alloc.barriers;
    base.barriers_last = alloc.barriers;
    let init = ComputeEncoderData {
        base,
        // SAFETY: `WmtcmdComputeNop` is a C POD struct valid when zeroed.
        cmd_head: unsafe { core::mem::zeroed() },
        cmd_tail: ptr::null_mut(),
    };
    // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `ComputeEncoderData`.
    unsafe { ptr::write(ptr, init) };
    ptr.cast::<EncoderData>()
}

fn make_test_view(format: u32, slice: u32) -> TextureViewRef {
    // SAFETY: `NSObject_retain` and `NSObject_release` are no-op test stubs in `crate::tests`.
    let dev = unsafe { Obj::adopt(1) }.expect("non-zero handle");
    let info = WMTTextureInfo {
        pixel_format: format,
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
    let descriptor = Arc::new(Texture::new(info, &dev));
    let allocation = descriptor.allocate(TextureAllocationFlags::SHARED);
    let key = TextureViewDescriptor {
        format,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: slice,
        array_size: 1,
    }
    .key();
    TextureViewRef {
        descriptor,
        allocation,
        key,
        texture: u64::from(format),
        gpu_resource_id: u64::from(format),
    }
}

#[test]
fn decide_joins_as_cpp() {
    let (raw, alloc_ptr) = new_test_allocator();
    // SAFETY: `alloc_ptr` is valid for the duration of the test before `release_test_allocator`.
    let alloc = unsafe { &mut *alloc_ptr };

    let head = alloc.start_record().expect("start_record should succeed");
    assert!(!head.is_null());

    let r1 = 0x1000usize as *const c_void;
    let r2 = 0x2000usize as *const c_void;
    let r3 = 0x3000usize as *const c_void;

    // Pass 0: first pass after Null head must join. Writes r1.
    // SAFETY: `alloc` is valid and recording.
    let e0 = unsafe { alloc_compute_encoder(alloc) };
    alloc.encoder_current = e0;
    alloc.writes(r1);
    alloc.invalidate_current_pass();

    // Pass 1: no barrier, writes r1 and r2 -> depends on pass 0 (join == false, deps == [0]).
    // SAFETY: `alloc` is valid and recording.
    let e1 = unsafe { alloc_compute_encoder(alloc) };
    alloc.encoder_current = e1;
    alloc.writes(r1);
    alloc.writes(r2);
    alloc.invalidate_current_pass();

    // Pass 2: no barrier, writes r1 -> covered by pass 1 so depends only on pass 1 (deps == [1]).
    // SAFETY: `alloc` is valid and recording.
    let e2 = unsafe { alloc_compute_encoder(alloc) };
    alloc.encoder_current = e2;
    alloc.writes(r1);
    alloc.invalidate_current_pass();

    // Transition barrier (non-join) on r2, then Pass 3 writes r3 -> depends on pass 1 via transition.
    alloc.barrier(false, &[r2]);
    // SAFETY: `alloc` is valid and recording.
    let e3 = unsafe { alloc_compute_encoder(alloc) };
    alloc.encoder_current = e3;
    alloc.writes(r3);
    alloc.invalidate_current_pass();

    // Pass 4: unknown write (`null` resource) forces a join and resets the group.
    // SAFETY: `alloc` is valid and recording.
    let e4 = unsafe { alloc_compute_encoder(alloc) };
    alloc.encoder_current = e4;
    alloc.writes(ptr::null());
    alloc.invalidate_current_pass();

    // Pass 5: explicit join barrier forces join.
    alloc.barrier(true, &[]);
    // SAFETY: `alloc` is valid and recording.
    let e5 = unsafe { alloc_compute_encoder(alloc) };
    alloc.encoder_current = e5;
    alloc.writes(r1);
    alloc.invalidate_current_pass();

    let total = alloc.end_record().expect("end_record should succeed");
    assert_eq!(total, 6);

    // SAFETY: `e0` lives in `alloc`'s CPU heap until Reset or Drop.
    let d0 = unsafe { &*e0 };
    // SAFETY: `e1` lives in `alloc`'s CPU heap until Reset or Drop.
    let d1 = unsafe { &*e1 };
    // SAFETY: `e2` lives in `alloc`'s CPU heap until Reset or Drop.
    let d2 = unsafe { &*e2 };
    // SAFETY: `e3` lives in `alloc`'s CPU heap until Reset or Drop.
    let d3 = unsafe { &*e3 };
    // SAFETY: `e4` lives in `alloc`'s CPU heap until Reset or Drop.
    let d4 = unsafe { &*e4 };
    // SAFETY: `e5` lives in `alloc`'s CPU heap until Reset or Drop.
    let d5 = unsafe { &*e5 };

    assert!(d0.join);
    assert_eq!(d0.position, 0);
    assert_eq!(d0.dep_count, 0);

    assert!(!d1.join);
    assert_eq!(d1.position, 1);
    assert_eq!(d1.dep_count, 1);
    // SAFETY: `d1.deps` points to `d1.dep_count` u32 entries in the CPU heap.
    assert_eq!(unsafe { *d1.deps }, 0);

    assert!(!d2.join);
    assert_eq!(d2.position, 2);
    assert_eq!(d2.dep_count, 1);
    // SAFETY: `d2.deps` points to `d2.dep_count` u32 entries in the CPU heap.
    assert_eq!(unsafe { *d2.deps }, 1);

    assert!(!d3.join);
    assert_eq!(d3.position, 3);
    assert_eq!(d3.dep_count, 1);
    // SAFETY: `d3.deps` points to `d3.dep_count` u32 entries in the CPU heap.
    assert_eq!(unsafe { *d3.deps }, 1);

    assert!(d4.join);
    assert_eq!(d4.position, 4);
    assert_eq!(d4.dep_count, 0);

    assert!(d5.join);
    assert_eq!(d5.position, 5);
    assert_eq!(d5.dep_count, 0);

    // SAFETY: `raw` is the allocator COM pointer created at the start of the test.
    unsafe { release_test_allocator(raw) };
}

#[test]
fn fold_clears_as_cpp() {
    let (raw, alloc_ptr) = new_test_allocator();
    // SAFETY: `alloc_ptr` is valid for the duration of the test before `release_test_allocator`.
    let alloc = unsafe { &mut *alloc_ptr };

    let _head = alloc.start_record().expect("start_record should succeed");

    let view_rt0 = make_test_view(70, 0);
    let view_other = make_test_view(80, 1);
    let view_ds = make_test_view(260, 0);

    let res_rt0 = view_rt0.descriptor_ptr();
    let res_other = view_other.descriptor_ptr();
    let res_ds = view_ds.descriptor_ptr();

    let push_clear = |alloc: &mut Allocator,
                      res: *const c_void,
                      view: &TextureViewRef,
                      clear_dsv: u32,
                      color: WMTClearColor,
                      depth_stencil: (f32, u8),
                      dim: u32| {
        let ptr = alloc
            .allocate_cpu_heap(
                size_of::<ClearEncoderData>(),
                align_of::<ClearEncoderData>(),
            )
            .cast::<c_void>()
            .cast::<ClearEncoderData>();
        assert!(!ptr.is_null());
        let mut base = EncoderData::new(EncoderType::Clear, 0);
        base.writes_unknown = false;
        base.barriers = alloc.barriers;
        base.barriers_last = alloc.barriers;
        let clear = ClearEncoderData {
            base,
            color,
            depth_stencil,
            attachment: Some(view.clone()),
            clear_dsv,
            array_length: 1,
            width: dim,
            height: dim,
            depth_plane: 0,
        };
        // SAFETY: `ptr` is valid aligned CPU heap memory for `ClearEncoderData`.
        unsafe { ptr::write(ptr, clear) };
        alloc.encoder_current = ptr.cast::<EncoderData>();
        alloc.writes(res);
        alloc.invalidate_current_pass();
    };

    // Clear 0: RTV0 cleared to red.
    push_clear(
        alloc,
        res_rt0,
        &view_rt0,
        0,
        WMTClearColor {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        (0.0, 0),
        64,
    );
    // Clear 1: RTV0 cleared again to green (must overwrite red when folded).
    push_clear(
        alloc,
        res_rt0,
        &view_rt0,
        0,
        WMTClearColor {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
        (0.0, 0),
        64,
    );
    // Clear 2: unrelated view (must be kept and not folded).
    push_clear(
        alloc,
        res_other,
        &view_other,
        0,
        WMTClearColor {
            r: 0.5,
            g: 0.5,
            b: 0.5,
            a: 1.0,
        },
        (0.0, 0),
        64,
    );
    // Clear 3: DSV cleared to depth 0.25, stencil 7.
    push_clear(
        alloc,
        res_ds,
        &view_ds,
        3,
        WMTClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
        (0.25, 7),
        64,
    );

    let r_ptr = alloc
        .allocate_cpu_heap(
            size_of::<RenderEncoderData>(),
            align_of::<RenderEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<RenderEncoderData>();
    assert!(!r_ptr.is_null());
    let mut r_base = EncoderData::new(EncoderType::Render, 0);
    r_base.writes_unknown = false;
    r_base.barriers = alloc.barriers;
    r_base.barriers_last = alloc.barriers;

    let mut colors: [RenderEncoderColorAttachmentData; 8] = Default::default();
    colors[0] = RenderEncoderColorAttachmentData {
        attachment: Some(view_rt0),
        load_action: WMT_LOAD_ACTION_LOAD,
        store_action: WMT_STORE_ACTION_STORE,
        ..Default::default()
    };

    let depth = RenderEncoderDepthAttachmentData {
        attachment: Some(view_ds.clone()),
        load_action: WMT_LOAD_ACTION_LOAD,
        store_action: WMT_STORE_ACTION_DONT_CARE,
        ..Default::default()
    };
    let stencil = RenderEncoderStencilAttachmentData {
        attachment: Some(view_ds),
        load_action: WMT_LOAD_ACTION_LOAD,
        store_action: WMT_STORE_ACTION_DONT_CARE,
        ..Default::default()
    };

    let render_init = RenderEncoderData {
        base: r_base,
        colors,
        depth,
        stencil,
        default_raster_sample_count: 1,
        render_target_array_length: 1,
        render_target_height: 64,
        render_target_width: 64,
        // SAFETY: `WmtcmdRenderNop` is a C POD struct valid when zeroed.
        cmd_head: unsafe { core::mem::zeroed() },
        cmd_tail: ptr::null_mut(),
        dsv_planar_flags: 3,
        dsv_readonly_flags: 0,
        render_target_count: 1,
        use_visibility_result: false,
        visibility_buffer: 0,
        use_tessellation: false,
        use_geometry: false,
        uav_writes: false,
        // SAFETY: `WmtcmdComputeNop` is a C POD struct valid when zeroed.
        pre_head: unsafe { core::mem::zeroed() },
        pre_tail: ptr::null_mut(),
    };
    // SAFETY: `r_ptr` points to valid CPU heap memory for `RenderEncoderData`.
    unsafe { ptr::write(r_ptr, render_init) };

    // SAFETY: `r_ptr` was just initialized above.
    let folded = alloc.fold_clears(unsafe { &mut *r_ptr });
    assert_eq!(folded, 3);

    // SAFETY: `r_ptr` remains valid in the CPU heap.
    let render = unsafe { &*r_ptr };
    assert_eq!(render.colors[0].load_action, WMT_LOAD_ACTION_CLEAR);
    assert!((render.colors[0].clear_color.r - 0.0).abs() < f64::EPSILON);
    assert!((render.colors[0].clear_color.g - 1.0).abs() < f64::EPSILON);
    assert!((render.colors[0].clear_color.b - 0.0).abs() < f64::EPSILON);
    assert!((render.colors[0].clear_color.a - 1.0).abs() < f64::EPSILON);

    assert_eq!(render.depth.load_action, WMT_LOAD_ACTION_CLEAR);
    assert!((render.depth.clear_depth - 0.25).abs() < f32::EPSILON);
    assert_eq!(render.stencil.load_action, WMT_LOAD_ACTION_CLEAR);
    assert_eq!(render.stencil.clear_stencil, 7);

    alloc.invalidate_current_pass();
    let remaining_passes = alloc.end_record().expect("end_record should succeed");
    // 1 kept clear (view_other) + 1 render pass = 2 passes.
    assert_eq!(remaining_passes, 2);

    // SAFETY: `raw` is the allocator COM pointer created at the start of the test.
    unsafe { release_test_allocator(raw) };
}

#[test]
fn reset_refused_while_recording() {
    let (raw, alloc_ptr) = new_test_allocator();
    // SAFETY: `alloc_ptr` is valid for the duration of the test before `release_test_allocator`.
    let alloc = unsafe { &mut *alloc_ptr };
    // SAFETY: `raw` points to a valid `Allocator` COM object.
    let vtbl = unsafe { (*raw.cast::<RawAllocatorCom>()).vtbl };

    let head = alloc.start_record().expect("start_record should succeed");
    assert!(!head.is_null());

    // Starting a second recording while already recording must fail.
    assert!(alloc.start_record().is_err());

    // Allocate across a CPU heap block boundary to trigger a spill block.
    let chunk = CPU_HEAP_SIZE / 2 + 1024;
    let p1 = alloc.allocate_cpu_heap(chunk, 16);
    let p2 = alloc.allocate_cpu_heap(chunk, 16);
    assert!(!p1.is_null());
    assert!(!p2.is_null());
    assert_eq!(alloc.spilled_cpu_heap.len(), 1);

    // Resetting while a command list is still recording must fail with E_FAIL.
    // SAFETY: `vtbl` is a valid allocator vtable pointer.
    let reset_fn = unsafe { (*vtbl).reset };
    // SAFETY: `raw` is a valid `ID3D12CommandAllocator`.
    let hr_busy = unsafe { reset_fn(raw) };
    assert_eq!(hr_busy, com::E_FAIL);
    assert_eq!(alloc.spilled_cpu_heap.len(), 1);

    let count = alloc.end_record().expect("end_record should succeed");
    assert_eq!(count, 0);

    // Resetting after recording has ended must succeed with S_OK and free spilled blocks.
    // SAFETY: `raw` is a valid `ID3D12CommandAllocator`.
    let hr_ok = unsafe { reset_fn(raw) };
    assert_eq!(hr_ok, com::S_OK);
    assert_eq!(alloc.spilled_cpu_heap.len(), 0);
    assert_eq!(alloc.cpu_heap_offset, 0);

    // SAFETY: `raw` is the allocator COM pointer created at the start of the test.
    unsafe { release_test_allocator(raw) };
}
