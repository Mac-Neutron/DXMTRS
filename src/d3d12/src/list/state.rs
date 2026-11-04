//! Command-list creation, lifecycle (`Close`/`Reset`), and state setters
//!
//! (`d3d12_command_list.cpp`).

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::Arc;

use dxmt_core::format::depth_stencil_planar_flags;
use winemetal_wire::{
    WMT_INDEX_TYPE_U_INT16, WMT_INDEX_TYPE_U_INT32, WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_STORE,
    WMTScissorRect, WMTViewport,
};

use crate::{
    abi::{
        ext::IID_D3D12_COMMAND_LIST_EXT,
        list_state::{
            D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_SHADING_RATE_1X1,
            D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT,
            D3D12_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE, D3d12IndexBufferView,
            D3d12Rect, D3d12VertexBufferView, D3d12Viewport,
        },
        vtbl::{D3d12CpuDescriptorHandle, D3d12GpuDescriptorHandle, IUnknownVtbl},
    },
    allocator::Allocator,
    com::{
        self, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult,
        IID_ID3D12_COMMAND_LIST, IID_ID3D12_DEVICE_CHILD, IID_ID3D12_GRAPHICS_COMMAND_LIST,
        IID_ID3D12_GRAPHICS_COMMAND_LIST1, IID_ID3D12_GRAPHICS_COMMAND_LIST2,
        IID_ID3D12_GRAPHICS_COMMAND_LIST3, IID_ID3D12_GRAPHICS_COMMAND_LIST4,
        IID_ID3D12_GRAPHICS_COMMAND_LIST5, IID_ID3D12_GRAPHICS_COMMAND_LIST6,
        IID_ID3D12_GRAPHICS_COMMAND_LIST7, IID_ID3D12_OBJECT, IID_IUNKNOWN, S_OK,
    },
    descriptor_heap::{self, Heap},
    device::{Device, null, residency},
    encoder::{EncoderData, EncoderType, RenderEncoderData, TextureViewRef},
    list::{DirtyFlags, List},
    pipeline::{Compute, Graphics},
    root_signature::object::RootSignature,
};

const DXGI_FORMAT_R32_UINT: u32 = 42;
const D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER: u32 = 1;
const RTV_DSV_HEAP_INCREMENTAL_SIZE: usize = 32;
const MAX_SCISSOR_COORD: i32 = 16384;
const DEFAULT_RENDER_TARGET_DIM: u32 = 16384;
const D3D12_DSV_READONLY_MASK: u32 = 0x3;

impl Drop for List {
    fn drop(&mut self) {
        set_pso_graphics(self, ptr::null());
        set_pso_compute(self, ptr::null());
        set_root_sig_graphics(self, ptr::null());
        set_root_sig_compute(self, ptr::null());
        if !self.allocator.is_null() {
            // SAFETY: `self.allocator` holds one private reference acquired in `reset`.
            unsafe { com::release_private_com(self.allocator) };
            self.allocator = ptr::null_mut();
        }
        for (obj, release_fn) in self.retained_objects.drain(..) {
            if !obj.is_null() {
                // SAFETY: `obj` was retained with `release_fn`.
                unsafe { release_fn(obj) };
            }
        }
        if !self.device.is_null() {
            // SAFETY: `self.device` holds one public reference acquired in `create_closed`.
            unsafe { com::release_com(self.device) };
            self.device = ptr::null_mut();
        }
    }
}

fn set_pso_graphics(list: &mut List, new_pso: *const Graphics) {
    if ptr::eq(list.pso_graphics, new_pso) {
        return;
    }
    if !new_pso.is_null() {
        // SAFETY: `new_pso` is a live `Graphics` COM object.
        unsafe { com::add_ref_private_com(new_pso.cast_mut()) };
    }
    if !list.pso_graphics.is_null() {
        let old = list.pso_graphics.cast_mut();
        list.pso_graphics = ptr::null();
        // SAFETY: `old` holds one private reference owned by `list.pso_graphics`.
        unsafe { com::release_private_com(old) };
    }
    list.pso_graphics = new_pso;
}

fn set_pso_compute(list: &mut List, new_pso: *const Compute) {
    if ptr::eq(list.pso_compute, new_pso) {
        return;
    }
    if !new_pso.is_null() {
        // SAFETY: `new_pso` is a live `Compute` COM object.
        unsafe { com::add_ref_private_com(new_pso.cast_mut()) };
    }
    if !list.pso_compute.is_null() {
        let old = list.pso_compute.cast_mut();
        list.pso_compute = ptr::null();
        // SAFETY: `old` holds one private reference owned by `list.pso_compute`.
        unsafe { com::release_private_com(old) };
    }
    list.pso_compute = new_pso;
}

fn set_root_sig_graphics(list: &mut List, new_rs: *const RootSignature) {
    if ptr::eq(list.root_sig_graphics, new_rs) {
        return;
    }
    if !new_rs.is_null() {
        // SAFETY: `new_rs` is a live `RootSignature` COM object.
        unsafe { com::add_ref_private_com(new_rs.cast_mut()) };
    }
    if !list.root_sig_graphics.is_null() {
        let old = list.root_sig_graphics.cast_mut();
        list.root_sig_graphics = ptr::null();
        // SAFETY: `old` holds one private reference owned by `list.root_sig_graphics`.
        unsafe { com::release_private_com(old) };
    }
    list.root_sig_graphics = new_rs;
}

fn set_root_sig_compute(list: &mut List, new_rs: *const RootSignature) {
    if ptr::eq(list.root_sig_compute, new_rs) {
        return;
    }
    if !new_rs.is_null() {
        // SAFETY: `new_rs` is a live `RootSignature` COM object.
        unsafe { com::add_ref_private_com(new_rs.cast_mut()) };
    }
    if !list.root_sig_compute.is_null() {
        let old = list.root_sig_compute.cast_mut();
        list.root_sig_compute = ptr::null();
        // SAFETY: `old` holds one private reference owned by `list.root_sig_compute`.
        unsafe { com::release_private_com(old) };
    }
    list.root_sig_compute = new_rs;
}

fn apply_initial_pso(list: &mut List, initial_state: *mut c_void) {
    set_pso_graphics(list, ptr::null());
    set_pso_compute(list, ptr::null());
    if initial_state.is_null() {
        return;
    }
    // SAFETY: `initial_state` is a valid `Graphics` or `Compute` pipeline state with `is_compute`
    // at identical struct offset.
    let is_compute = unsafe { (*initial_state.cast::<Compute>()).is_compute };
    if is_compute {
        set_pso_compute(list, initial_state.cast::<Compute>());
    } else {
        set_pso_graphics(list, initial_state.cast::<Graphics>());
    }
}

/// Resets recording state on `list` (`ResetState`, `d3d12_command_list.cpp:242`).
pub fn reset_state(list: &mut List, initial_state: *mut c_void) {
    apply_initial_pso(list, initial_state);
    list.render_pass = List::empty_render_pass();
    list.deferred_clears.clear();
    list.pending_resolves.clear();
    list.primitive_topology = 0;
    list.bound_pso = 0;
    list.bound_dss = u64::MAX;
    list.bound_rasterizer = [u32::MAX; 7];
    list.bound_vb_slot_mask = u32::MAX;
    list.bound_geometry = false;
    list.viewport_count = 0;
    list.viewports = [WMTViewport {
        origin_x: 0.0,
        origin_y: 0.0,
        width: 0.0,
        height: 0.0,
        znear: 0.0,
        zfar: 1.0,
    }; 16];
    list.scissor_rect_count = 0;
    list.scissor_rects = [WMTScissorRect {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    }; 16];
    list.blend_factor = [1.0; 4];
    list.stencil_ref = 0;
    list.active_visibility_offset = u64::MAX;
    list.render_pass_visibility_buffer = 0;
    set_root_sig_graphics(list, ptr::null());
    list.root_args_graphics = [0; 64];
    set_root_sig_compute(list, ptr::null());
    list.root_args_compute = [0; 64];
    list.bound_cbv_heap = ptr::null();
    list.bound_sampler_heap = ptr::null();
    list.vertex_buffers = [(0, 0, 0); 32];
    list.ib_cache = [(0, 0, 0); 4];
    list.ib_cache_next = 0;
    list.static_sampler_cache = [(ptr::null(), 0); 4];
    list.static_sampler_cache_next = 0;
    list.gpu_vb_table_cache = None;
    list.gpu_graphics_root_args_offset = None;
    list.gpu_compute_root_args_offset = None;
    list.index_buffer = 0;
    list.index_buffer_offset = 0;
    list.index_buffer_size = 0;
    list.index_buffer_format = u32::from(WMT_INDEX_TYPE_U_INT16);
    list.sm50_index_buffer_format = 0;
    list.dirty_state = DirtyFlags::empty();
    list.inside_render_pass = false;
}

const fn stage_heap_bases(list: &List, root_sig: *const RootSignature, staging: &mut [u64; 64]) {
    if root_sig.is_null() {
        return;
    }
    // SAFETY: `root_sig` is a valid `RootSignature` bound on `list`.
    let rs = unsafe { &*root_sig };
    let offset = rs.heap_qword_offset();
    if offset == u32::MAX {
        return;
    }
    let idx = offset as usize;
    if idx + 1 >= staging.len() {
        return;
    }
    let cbv_base = if list.bound_cbv_heap.is_null() {
        0
    } else {
        // SAFETY: `bound_cbv_heap` is a valid `Heap` bound on `list`.
        unsafe { (*list.bound_cbv_heap).gpu_address() }
    };
    let sampler_base = if list.bound_sampler_heap.is_null() {
        0
    } else {
        // SAFETY: `bound_sampler_heap` is a valid `Heap` bound on `list`.
        unsafe { (*list.bound_sampler_heap).gpu_address() }
    };
    staging[idx] = cbv_base;
    staging[idx + 1] = sampler_base;
}

fn record_pass_write(render_pass: &mut RenderEncoderData, resource: *const c_void) {
    if resource.is_null() {
        return;
    }
    let count = usize::from(render_pass.base.write_count);
    if count >= render_pass.base.writes.len() {
        render_pass.base.writes_unknown = true;
        return;
    }
    if render_pass.base.writes[..count].contains(&resource) {
        return;
    }
    render_pass.base.writes[count] = resource;
    render_pass.base.write_count += 1;
}

fn same_render_pass_targets(a: &RenderEncoderData, b: &RenderEncoderData) -> bool {
    if a.render_target_count != b.render_target_count
        || a.dsv_planar_flags != b.dsv_planar_flags
        || a.dsv_readonly_flags != b.dsv_readonly_flags
        || a.render_target_width != b.render_target_width
        || a.render_target_height != b.render_target_height
        || a.render_target_array_length != b.render_target_array_length
        || a.depth.attachment != b.depth.attachment
        || a.depth.depth_plane != b.depth.depth_plane
        || a.stencil.attachment != b.stencil.attachment
        || a.stencil.depth_plane != b.stencil.depth_plane
    {
        return false;
    }
    for i in 0..8 {
        if a.colors[i].attachment != b.colors[i].attachment
            || a.colors[i].depth_plane != b.colors[i].depth_plane
        {
            return false;
        }
    }
    true
}

/// Creates a closed command list (`CreateClosedCommandList`, `d3d12_command_list.cpp:2513`).
///
/// # Safety
///
/// `device`, `riid`, and `out` are caller-supplied COM pointers.
pub unsafe fn create_closed(
    device: *mut Device,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `out` is null or a writable out-pointer.
    unsafe { com::init_out_ptr(out) };
    if out.is_null() {
        return E_INVALIDARG;
    }
    if !device.is_null() {
        // SAFETY: `device` is a live `Device` COM object.
        unsafe { com::add_ref_com(device) };
    }
    let raw = Box::into_raw(List::new_closed(device));
    // SAFETY: `raw` is a freshly allocated `List` with 0 references; `query_interface` adds the
    // first public reference on success.
    let hr = unsafe { query_interface(raw.cast(), riid, out) };
    if hr < 0 {
        // SAFETY: `query_interface` did not add a reference on failure, so `raw` has 0 references.
        drop(unsafe { Box::from_raw(raw) });
    }
    hr
}

/// `ID3D12Device::CreateCommandList` (`d3d12_device.cpp:526`).
///
/// # Safety
///
/// `this`, `allocator`, `initial_state`, `riid`, and `list` are caller-supplied COM pointers.
pub unsafe fn create_command_list(
    this: *mut c_void,
    _node_mask: u32,
    _list_type: u32,
    allocator: *mut c_void,
    initial_state: *mut c_void,
    riid: *const c_void,
    list: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandList");
    // SAFETY: `list` is null or a writable out-pointer.
    unsafe { com::init_out_ptr(list) };
    if allocator.is_null() || list.is_null() {
        return E_INVALIDARG;
    }
    crate::scope!("list.CreateCommandList");
    // SAFETY: `this` is the receiver pointer or null.
    let dev_ptr = if unsafe { com::has_vtbl(this, &crate::device::vtbl::DEVICE_VTBL) } {
        this.cast::<Device>()
    } else {
        ptr::null_mut()
    };
    if !dev_ptr.is_null() {
        // SAFETY: `dev_ptr` is a live `Device` COM object.
        unsafe { com::add_ref_com(dev_ptr) };
    }
    let raw = Box::into_raw(List::new_closed(dev_ptr));
    // SAFETY: `raw` is a valid `List` in the closed state; `allocator` is non-null.
    let reset_hr = unsafe { initialize(raw.cast(), allocator, initial_state) };
    if reset_hr < 0 {
        // SAFETY: `raw` has 0 references on failure.
        drop(unsafe { Box::from_raw(raw) });
        return reset_hr;
    }
    // SAFETY: `raw` is a live `List`; `query_interface` adds the first public reference on success.
    let hr = unsafe { query_interface(raw.cast(), riid, list) };
    if hr < 0 {
        // SAFETY: `query_interface` did not add a reference on failure, so `raw` has 0 references.
        drop(unsafe { Box::from_raw(raw) });
    }
    hr
}

/// `ID3D12Device4::CreateCommandList1` (`d3d12_device.cpp:1577`).
///
/// # Safety
///
/// `this`, `riid`, and `list` are caller-supplied COM pointers.
pub unsafe fn create_command_list1(
    this: *mut c_void,
    _node_mask: u32,
    list_type: u32,
    _flags: u32,
    riid: *const c_void,
    list: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandList1");
    // SAFETY: `list` is null or a writable out-pointer.
    unsafe { com::init_out_ptr(list) };
    if !matches!(list_type, 0 | 2 | 3) {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is the receiver pointer or null.
    let dev_ptr = if unsafe { com::has_vtbl(this, &crate::device::vtbl::DEVICE_VTBL) } {
        this.cast::<Device>()
    } else {
        ptr::null_mut()
    };
    // SAFETY: `dev_ptr`, `riid`, and `list` are forwarded to `create_closed`.
    unsafe { create_closed(dev_ptr, riid, list) }
}

/// `ID3D12GraphicsCommandList::QueryInterface` (`d3d12_command_list.cpp:327`).
///
/// # Safety
///
/// `this`, `riid`, and `ppv_object` are caller-supplied COM pointers.
pub unsafe fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("list.QueryInterface");
    if ppv_object.is_null() {
        return E_POINTER;
    }
    // SAFETY: `ppv_object` is non-null.
    unsafe { com::init_out_ptr(ppv_object) };
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `riid` is null or points to a 16-byte `Guid`.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    let list_ptr = this.cast::<List>();
    if matches!(
        iid,
        IID_IUNKNOWN
            | IID_ID3D12_OBJECT
            | IID_ID3D12_DEVICE_CHILD
            | IID_ID3D12_COMMAND_LIST
            | IID_ID3D12_GRAPHICS_COMMAND_LIST
            | IID_ID3D12_GRAPHICS_COMMAND_LIST1
            | IID_ID3D12_GRAPHICS_COMMAND_LIST2
            | IID_ID3D12_GRAPHICS_COMMAND_LIST3
            | IID_ID3D12_GRAPHICS_COMMAND_LIST4
            | IID_ID3D12_GRAPHICS_COMMAND_LIST5
            | IID_ID3D12_GRAPHICS_COMMAND_LIST6
            | IID_ID3D12_GRAPHICS_COMMAND_LIST7
    ) {
        // SAFETY: `list_ptr` is a live `List`.
        unsafe { com::add_ref_com(list_ptr) };
        // SAFETY: `ppv_object` is non-null.
        unsafe { ppv_object.write_unaligned(this) };
        return S_OK;
    }
    if iid == IID_D3D12_COMMAND_LIST_EXT {
        // SAFETY: `list_ptr` is a live `List`.
        unsafe { com::add_ref_com(list_ptr) };
        // SAFETY: `List::EXT_OFFSET` is within `List`.
        let ext_ptr = unsafe { com::to_companion(list_ptr, List::EXT_OFFSET) };
        // SAFETY: `ppv_object` is non-null.
        unsafe { ppv_object.write_unaligned(ext_ptr) };
        return S_OK;
    }
    if com::qi_log(&IID_ID3D12_GRAPHICS_COMMAND_LIST, &iid) {
        crate::warn!("D3D12GraphicsCommandList: Unknown interface query {iid}");
    }
    E_NOINTERFACE
}

/// `IMTLD3D12CommandListExt::QueryInterface` (`d3d12_command_list.cpp:41`).
///
/// # Safety
///
/// `this` is a valid `List` companion pointer at [`List::EXT_OFFSET`].
pub unsafe fn ext_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    object: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is the `ext_vtbl` companion pointer at `List::EXT_OFFSET`.
    let list = unsafe { this.byte_sub(List::EXT_OFFSET) };
    // SAFETY: `list` is the enclosing `List` pointer.
    unsafe { query_interface(list, riid, object) }
}

/// `IMTLD3D12CommandListExt::AddRef` (`d3d12_command_list.cpp:41`).
///
/// # Safety
///
/// `this` is a valid `List` companion pointer at [`List::EXT_OFFSET`].
pub unsafe fn ext_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at `List::EXT_OFFSET` inside a live `List`.
    unsafe { com::companion_add_ref::<List, { List::EXT_OFFSET }>(this) }
}

/// `IMTLD3D12CommandListExt::Release` (`d3d12_command_list.cpp:41`).
///
/// # Safety
///
/// `this` is a valid `List` companion pointer at [`List::EXT_OFFSET`].
pub unsafe fn ext_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at `List::EXT_OFFSET` inside a live `List`.
    unsafe { com::companion_release::<List, { List::EXT_OFFSET }>(this) }
}

/// `ID3D12Object::GetPrivateData`.
///
/// # Safety
///
/// `this`, `guid`, `data_size`, and `data` are caller-supplied pointers.
pub unsafe fn get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `guid` is null or points to a `Guid`.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &*this.cast::<List>() };
    // SAFETY: caller supplies `data_size` and `data`.
    unsafe { list.private_data.get_data(guid, data_size, data) }
}

/// `ID3D12Object::SetPrivateData`.
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied pointers.
pub unsafe fn set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    data: *const c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `guid` is null or points to a `Guid`.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &*this.cast::<List>() };
    // SAFETY: caller supplies `data` of `data_size` bytes.
    unsafe { list.private_data.set_data(guid, data_size, data) }
}

/// `ID3D12Object::SetPrivateDataInterface`.
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied pointers.
pub unsafe fn set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    data: *const c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `guid` is null or points to a `Guid`.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &*this.cast::<List>() };
    // SAFETY: caller supplies `data` as an `IUnknown` pointer or null.
    unsafe { list.private_data.set_interface(guid, data.cast_mut()) }
}

/// `ID3D12Object::SetName`.
///
/// # Safety
///
/// `this` and `name` are caller-supplied pointers.
pub unsafe fn set_name(this: *mut c_void, name: *const c_void) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &*this.cast::<List>() };
    // SAFETY: `name` is null or a NUL-terminated UTF-16 string.
    unsafe { com::set_name_private_data(&list.private_data, name.cast::<u16>()) }
}

/// `ID3D12DeviceChild::GetDevice`.
///
/// # Safety
///
/// `this`, `riid`, and `device` are caller-supplied pointers.
pub unsafe fn get_device(
    this: *mut c_void,
    riid: *const c_void,
    device: *mut *mut c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &*this.cast::<List>() };
    if list.device.is_null() {
        // SAFETY: `device` is null or a writable out-pointer.
        unsafe { com::init_out_ptr(device) };
        return E_FAIL;
    }
    let dev_ptr = list.device.cast::<c_void>();
    // SAFETY: `dev_ptr` is a live `Device` whose first field is a vtable starting with `IUnknownVtbl`.
    let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev_ptr) };
    // SAFETY: forwards `QueryInterface` to the parent `Device`.
    unsafe { (unk.query_interface)(dev_ptr, riid, device) }
}

/// `ID3D12CommandList::GetType` (`d3d12_command_list.cpp:344`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn get_type(this: *mut c_void) -> u32 {
    crate::scope!("list.GetType");
    let _ = this.addr();
    D3D12_COMMAND_LIST_TYPE_DIRECT
}

/// `ID3D12GraphicsCommandList::Close` (`d3d12_command_list.cpp:349`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn close(this: *mut c_void) -> Hresult {
    crate::scope!("list.Close");
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.submission.encoder_count < usize::MAX || list.allocator.is_null() {
        return E_FAIL;
    }
    // SAFETY: `list.allocator` is non-null while recording.
    let alloc = unsafe { &mut *list.allocator };
    list.submission.barrier_count = alloc.barriers;
    let res = if alloc.device.is_null() {
        if alloc.encoder_last.is_null() {
            Err(E_FAIL)
        } else {
            if !alloc.encoder_current.is_null() {
                // SAFETY: `alloc.encoder_last` is a valid `EncoderData` node.
                unsafe { (*alloc.encoder_last).next = alloc.encoder_current };
                alloc.encoder_last = alloc.encoder_current;
                alloc.encoder_current = ptr::null_mut();
                alloc.encoder_count += 1;
            }
            alloc.encoder_last = ptr::null_mut();
            let count = alloc.encoder_count;
            alloc.encoder_count = 0;
            Ok(count)
        }
    } else {
        alloc.end_record()
    };
    match res {
        Ok(count) => {
            list.submission.encoder_count = count;
            S_OK
        }
        Err(hr) => hr,
    }
}

unsafe fn initialize(
    this: *mut c_void,
    allocator: *mut c_void,
    initial_state: *mut c_void,
) -> Hresult {
    if this.is_null() || allocator.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.submission.encoder_count == usize::MAX {
        return E_FAIL;
    }
    let new_alloc = allocator.cast::<Allocator>();
    if list.allocator != new_alloc {
        // SAFETY: `new_alloc` is a valid `Allocator` COM object.
        unsafe { com::add_ref_private_com(new_alloc) };
        if !list.allocator.is_null() {
            // SAFETY: `list.allocator` was previously retained with `add_ref_private_com`.
            unsafe { com::release_private_com(list.allocator) };
        }
        list.allocator = new_alloc;
    }
    reset_state(list, initial_state);
    list.submission.entry = ptr::null_mut();
    list.submission.encoder_count = 0;
    list.submission.barrier_count = 0;
    list.submission.timestamp_resolves.clear();
    list.submission.timestamps = false;
    list.submission.custom_resolves = false;
    list.submission.barrier_names.clear();
    list.submission.barrier_opaque.clear();
    // SAFETY: `new_alloc` is non-null and retained by `list`.
    let alloc = unsafe { &mut *new_alloc };
    let res = if alloc.device.is_null() {
        if alloc.encoder_last.is_null() {
            alloc.barriers = 0;
            alloc.joins.clear();
            alloc.joins.push(0);
            alloc.group.clear();
            alloc.transitions.clear();
            alloc.clears_from = ptr::null_mut();
            let mut head = Box::new(EncoderData::new(EncoderType::Null, 0));
            let head_ptr: *mut EncoderData = &raw mut *head;
            alloc.encoder_lists.push(head);
            alloc.encoder_last = head_ptr;
            Ok(head_ptr)
        } else {
            Err(E_INVALIDARG)
        }
    } else {
        alloc.start_record()
    };
    match res {
        Ok(head) => {
            list.submission.entry = head;
            list.submission.encoder_count = usize::MAX;
            S_OK
        }
        Err(hr) => hr,
    }
}

/// `ID3D12GraphicsCommandList::Reset` (`d3d12_command_list.cpp:355`).
///
/// # Safety
///
/// `this`, `allocator`, and `initial_state` are caller-supplied pointers.
pub unsafe fn reset(
    this: *mut c_void,
    allocator: *mut c_void,
    initial_state: *mut c_void,
) -> Hresult {
    crate::scope!("list.Reset");
    // SAFETY: `this`, `allocator`, and `initial_state` are forwarded to `initialize`.
    unsafe { initialize(this, allocator, initial_state) }
}

/// `ID3D12GraphicsCommandList::ClearState` (`d3d12_command_list.cpp:367`).
///
/// # Safety
///
/// `this` and `pipeline_state` are caller-supplied pointers.
pub unsafe fn clear_state(this: *mut c_void, pipeline_state: *mut c_void) {
    crate::scope!("list.ClearState");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    if !list.allocator.is_null() {
        // SAFETY: `list.allocator` is valid while recording.
        let alloc = unsafe { &mut *list.allocator };
        if !alloc.encoder_current.is_null() {
            alloc.invalidate_current_pass();
        }
    }
    reset_state(list, pipeline_state);
}

/// `ID3D12GraphicsCommandList::IASetPrimitiveTopology` (`d3d12_command_list.cpp:1266`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn ia_set_primitive_topology(this: *mut c_void, topology: u32) {
    crate::scope!("list.IASetPrimitiveTopology");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    list.primitive_topology = topology;
}

/// `ID3D12GraphicsCommandList::RSSetViewports` (`d3d12_command_list.cpp:1272`).
///
/// # Safety
///
/// `this` and `viewports` are caller-supplied pointers.
pub unsafe fn rs_set_viewports(this: *mut c_void, num_viewports: u32, viewports: *const c_void) {
    crate::scope!("list.RSSetViewports");
    if this.is_null()
        || (num_viewports as usize) > D3D12_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE
    {
        return;
    }
    if num_viewports > 0 && viewports.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let new_count = u8::try_from(num_viewports).unwrap_or(0);
    let mut changed = list.viewport_count != new_count;
    list.viewport_count = new_count;
    let vp_ptr = viewports.cast::<D3d12Viewport>();
    for i in 0..(num_viewports as usize) {
        // SAFETY: `i < num_viewports`, so `vp_ptr.add(i)` is in-bounds.
        let elem = unsafe { vp_ptr.add(i) };
        // SAFETY: `elem` points to a readable `D3d12Viewport`.
        let vp = unsafe { elem.read_unaligned() };
        let next = WMTViewport {
            origin_x: f64::from(vp.top_left_x),
            origin_y: f64::from(vp.top_left_y),
            width: f64::from(vp.width),
            height: f64::from(vp.height),
            znear: f64::from(vp.min_depth),
            zfar: f64::from(vp.max_depth),
        };
        let cur = &list.viewports[i];
        if cur.origin_x.to_bits() != next.origin_x.to_bits()
            || cur.origin_y.to_bits() != next.origin_y.to_bits()
            || cur.width.to_bits() != next.width.to_bits()
            || cur.height.to_bits() != next.height.to_bits()
            || cur.znear.to_bits() != next.znear.to_bits()
            || cur.zfar.to_bits() != next.zfar.to_bits()
        {
            list.viewports[i] = next;
            changed = true;
        }
    }
    if changed {
        list.dirty_state.insert(DirtyFlags::VIEWPORT);
    }
}

/// `ID3D12GraphicsCommandList::RSSetScissorRects` (`d3d12_command_list.cpp:1284`).
///
/// # Safety
///
/// `this` and `rects` are caller-supplied pointers.
pub unsafe fn rs_set_scissor_rects(this: *mut c_void, num_rects: u32, rects: *const c_void) {
    crate::scope!("list.RSSetScissorRects");
    if this.is_null()
        || (num_rects as usize) > D3D12_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE
    {
        return;
    }
    if num_rects > 0 && rects.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let new_count = u8::try_from(num_rects).unwrap_or(0);
    let mut changed = list.scissor_rect_count != new_count;
    list.scissor_rect_count = new_count;
    let rect_ptr = rects.cast::<D3d12Rect>();
    for i in 0..(num_rects as usize) {
        // SAFETY: `i < num_rects`, so `rect_ptr.add(i)` is in-bounds.
        let elem = unsafe { rect_ptr.add(i) };
        // SAFETY: `elem` points to a readable `D3d12Rect`.
        let r = unsafe { elem.read_unaligned() };
        let left = r.left.clamp(0, MAX_SCISSOR_COORD);
        let top = r.top.clamp(0, MAX_SCISSOR_COORD);
        let right = r.right.clamp(left, MAX_SCISSOR_COORD);
        let bottom = r.bottom.clamp(top, MAX_SCISSOR_COORD);
        let next = WMTScissorRect {
            x: u64::try_from(left).unwrap_or(0),
            y: u64::try_from(top).unwrap_or(0),
            width: u64::try_from(right - left).unwrap_or(0),
            height: u64::try_from(bottom - top).unwrap_or(0),
        };
        let cur = &list.scissor_rects[i];
        if cur.x != next.x
            || cur.y != next.y
            || cur.width != next.width
            || cur.height != next.height
        {
            list.scissor_rects[i] = next;
            changed = true;
        }
    }
    if changed {
        list.dirty_state.insert(DirtyFlags::SCISSOR);
    }
}

/// `ID3D12GraphicsCommandList::OMSetBlendFactor` (`d3d12_command_list.cpp:1296`).
///
/// # Safety
///
/// `this` and `blend_factor` are caller-supplied pointers.
pub unsafe fn om_set_blend_factor(this: *mut c_void, blend_factor: *const f32) {
    crate::scope!("list.OMSetBlendFactor");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let next = if blend_factor.is_null() {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        let mut arr = [0.0f32; 4];
        for (i, dst) in arr.iter_mut().enumerate() {
            // SAFETY: `blend_factor` points to 4 `f32` values when non-null.
            let elem = unsafe { blend_factor.add(i) };
            // SAFETY: `elem` points to a readable `f32`.
            *dst = unsafe { elem.read_unaligned() };
        }
        arr
    };
    if list.blend_factor[0].to_bits() != next[0].to_bits()
        || list.blend_factor[1].to_bits() != next[1].to_bits()
        || list.blend_factor[2].to_bits() != next[2].to_bits()
        || list.blend_factor[3].to_bits() != next[3].to_bits()
    {
        list.blend_factor = next;
        list.dirty_state.insert(DirtyFlags::BLEND_FACTOR);
    }
}

/// `ID3D12GraphicsCommandList::OMSetStencilRef` (`d3d12_command_list.cpp:1310`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn om_set_stencil_ref(this: *mut c_void, stencil_ref: u32) {
    crate::scope!("list.OMSetStencilRef");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let masked = stencil_ref & 0xff;
    if list.stencil_ref == masked {
        return;
    }
    list.stencil_ref = masked;
    list.dirty_state.insert(DirtyFlags::STENCIL_REF);
}

/// `ID3D12GraphicsCommandList::SetPipelineState` (`d3d12_command_list.cpp:1319`).
///
/// # Safety
///
/// `this` and `pipeline_state` are caller-supplied pointers.
pub unsafe fn set_pipeline_state(this: *mut c_void, pipeline_state: *mut c_void) {
    crate::scope!("list.SetPipelineState");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    if pipeline_state.is_null() {
        set_pso_graphics(list, ptr::null());
        set_pso_compute(list, ptr::null());
        list.dirty_state.insert(DirtyFlags::PSO);
        return;
    }
    // SAFETY: `pipeline_state` is a valid `Graphics` or `Compute` with `is_compute` at the same offset.
    let is_compute = unsafe { (*pipeline_state.cast::<Compute>()).is_compute };
    if is_compute {
        let compute_pso = pipeline_state.cast::<Compute>().cast_const();
        if ptr::eq(list.pso_compute, compute_pso) {
            return;
        }
        set_pso_compute(list, compute_pso);
        set_pso_graphics(list, ptr::null());
        list.dirty_state.insert(DirtyFlags::PSO);
        return;
    }
    let graphics_pso = pipeline_state.cast::<Graphics>().cast_const();
    if ptr::eq(list.pso_graphics, graphics_pso) {
        return;
    }
    set_pso_graphics(list, graphics_pso);
    set_pso_compute(list, ptr::null());
    // SAFETY: `graphics_pso` is a non-null `Graphics` pointer.
    let new_slot_mask = unsafe { (*graphics_pso).slot_mask };
    if new_slot_mask == list.bound_vb_slot_mask {
        list.dirty_state.insert(DirtyFlags::PSO);
    } else {
        list.dirty_state
            .insert(DirtyFlags::PSO | DirtyFlags::VERTEX_BUFFERS);
    }
}

/// `ID3D12GraphicsCommandList::ExecuteBundle` (`d3d12_command_list.cpp:1405`).
///
/// # Safety
///
/// `this` and `command_list` are caller-supplied pointers.
pub unsafe fn execute_bundle(_this: *mut c_void, _command_list: *mut c_void) {
    crate::scope!("list.ExecuteBundle");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ExecuteBundle");
}

/// `ID3D12GraphicsCommandList::SetDescriptorHeaps` (`d3d12_command_list.cpp:1415`).
///
/// # Safety
///
/// `this` and `descriptor_heaps` are caller-supplied pointers.
pub unsafe fn set_descriptor_heaps(
    this: *mut c_void,
    num_descriptor_heaps: u32,
    descriptor_heaps: *const *mut c_void,
) {
    crate::scope!("list.SetDescriptorHeaps");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let mut new_cbv: *const Heap = ptr::null();
    let mut new_sampler: *const Heap = ptr::null();
    if num_descriptor_heaps > 0 && !descriptor_heaps.is_null() {
        for i in 0..(num_descriptor_heaps as usize) {
            // SAFETY: `i < num_descriptor_heaps`, so `descriptor_heaps.add(i)` is in-bounds.
            let elem = unsafe { descriptor_heaps.add(i) };
            // SAFETY: `elem` points to a readable pointer.
            let raw = unsafe { elem.read_unaligned() };
            if raw.is_null() {
                continue;
            }
            let heap_ptr = raw.cast::<Heap>().cast_const();
            // SAFETY: `heap_ptr` is a valid `Heap`.
            let heap_type = crate::descriptor_heap::desc_of(unsafe { &*heap_ptr }).r#type;
            if heap_type == D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER {
                new_sampler = heap_ptr;
            } else {
                new_cbv = heap_ptr;
            }
        }
    }
    if ptr::eq(new_cbv, list.bound_cbv_heap) && ptr::eq(new_sampler, list.bound_sampler_heap) {
        return;
    }
    list.bound_cbv_heap = new_cbv;
    list.bound_sampler_heap = new_sampler;
    let mut comp_staging = list.root_args_compute;
    stage_heap_bases(list, list.root_sig_compute, &mut comp_staging);
    list.root_args_compute = comp_staging;

    let mut gfx_staging = list.root_args_graphics;
    stage_heap_bases(list, list.root_sig_graphics, &mut gfx_staging);
    list.root_args_graphics = gfx_staging;

    list.gpu_compute_root_args_offset = None;
    list.gpu_graphics_root_args_offset = None;
    list.dirty_state.insert(
        DirtyFlags::COMPUTE_ROOT_ARGS | DirtyFlags::ROOT_ARGS | DirtyFlags::DESCRIPTOR_HEAP,
    );
}

/// `ID3D12GraphicsCommandList::SetComputeRootSignature` (`d3d12_command_list.cpp:1428`).
///
/// # Safety
///
/// `this` and `root_signature` are caller-supplied pointers.
pub unsafe fn set_compute_root_signature(this: *mut c_void, root_signature: *mut c_void) {
    crate::scope!("list.SetComputeRootSignature");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let rs_ptr = root_signature.cast::<RootSignature>().cast_const();
    if ptr::eq(list.root_sig_compute, rs_ptr) {
        return;
    }
    set_root_sig_compute(list, rs_ptr);
    if !rs_ptr.is_null() {
        let mut staging = list.root_args_compute;
        stage_heap_bases(list, rs_ptr, &mut staging);
        // SAFETY: `rs_ptr` is a valid `RootSignature`.
        let lock_offset = unsafe { (*rs_ptr).lock_qword_offset() } as usize;
        if lock_offset < staging.len() && !list.device.is_null() {
            // SAFETY: `list.device` is a valid `Device`.
            staging[lock_offset] = unsafe { null::atomic64_locks(&*list.device) };
        }
        list.root_args_compute = staging;
    }
    list.gpu_compute_root_args_offset = None;
    // See the graphics setter: the cache is keyed by the signature's address.
    list.static_sampler_cache = [(ptr::null(), 0); 4];
    list.static_sampler_cache_next = 0;
    list.dirty_state
        .insert(DirtyFlags::COMPUTE_ROOT_ARGS | DirtyFlags::COMPUTE_STATIC_SAMPLERS);
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootSignature` (`d3d12_command_list.cpp:1444`).
///
/// # Safety
///
/// `this` and `root_signature` are caller-supplied pointers.
pub unsafe fn set_graphics_root_signature(this: *mut c_void, root_signature: *mut c_void) {
    crate::scope!("list.SetGraphicsRootSignature");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let rs_ptr = root_signature.cast::<RootSignature>().cast_const();
    if ptr::eq(list.root_sig_graphics, rs_ptr) {
        return;
    }
    set_root_sig_graphics(list, rs_ptr);
    if !rs_ptr.is_null() {
        let mut staging = list.root_args_graphics;
        stage_heap_bases(list, rs_ptr, &mut staging);
        // SAFETY: `rs_ptr` is a valid `RootSignature`.
        let lock_offset = unsafe { (*rs_ptr).lock_qword_offset() } as usize;
        if lock_offset < staging.len() && !list.device.is_null() {
            // SAFETY: `list.device` is a valid `Device`.
            staging[lock_offset] = unsafe { null::atomic64_locks(&*list.device) };
        }
        list.root_args_graphics = staging;
    }
    list.gpu_graphics_root_args_offset = None;
    // The cache is keyed by the root signature's address; an address can be reused after the
    // signature is released, so a new signature must not find an old signature's samplers.
    list.static_sampler_cache = [(ptr::null(), 0); 4];
    list.static_sampler_cache_next = 0;
    list.dirty_state
        .insert(DirtyFlags::ROOT_ARGS | DirtyFlags::STATIC_SAMPLERS);
}

fn slot_qword_offset(root_sig: *const RootSignature, index: u32) -> Option<usize> {
    if root_sig.is_null() {
        return None;
    }
    // SAFETY: `root_sig` is a valid `RootSignature`.
    let rs = unsafe { &*root_sig };
    let offsets = rs.slot_qword_offsets();
    let idx = index as usize;
    offsets.get(idx).map(|&off| off as usize)
}

/// `ID3D12GraphicsCommandList::SetComputeRootDescriptorTable` (`d3d12_command_list.cpp:1459`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_descriptor_table(
    this: *mut c_void,
    root_parameter_index: u32,
    base_descriptor: D3d12GpuDescriptorHandle,
) {
    crate::scope!("list.SetComputeRootDescriptorTable");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_compute, root_parameter_index) else {
        return;
    };
    if qword < list.root_args_compute.len() && list.root_args_compute[qword] != base_descriptor.ptr
    {
        list.root_args_compute[qword] = base_descriptor.ptr;
        list.gpu_compute_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::COMPUTE_ROOT_ARGS);
    }
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootDescriptorTable` (`d3d12_command_list.cpp:1470`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_descriptor_table(
    this: *mut c_void,
    root_parameter_index: u32,
    base_descriptor: D3d12GpuDescriptorHandle,
) {
    crate::scope!("list.SetGraphicsRootDescriptorTable");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_graphics, root_parameter_index) else {
        return;
    };
    if qword < list.root_args_graphics.len()
        && list.root_args_graphics[qword] != base_descriptor.ptr
    {
        list.root_args_graphics[qword] = base_descriptor.ptr;
        list.gpu_graphics_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::ROOT_ARGS);
    }
}

fn write_u32_in_qwords(
    staging: &mut [u64; 64],
    qword_base: usize,
    dword_offset: u32,
    value: u32,
) -> bool {
    let total_dword = qword_base
        .saturating_mul(2)
        .saturating_add(dword_offset as usize);
    let qword_idx = total_dword / 2;
    if qword_idx >= staging.len() {
        return false;
    }
    let shift = (total_dword % 2) * 32;
    let mask = !(0xffff_ffff_u64 << shift);
    let next = (staging[qword_idx] & mask) | (u64::from(value) << shift);
    if staging[qword_idx] == next {
        return false;
    }
    staging[qword_idx] = next;
    true
}

/// `ID3D12GraphicsCommandList::SetComputeRoot32BitConstant` (`d3d12_command_list.cpp:1480`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root32_bit_constant(
    this: *mut c_void,
    root_parameter_index: u32,
    src_data: u32,
    dest_offset_in32_bit_values: u32,
) {
    crate::scope!("list.SetComputeRoot32BitConstant");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_compute, root_parameter_index) else {
        return;
    };
    if write_u32_in_qwords(
        &mut list.root_args_compute,
        qword,
        dest_offset_in32_bit_values,
        src_data,
    ) {
        list.gpu_compute_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::COMPUTE_ROOT_ARGS);
    }
}

/// `ID3D12GraphicsCommandList::SetGraphicsRoot32BitConstant` (`d3d12_command_list.cpp:1492`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root32_bit_constant(
    this: *mut c_void,
    root_parameter_index: u32,
    src_data: u32,
    dest_offset_in32_bit_values: u32,
) {
    crate::scope!("list.SetGraphicsRoot32BitConstant");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_graphics, root_parameter_index) else {
        return;
    };
    if write_u32_in_qwords(
        &mut list.root_args_graphics,
        qword,
        dest_offset_in32_bit_values,
        src_data,
    ) {
        list.gpu_graphics_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::ROOT_ARGS);
    }
}

/// `ID3D12GraphicsCommandList::SetComputeRoot32BitConstants` (`d3d12_command_list.cpp:1504`).
///
/// # Safety
///
/// `this` and `src_data` are caller-supplied pointers.
pub unsafe fn set_compute_root32_bit_constants(
    this: *mut c_void,
    root_parameter_index: u32,
    num32_bit_values_to_set: u32,
    src_data: *const c_void,
    dest_offset_in32_bit_values: u32,
) {
    crate::scope!("list.SetComputeRoot32BitConstants");
    if this.is_null() || (num32_bit_values_to_set > 0 && src_data.is_null()) {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_compute, root_parameter_index) else {
        return;
    };
    let src_u32 = src_data.cast::<u32>();
    let count = usize::try_from(num32_bit_values_to_set).unwrap_or(0);
    let start = qword
        .saturating_mul(2)
        .saturating_add(dest_offset_in32_bit_values as usize);
    if count > 0
        && start
            .checked_add(count)
            .is_some_and(|end| end <= list.root_args_compute.len() * 2)
    {
        // SAFETY: `src_u32` is valid for `count` u32 values (the caller's array).
        let src = unsafe { core::slice::from_raw_parts(src_u32, count) };
        let base = list.root_args_compute.as_mut_ptr().cast::<u32>();
        // SAFETY: the bounds check above keeps `start..start + count` inside the qword array.
        let dst_ptr = unsafe { base.add(start) };
        // SAFETY: `dst_ptr` points at `count` consecutive u32 values in `root_args_compute`.
        let dst = unsafe { core::slice::from_raw_parts_mut(dst_ptr, count) };
        if dst != src {
            dst.copy_from_slice(src);
            list.gpu_compute_root_args_offset = None;
            list.dirty_state.insert(DirtyFlags::COMPUTE_ROOT_ARGS);
        }
        return;
    }
    let mut changed = false;
    for i in 0..num32_bit_values_to_set {
        // SAFETY: `i < num32_bit_values_to_set`, so `src_u32.add(i as usize)` is in-bounds.
        let elem = unsafe { src_u32.add(i as usize) };
        // SAFETY: `elem` points to a readable `u32`.
        let val = unsafe { elem.read_unaligned() };
        if write_u32_in_qwords(
            &mut list.root_args_compute,
            qword,
            dest_offset_in32_bit_values.saturating_add(i),
            val,
        ) {
            changed = true;
        }
    }
    if changed {
        list.gpu_compute_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::COMPUTE_ROOT_ARGS);
    }
}

/// `ID3D12GraphicsCommandList::SetGraphicsRoot32BitConstants` (`d3d12_command_list.cpp:1519`).
///
/// # Safety
///
/// `this` and `src_data` are caller-supplied pointers.
pub unsafe fn set_graphics_root32_bit_constants(
    this: *mut c_void,
    root_parameter_index: u32,
    num32_bit_values_to_set: u32,
    src_data: *const c_void,
    dest_offset_in32_bit_values: u32,
) {
    crate::scope!("list.SetGraphicsRoot32BitConstants");
    if this.is_null() || (num32_bit_values_to_set > 0 && src_data.is_null()) {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_graphics, root_parameter_index) else {
        return;
    };
    let src_u32 = src_data.cast::<u32>();
    let count = usize::try_from(num32_bit_values_to_set).unwrap_or(0);
    let start = qword
        .saturating_mul(2)
        .saturating_add(dest_offset_in32_bit_values as usize);
    if count > 0
        && start
            .checked_add(count)
            .is_some_and(|end| end <= list.root_args_graphics.len() * 2)
    {
        // SAFETY: `src_u32` is valid for `count` u32 values (the caller's array).
        let src = unsafe { core::slice::from_raw_parts(src_u32, count) };
        let base = list.root_args_graphics.as_mut_ptr().cast::<u32>();
        // SAFETY: the bounds check above keeps `start..start + count` inside the qword array.
        let dst_ptr = unsafe { base.add(start) };
        // SAFETY: `dst_ptr` points at `count` consecutive u32 values in `root_args_graphics`.
        let dst = unsafe { core::slice::from_raw_parts_mut(dst_ptr, count) };
        if dst != src {
            dst.copy_from_slice(src);
            list.gpu_graphics_root_args_offset = None;
            list.dirty_state.insert(DirtyFlags::ROOT_ARGS);
        }
        return;
    }
    let mut changed = false;
    for i in 0..num32_bit_values_to_set {
        // SAFETY: `i < num32_bit_values_to_set`, so `src_u32.add(i as usize)` is in-bounds.
        let elem = unsafe { src_u32.add(i as usize) };
        // SAFETY: `elem` points to a readable `u32`.
        let val = unsafe { elem.read_unaligned() };
        if write_u32_in_qwords(
            &mut list.root_args_graphics,
            qword,
            dest_offset_in32_bit_values.saturating_add(i),
            val,
        ) {
            changed = true;
        }
    }
    if changed {
        list.gpu_graphics_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::ROOT_ARGS);
    }
}

fn set_compute_root_va(this: *mut c_void, index: u32, va: u64) {
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_compute, index) else {
        return;
    };
    if qword < list.root_args_compute.len() && list.root_args_compute[qword] != va {
        list.root_args_compute[qword] = va;
        list.gpu_compute_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::COMPUTE_ROOT_ARGS);
    }
}

fn set_graphics_root_va(this: *mut c_void, index: u32, va: u64) {
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let Some(qword) = slot_qword_offset(list.root_sig_graphics, index) else {
        return;
    };
    if qword < list.root_args_graphics.len() && list.root_args_graphics[qword] != va {
        list.root_args_graphics[qword] = va;
        list.gpu_graphics_root_args_offset = None;
        list.dirty_state.insert(DirtyFlags::ROOT_ARGS);
    }
}

/// `ID3D12GraphicsCommandList::SetComputeRootConstantBufferView` (`d3d12_command_list.cpp:1533`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_constant_buffer_view(
    this: *mut c_void,
    root_parameter_index: u32,
    buffer_location: u64,
) {
    crate::scope!("list.SetComputeRootConstantBufferView");
    set_compute_root_va(this, root_parameter_index, buffer_location);
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootConstantBufferView` (`d3d12_command_list.cpp:1544`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_constant_buffer_view(
    this: *mut c_void,
    root_parameter_index: u32,
    buffer_location: u64,
) {
    crate::scope!("list.SetGraphicsRootConstantBufferView");
    set_graphics_root_va(this, root_parameter_index, buffer_location);
}

/// `ID3D12GraphicsCommandList::SetComputeRootShaderResourceView` (`d3d12_command_list.cpp:1554`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_shader_resource_view(
    this: *mut c_void,
    root_parameter_index: u32,
    buffer_location: u64,
) {
    crate::scope!("list.SetComputeRootShaderResourceView");
    set_compute_root_va(this, root_parameter_index, buffer_location);
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootShaderResourceView` (`d3d12_command_list.cpp:1565`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_shader_resource_view(
    this: *mut c_void,
    root_parameter_index: u32,
    buffer_location: u64,
) {
    crate::scope!("list.SetGraphicsRootShaderResourceView");
    set_graphics_root_va(this, root_parameter_index, buffer_location);
}

/// `ID3D12GraphicsCommandList::SetComputeRootUnorderedAccessView` (`d3d12_command_list.cpp:1575`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_unordered_access_view(
    this: *mut c_void,
    root_parameter_index: u32,
    buffer_location: u64,
) {
    crate::scope!("list.SetComputeRootUnorderedAccessView");
    set_compute_root_va(this, root_parameter_index, buffer_location);
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootUnorderedAccessView` (`d3d12_command_list.cpp:1586`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_unordered_access_view(
    this: *mut c_void,
    root_parameter_index: u32,
    buffer_location: u64,
) {
    crate::scope!("list.SetGraphicsRootUnorderedAccessView");
    set_graphics_root_va(this, root_parameter_index, buffer_location);
}

/// `ID3D12GraphicsCommandList::IASetIndexBuffer` (`d3d12_command_list.cpp:1597`).
///
/// # Safety
///
/// `this` and `view` are caller-supplied pointers.
pub unsafe fn ia_set_index_buffer(this: *mut c_void, view: *const c_void) {
    crate::scope!("list.IASetIndexBuffer");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    if !view.is_null() && !list.device.is_null() {
        // SAFETY: `view` points to a readable `D3d12IndexBufferView`.
        let ibv = unsafe { view.cast::<D3d12IndexBufferView>().read_unaligned() };
        let va = ibv.buffer_location;
        if va != 0 {
            if va == list.index_buffer_size
                && list.index_buffer != 0
                && ibv.format == list.sm50_index_buffer_format
            {
                return;
            }
            let mut found: Option<(u64, u64)> = None;
            for &(base_va, end_va, mtl_buf) in &list.ib_cache {
                if base_va != 0 && va >= base_va && va < end_va {
                    found = Some((mtl_buf, va - base_va));
                    break;
                }
            }
            if found.is_none() {
                // SAFETY: `list.device` is a valid `Device`.
                let dev = unsafe { &*list.device };
                if let Some((buf, offset)) = residency::lookup_buffer_by_va(dev, va) {
                    let mtl_buf = buf.current().map_or(0, |a| a.buffer());
                    let base_va = va.wrapping_sub(offset);
                    let end_va = base_va.saturating_add(buf.length());
                    if mtl_buf != 0 && end_va > base_va {
                        let slot = usize::from(list.ib_cache_next) & 3;
                        list.ib_cache[slot] = (base_va, end_va, mtl_buf);
                        list.ib_cache_next = list.ib_cache_next.wrapping_add(1);
                    }
                    found = Some((mtl_buf, offset));
                }
            }
            if let Some((mtl_buf, offset)) = found {
                list.index_buffer_size = va;
                list.index_buffer = mtl_buf;
                list.index_buffer_offset = offset;
                list.index_buffer_format = if ibv.format == DXGI_FORMAT_R32_UINT {
                    u32::from(WMT_INDEX_TYPE_U_INT32)
                } else {
                    u32::from(WMT_INDEX_TYPE_U_INT16)
                };
                list.sm50_index_buffer_format = ibv.format;
                list.dirty_state.insert(DirtyFlags::INDEX_BUFFER);
                return;
            }
        }
    }
    list.index_buffer_size = 0;
    list.index_buffer = 0;
    list.index_buffer_offset = 0;
    list.index_buffer_format = 0;
    list.sm50_index_buffer_format = 0;
}

/// `ID3D12GraphicsCommandList::IASetVertexBuffers` (`d3d12_command_list.cpp:1613`).
///
/// # Safety
///
/// `this` and `views` are caller-supplied pointers.
pub unsafe fn ia_set_vertex_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_views: u32,
    views: *const c_void,
) {
    crate::scope!("list.IASetVertexBuffers");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let start = start_slot as usize;
    let end = start
        .saturating_add(num_views as usize)
        .min(list.vertex_buffers.len());
    let vbv_ptr = views.cast::<D3d12VertexBufferView>();
    let mut changed = false;
    for slot in start..end {
        let next = if vbv_ptr.is_null() {
            (0, 0, 0)
        } else {
            // SAFETY: `slot - start < num_views`, so `vbv_ptr.add(slot - start)` is in-bounds.
            let elem = unsafe { vbv_ptr.add(slot - start) };
            // SAFETY: `elem` points to a readable `D3d12VertexBufferView`.
            let v = unsafe { elem.read_unaligned() };
            (v.buffer_location, v.stride_in_bytes, v.size_in_bytes)
        };
        if list.vertex_buffers[slot] != next {
            list.vertex_buffers[slot] = next;
            changed = true;
        }
    }
    if changed {
        list.bound_vb_slot_mask = u32::MAX;
        list.gpu_vb_table_cache = None;
        list.dirty_state.insert(DirtyFlags::VERTEX_BUFFERS);
    }
}

/// `ID3D12GraphicsCommandList::SOSetTargets` (`d3d12_command_list.cpp:1622`).
///
/// # Safety
///
/// `this` and `views` are caller-supplied pointers.
pub unsafe fn so_set_targets(
    _this: *mut c_void,
    _start_slot: u32,
    _num_views: u32,
    _views: *const c_void,
) {
    crate::scope!("list.SOSetTargets");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SOSetTargets");
}

/// `ID3D12GraphicsCommandList::OMSetRenderTargets` (`d3d12_command_list.cpp:1628`).
///
/// # Safety
///
/// `this`, `render_target_descriptors`, and `depth_stencil_descriptor` are caller-supplied
/// pointers.
pub unsafe fn om_set_render_targets(
    this: *mut c_void,
    num_render_target_descriptors: u32,
    render_target_descriptors: *const c_void,
    rts_single_handle_to_descriptor_range: i32,
    depth_stencil_descriptor: *const c_void,
) {
    crate::scope!("list.OMSetRenderTargets");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a live `List`.
    let list = unsafe { &mut *this.cast::<List>() };
    let mut new_pass = List::empty_render_pass();

    let count =
        (num_render_target_descriptors as usize).min(D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT);
    new_pass.render_target_count = u8::try_from(count).unwrap_or(0);

    let mut width = DEFAULT_RENDER_TARGET_DIM;
    let mut height = DEFAULT_RENDER_TARGET_DIM;
    let mut array_length = 0u32;

    if count > 0 && !render_target_descriptors.is_null() {
        let rtv_ptr = render_target_descriptors.cast::<D3d12CpuDescriptorHandle>();
        for i in 0..count {
            let rtv = if rts_single_handle_to_descriptor_range != 0 {
                // SAFETY: `rtv_ptr` points to a readable base `D3d12CpuDescriptorHandle`.
                let base = unsafe { rtv_ptr.read_unaligned() };
                D3d12CpuDescriptorHandle {
                    ptr: base.ptr.wrapping_add(i * RTV_DSV_HEAP_INCREMENTAL_SIZE),
                }
            } else {
                // SAFETY: `i < count`, so `rtv_ptr.add(i)` is in-bounds.
                let elem = unsafe { rtv_ptr.add(i) };
                // SAFETY: `elem` points to a readable `D3d12CpuDescriptorHandle`.
                unsafe { elem.read_unaligned() }
            };
            if rtv.ptr == 0 {
                continue;
            }
            // SAFETY: `rtv` is an application-supplied RTV CPU descriptor handle.
            let Some(desc) = (unsafe { descriptor_heap::render_target(rtv) }) else {
                continue;
            };
            let Some(tex) = desc.texture else {
                continue;
            };
            record_pass_write(&mut new_pass, Arc::as_ptr(&tex).cast::<c_void>());
            let color = &mut new_pass.colors[i];
            color.attachment = TextureViewRef::from_texture(&tex, desc.view);
            color.depth_plane = desc.depth_plane;
            color.load_action = WMT_LOAD_ACTION_LOAD;
            color.store_action = WMT_STORE_ACTION_STORE;
            width = width.min(desc.width);
            height = height.min(desc.height);
            array_length = array_length.max(desc.render_target_array_length);
        }
    }

    if !depth_stencil_descriptor.is_null() {
        // SAFETY: `depth_stencil_descriptor` points to a readable `D3d12CpuDescriptorHandle`.
        let dsv = unsafe {
            depth_stencil_descriptor
                .cast::<D3d12CpuDescriptorHandle>()
                .read_unaligned()
        };
        if dsv.ptr != 0 {
            // SAFETY: `dsv` is an application-supplied DSV CPU descriptor handle.
            if let Some(desc) = unsafe { descriptor_heap::render_target(dsv) }
                && let Some(tex) = desc.texture
            {
                record_pass_write(&mut new_pass, Arc::as_ptr(&tex).cast::<c_void>());
                let planar =
                    u8::try_from(depth_stencil_planar_flags(desc.view.format())).unwrap_or(0);
                let view_ref = TextureViewRef::from_texture(&tex, desc.view);
                if (planar & 1) != 0 {
                    new_pass.depth.attachment.clone_from(&view_ref);
                    new_pass.depth.depth_plane = 0;
                    new_pass.depth.load_action = WMT_LOAD_ACTION_LOAD;
                    new_pass.depth.store_action = WMT_STORE_ACTION_STORE;
                }
                if (planar & 2) != 0 {
                    new_pass.stencil.attachment = view_ref;
                    new_pass.stencil.depth_plane = 0;
                    new_pass.stencil.load_action = WMT_LOAD_ACTION_LOAD;
                    new_pass.stencil.store_action = WMT_STORE_ACTION_STORE;
                }
                new_pass.dsv_planar_flags = planar;
                new_pass.dsv_readonly_flags =
                    u8::try_from(desc.flags & D3D12_DSV_READONLY_MASK).unwrap_or(0);
                width = width.min(desc.width);
                height = height.min(desc.height);
                array_length = array_length.max(desc.render_target_array_length);
            }
        }
    }

    new_pass.render_target_width = width;
    new_pass.render_target_height = height;
    new_pass.render_target_array_length = u16::try_from(array_length).unwrap_or(u16::MAX);

    if list.inside_render_pass
        && list.deferred_clears.is_empty()
        && !list.allocator.is_null()
        && same_render_pass_targets(&list.render_pass, &new_pass)
    {
        // SAFETY: `list.allocator` is valid while recording.
        let alloc = unsafe { &*list.allocator };
        if !alloc.encoder_current.is_null()
            // SAFETY: `alloc.encoder_current` is a valid `EncoderData`.
            && unsafe { (*alloc.encoder_current).kind } == EncoderType::Render
        {
            return;
        }
    }

    if !list.allocator.is_null() {
        // SAFETY: `list.allocator` is valid while recording.
        let alloc = unsafe { &mut *list.allocator };
        if !alloc.encoder_current.is_null() {
            alloc.invalidate_current_pass();
        }
    }
    list.inside_render_pass = false;
    list.render_pass = new_pass;
}

/// `ID3D12GraphicsCommandList::SetPredication` (`d3d12_command_list.cpp:1977`).
///
/// # Safety
///
/// `this` and `buffer` are caller-supplied pointers.
pub unsafe fn set_predication(
    _this: *mut c_void,
    _buffer: *mut c_void,
    _aligned_buffer_offset: u64,
    _operation: u32,
) {
    crate::scope!("list.SetPredication");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetPredication");
}

/// `ID3D12GraphicsCommandList::SetMarker` (`d3d12_command_list.cpp:1983`).
///
/// # Safety
///
/// `this` and `data` are caller-supplied pointers.
pub unsafe fn set_marker(this: *mut c_void, _metadata: u32, _data: *const c_void, _size: u32) {
    crate::scope!("list.SetMarker");
    let _ = this.addr();
}

/// `ID3D12GraphicsCommandList::BeginEvent` (`d3d12_command_list.cpp:1986`).
///
/// # Safety
///
/// `this` and `data` are caller-supplied pointers.
pub unsafe fn begin_event(this: *mut c_void, _metadata: u32, _data: *const c_void, _size: u32) {
    crate::scope!("list.BeginEvent");
    let _ = this.addr();
}

/// `ID3D12GraphicsCommandList::EndEvent` (`d3d12_command_list.cpp:1989`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn end_event(this: *mut c_void) {
    crate::scope!("list.EndEvent");
    let _ = this.addr();
}

/// `ID3D12GraphicsCommandList1::OMSetDepthBounds` (`d3d12_command_list.cpp:2114`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn om_set_depth_bounds(_this: *mut c_void, min: f32, max: f32) {
    crate::scope!("list.OMSetDepthBounds");
    crate::warn!("OMSetDepthBounds: ignoring ({min}, {max})");
}

/// `ID3D12GraphicsCommandList1::SetSamplePositions` (`d3d12_command_list.cpp:2120`).
///
/// # Safety
///
/// `this` and `sample_positions` are caller-supplied pointers.
pub unsafe fn set_sample_positions(
    _this: *mut c_void,
    _num_samples_per_pixel: u32,
    _num_pixels: u32,
    _sample_positions: *mut c_void,
) {
    crate::scope!("list.SetSamplePositions");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetSamplePositions");
}

/// `ID3D12GraphicsCommandList1::SetViewInstanceMask` (`d3d12_command_list.cpp:2207`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_view_instance_mask(_this: *mut c_void, _mask: u32) {
    crate::scope!("list.SetViewInstanceMask");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetViewInstanceMask");
}

/// `ID3D12GraphicsCommandList3::SetProtectedResourceSession` (`d3d12_command_list.cpp:2222`).
///
/// # Safety
///
/// `this` and `session` are caller-supplied pointers.
pub unsafe fn set_protected_resource_session(_this: *mut c_void, session: *mut c_void) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.SetProtectedResourceSession");
    if !session.is_null() && !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!("SetProtectedResourceSession: protected sessions not supported");
    }
}

/// `ID3D12GraphicsCommandList4::SetPipelineState1` (`d3d12_command_list.cpp:2330`).
///
/// # Safety
///
/// `this` and `state_object` are caller-supplied pointers.
pub unsafe fn set_pipeline_state1(_this: *mut c_void, state_object: *mut c_void) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.SetPipelineState1");
    if !state_object.is_null() && !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!(
            "SetPipelineState1: state objects not supported (RaytracingTier NOT_SUPPORTED)"
        );
    }
}

/// `ID3D12GraphicsCommandList5::RSSetShadingRate` (`d3d12_command_list.cpp:2343`).
///
/// # Safety
///
/// `this` and `combiners` are caller-supplied pointers.
pub unsafe fn rs_set_shading_rate(
    _this: *mut c_void,
    base_shading_rate: u32,
    combiners: *const u32,
) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.RSSetShadingRate");
    if (base_shading_rate != D3D12_SHADING_RATE_1X1 || !combiners.is_null())
        && !WARNED.swap(true, Ordering::Relaxed)
    {
        crate::warn!(
            "RSSetShadingRate: variable rate shading not supported (VariableShadingRateTier NOT_SUPPORTED)"
        );
    }
}

/// `ID3D12GraphicsCommandList5::RSSetShadingRateImage` (`d3d12_command_list.cpp:2350`).
///
/// # Safety
///
/// `this` and `shading_rate_image` are caller-supplied pointers.
pub unsafe fn rs_set_shading_rate_image(_this: *mut c_void, shading_rate_image: *mut c_void) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    crate::scope!("list.RSSetShadingRateImage");
    if !shading_rate_image.is_null() && !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!(
            "RSSetShadingRateImage: variable rate shading not supported (VariableShadingRateTier NOT_SUPPORTED)"
        );
    }
}
