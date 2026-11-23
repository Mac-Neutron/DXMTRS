use core::{ffi::c_void, ptr};

use crate::{
    abi::{
        agility::{D3D12_SAMPLER_FLAG_UINT_BORDER_COLOR, SamplerBorderColor, SamplerDesc2},
        objects::{
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, DescriptorHeapDesc,
        },
        resource::ResourceAllocationInfo,
        vtbl::D3d12CpuDescriptorHandle,
    },
    com::{E_FAIL, E_INVALIDARG, E_NOTIMPL, IID_ID3D12_DESCRIPTOR_HEAP, S_OK, release_com},
    descriptor_heap::{EmbeddedDescriptorHandle, Heap},
    device::{
        Device,
        vtbl::{DEVICE_EXT_VTBL, DEVICE_VTBL},
    },
    unimpl,
};

const ZERO_HANDLE: D3d12CpuDescriptorHandle = D3d12CpuDescriptorHandle { ptr: 0 };

fn assert_reported(expected: &str) {
    assert_eq!(unimpl::take_last_reported().as_deref(), Some(expected));
}

#[test]
fn every_slot_dispatches_to_its_lane() {
    let device = Box::into_raw(Device::new_dummy());
    let this = device.cast::<c_void>();
    let n = ptr::null();
    let nm = ptr::null_mut();
    let mut out: *mut c_void = ptr::null_mut();

    // SAFETY: `this` is a valid `Device` pointer allocated by `Box::into_raw`.
    let rc_up = unsafe { (DEVICE_VTBL.base.add_ref)(this) };
    assert_eq!(rc_up, 2);
    // SAFETY: `this` has refcount 2.
    let rc_down = unsafe { (DEVICE_VTBL.base.release)(this) };
    assert_eq!(rc_down, 1);

    // SAFETY: `this` is valid and `out` is a stack pointer; null `riid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.query_interface)(this, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `guid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.get_private_data)(this, n, ptr::null_mut(), nm) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `guid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.set_private_data)(this, n, 0, n) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `guid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.set_private_data_interface)(this, n, n) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `name` clears the debug name (`S_FALSE` when none was set).
    let hr = unsafe { (DEVICE_VTBL.base.set_name)(this, n) };
    assert_eq!(hr, crate::com::S_FALSE);

    // SAFETY: `this` is valid.
    assert_eq!(unsafe { (DEVICE_VTBL.base.get_node_count)(this) }, 1);

    // SAFETY: `this` is valid; null `desc`/`riid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_command_queue)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; out-of-range `list_type` is rejected.
    let hr =
        unsafe { (DEVICE_VTBL.base.create_command_allocator)(this, u32::MAX, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_graphics_pipeline_state)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_compute_pipeline_state)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `allocator` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_command_list)(this, 0, 0, nm, nm, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `data` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.check_feature_support)(this, 0, nm, 0) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid; null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_descriptor_heap)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `this` is valid.
    assert!(unsafe { (DEVICE_VTBL.base.get_descriptor_handle_increment_size)(this, 0) } > 0);

    // SAFETY: `this` is valid; null/empty `blob` returns `E_FAIL`.
    let hr = unsafe { (DEVICE_VTBL.base.create_root_signature)(this, 0, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_FAIL);

    // SAFETY: zero descriptor handle is a safe no-op.
    unsafe { (DEVICE_VTBL.base.create_constant_buffer_view)(this, n, ZERO_HANDLE) };
    // SAFETY: zero descriptor handle is a safe no-op.
    unsafe { (DEVICE_VTBL.base.create_shader_resource_view)(this, nm, n, ZERO_HANDLE) };
    // SAFETY: zero descriptor handle is a safe no-op.
    unsafe { (DEVICE_VTBL.base.create_unordered_access_view)(this, nm, nm, n, ZERO_HANDLE) };
    // SAFETY: zero descriptor handle is a safe no-op.
    unsafe { (DEVICE_VTBL.base.create_render_target_view)(this, nm, n, ZERO_HANDLE) };
    // SAFETY: zero descriptor handle is a safe no-op.
    unsafe { (DEVICE_VTBL.base.create_depth_stencil_view)(this, nm, n, ZERO_HANDLE) };
    // SAFETY: null `desc` is a safe no-op.
    unsafe { (DEVICE_VTBL.base.create_sampler)(this, n, ZERO_HANDLE) };
    // SAFETY: zero range count is a safe no-op.
    unsafe { (DEVICE_VTBL.base.copy_descriptors)(this, 0, n, ptr::null(), 0, n, ptr::null(), 0) };
    // SAFETY: zero descriptor count is a safe no-op.
    unsafe { (DEVICE_VTBL.base.copy_descriptors_simple)(this, 0, ZERO_HANDLE, ZERO_HANDLE, 0) };

    // SAFETY: null `ret` and `descs` returns `ret`.
    let ret = unsafe { (DEVICE_VTBL.base.get_resource_allocation_info)(this, nm, 0, 0, n) };
    assert!(ret.is_null());
    // SAFETY: null `ret` returns `ret`.
    assert!(unsafe { (DEVICE_VTBL.base.get_custom_heap_properties)(this, nm, 0, 0) }.is_null());

    // SAFETY: null `heap_props`/`desc` is rejected.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_committed_resource)(this, n, 0, n, 0, n, n, &raw mut out)
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_heap)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `heap`/`desc` is rejected.
    let hr =
        unsafe { (DEVICE_VTBL.base.create_placed_resource)(this, nm, 0, n, 0, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `CreateReservedResource` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.create_reserved_resource)(this, n, 0, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `object` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_shared_handle)(this, nm, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `handle` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.open_shared_handle)(this, nm, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `OpenSharedHandleByName` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.open_shared_handle_by_name)(this, n, 0, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: `MakeResident` and `Evict` are no-ops returning `S_OK`.
    let hr = unsafe { (DEVICE_VTBL.base.make_resident)(this, 0, ptr::null()) };
    assert_eq!(hr, S_OK);
    // SAFETY: `Evict` is a no-op returning `S_OK`.
    let hr = unsafe { (DEVICE_VTBL.base.evict)(this, 0, ptr::null()) };
    assert_eq!(hr, S_OK);

    // SAFETY: null `riid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_fence)(this, 0, 0, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `GetDeviceRemovedReason` returns `S_OK`.
    let hr = unsafe { (DEVICE_VTBL.base.get_device_removed_reason)(this) };
    assert_eq!(hr, S_OK);

    // SAFETY: null `desc` and out-pointers is a safe no-op.
    unsafe {
        (DEVICE_VTBL.base.get_copyable_footprints)(
            this,
            n,
            0,
            0,
            0,
            nm,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
    };

    // SAFETY: null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_query_heap)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `SetStablePowerState` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.set_stable_power_state)(this, 0) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_command_signature)(this, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `GetResourceTiling` is a `not_implemented!` stub.
    unsafe {
        (DEVICE_VTBL.base.get_resource_tiling)(
            this,
            nm,
            ptr::null_mut(),
            nm,
            nm,
            ptr::null_mut(),
            0,
            nm,
        );
    };
    assert_reported("ID3D12Device::GetResourceTiling");

    // SAFETY: null `ret` returns `ret`.
    let ret = unsafe { (DEVICE_VTBL.base.get_adapter_luid)(this, nm) };
    assert!(ret.is_null());

    // SAFETY: null `riid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_pipeline_library)(this, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: zero fence count is a no-op returning `S_OK`.
    let hr = unsafe {
        (DEVICE_VTBL.base.set_event_on_multiple_fence_completion)(
            this,
            ptr::null(),
            ptr::null(),
            0,
            0,
            nm,
        )
    };
    assert_eq!(hr, S_OK);

    // SAFETY: `SetResidencyPriority` is a no-op returning `S_OK`.
    let hr =
        unsafe { (DEVICE_VTBL.base.set_residency_priority)(this, 0, ptr::null(), ptr::null()) };
    assert_eq!(hr, S_OK);

    // SAFETY: null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_pipeline_state)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `OpenExistingHeapFromAddress` returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.base.open_existing_heap_from_address)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    // SAFETY: `OpenExistingHeapFromFileMapping` returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.base.open_existing_heap_from_file_mapping)(this, nm, n, &raw mut out)
    };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `fence` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.enqueue_make_resident)(this, 0, 0, ptr::null(), nm, 0) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `riid` is rejected with `E_NOINTERFACE`.
    let hr = unsafe { (DEVICE_VTBL.base.create_command_list1)(this, 0, 0, 0, n, &raw mut out) };
    assert_eq!(hr, crate::com::E_NOINTERFACE);

    // SAFETY: `CreateProtectedResourceSession` returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.base.create_protected_resource_session)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `heap_props`/`desc` is rejected.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_committed_resource1)(this, n, 0, n, 0, n, nm, n, &raw mut out)
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `desc` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_heap1)(this, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `CreateReservedResource1` returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.base.create_reserved_resource1)(this, n, 0, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `ret` returns `ret`.
    let ret = unsafe { (DEVICE_VTBL.base.get_resource_allocation_info1)(this, nm, 0, 0, n, nm) };
    assert!(ret.is_null());

    // SAFETY: `CreateLifetimeTracker` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.create_lifetime_tracker)(this, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: `RemoveDevice` is a safe no-op.
    unsafe { (DEVICE_VTBL.base.remove_device)(this) };

    // SAFETY: null `num_meta_commands` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.enumerate_meta_commands)(this, ptr::null_mut(), nm) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `EnumerateMetaCommandParameters` returns `E_INVALIDARG`.
    let hr = unsafe {
        (DEVICE_VTBL.base.enumerate_meta_command_parameters)(
            this,
            n,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            nm,
        )
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `CreateMetaCommand` returns `E_INVALIDARG`.
    let hr = unsafe { (DEVICE_VTBL.base.create_meta_command)(this, n, 0, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `CreateStateObject` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.create_state_object)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `info` is a safe no-op.
    unsafe {
        (DEVICE_VTBL
            .base
            .get_raytracing_acceleration_structure_prebuild_info)(this, n, nm);
    };

    // SAFETY: returns `D3D12_DRIVER_MATCHING_IDENTIFIER_UNSUPPORTED_TYPE` (`1`).
    let id_status = unsafe { (DEVICE_VTBL.base.check_driver_matching_identifier)(this, 0, n) };
    assert_eq!(id_status, 1);

    // SAFETY: null `further_desired` returns `S_OK`.
    let hr = unsafe {
        (DEVICE_VTBL.base.set_background_processing_mode)(this, 0, 0, nm, ptr::null_mut())
    };
    assert_eq!(hr, S_OK);

    // SAFETY: `AddToStateObject` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.add_to_state_object)(this, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    // SAFETY: `CreateProtectedResourceSession1` returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.base.create_protected_resource_session1)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `ret` returns `ret`.
    let ret = unsafe { (DEVICE_VTBL.base.get_resource_allocation_info2)(this, nm, 0, 0, n, nm) };
    assert!(ret.is_null());

    // SAFETY: null `desc` is rejected.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_committed_resource2)(this, n, 0, n, 0, n, nm, n, &raw mut out)
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `desc` is rejected.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_placed_resource1)(this, nm, 0, n, 0, n, n, &raw mut out)
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: logs a warning and returns.
    unsafe {
        (DEVICE_VTBL
            .base
            .create_sampler_feedback_unordered_access_view)(this, nm, nm, ZERO_HANDLE);
    };

    // SAFETY: null `desc` and out-pointers is a safe no-op.
    unsafe {
        (DEVICE_VTBL.base.get_copyable_footprints1)(
            this,
            n,
            0,
            0,
            0,
            nm,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
    };

    // SAFETY: `CreateShaderCacheSession` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.create_shader_cache_session)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    // SAFETY: `ShaderCacheControl` returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.base.shader_cache_control)(this, 0, 0) };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `desc`/`riid` is rejected.
    let hr = unsafe { (DEVICE_VTBL.base.create_command_queue1)(this, n, n, n, &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `heap_props` is rejected.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_committed_resource3)(
            this,
            n,
            0,
            n,
            0,
            n,
            nm,
            0,
            ptr::null_mut(),
            n,
            &raw mut out,
        )
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `heap` is rejected.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_placed_resource2)(
            this,
            nm,
            0,
            n,
            0,
            n,
            0,
            ptr::null_mut(),
            n,
            &raw mut out,
        )
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `CreateReservedResource2` returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_reserved_resource2)(
            this,
            n,
            0,
            n,
            nm,
            0,
            ptr::null_mut(),
            n,
            &raw mut out,
        )
    };
    assert_eq!(hr, E_NOTIMPL);

    // SAFETY: null `desc` is a safe no-op for `CreateSampler2`.
    unsafe { (DEVICE_VTBL.create_sampler2)(this, ptr::null(), ZERO_HANDLE) };
    let heap_desc = DescriptorHeapDesc {
        r#type: D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER,
        num_descriptors: 2,
        flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        node_mask: 0,
    };
    let mut sampler_heap: *mut c_void = ptr::null_mut();
    // SAFETY: `this` is a valid `Device`, `heap_desc` and `sampler_heap` are valid pointers.
    let hr = unsafe {
        (DEVICE_VTBL.base.create_descriptor_heap)(
            this,
            (&raw const heap_desc).cast(),
            ptr::from_ref(&IID_ID3D12_DESCRIPTOR_HEAP).cast(),
            &raw mut sampler_heap,
        )
    };
    assert_eq!(hr, S_OK);
    let cpu_handle = EmbeddedDescriptorHandle::pack(sampler_heap.cast::<Heap>().cast_const(), 0);
    let sampler2_desc = SamplerDesc2 {
        filter: 0,
        address_u: 1,
        address_v: 1,
        address_w: 1,
        mip_lod_bias: 0.25,
        max_anisotropy: 1,
        comparison_func: 1,
        border_color: SamplerBorderColor {
            uint_border_color: [1, 2, 3, 4],
        },
        min_lod: 0.0,
        max_lod: 10.0,
        flags: D3D12_SAMPLER_FLAG_UINT_BORDER_COLOR,
    };
    // SAFETY: `this`, `sampler2_desc`, and `cpu_handle` are valid.
    unsafe { (DEVICE_VTBL.create_sampler2)(this, &raw const sampler2_desc, cpu_handle) };
    // SAFETY: balancing `create_descriptor_heap` reference.
    unsafe { release_com(sampler_heap.cast::<Heap>()) };

    // SAFETY: null `ret` returns `ret`.
    let ret = unsafe {
        (DEVICE_VTBL.get_resource_allocation_info3)(
            this,
            nm,
            0,
            0,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
        )
    };
    assert!(ret.is_null());

    let mut alloc_info = ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 0,
    };
    // SAFETY: `alloc_info` is a valid sret out-pointer; 0 descs writes sentinel size.
    let ret = unsafe {
        (DEVICE_VTBL.get_resource_allocation_info3)(
            this,
            (&raw mut alloc_info).cast(),
            0,
            0,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
        )
    };
    assert_eq!(ret, (&raw mut alloc_info).cast());
    assert_eq!(alloc_info.size_in_bytes, u64::MAX);

    out = this;
    // SAFETY: `OpenExistingHeapFromAddress1` zeroes `out` and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.open_existing_heap_from_address1)(this, n, 0, ptr::null(), &raw mut out)
    };
    assert_eq!(hr, E_NOTIMPL);
    assert!(out.is_null());

    out = this;
    // SAFETY: `CreateRootSignatureFromSubobjectInLibrary` zeroes `out` and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.create_root_signature_from_subobject_in_library)(
            this,
            0,
            n,
            0,
            ptr::null(),
            ptr::null(),
            &raw mut out,
        )
    };
    assert_eq!(hr, E_NOTIMPL);
    assert!(out.is_null());

    // Companion vtable (`IMTLD3D12DeviceExt1`) seated at `Device::EXT_OFFSET`.
    // SAFETY: `this` points to a live `Device`; `EXT_OFFSET` is within `Device`.
    let ext_this = unsafe { this.byte_add(Device::EXT_OFFSET) };

    // SAFETY: `ext_this` is a valid companion pointer on `Device`.
    let ext_up = unsafe { (DEVICE_EXT_VTBL.base.unknown.add_ref)(ext_this) };
    assert_eq!(ext_up, 2);
    // SAFETY: `ext_this` has refcount 2.
    let ext_down = unsafe { (DEVICE_EXT_VTBL.base.unknown.release)(ext_this) };
    assert_eq!(ext_down, 1);

    // SAFETY: null `riid` is rejected.
    let hr = unsafe {
        (DEVICE_EXT_VTBL.base.unknown.query_interface)(ext_this, ptr::null(), &raw mut out)
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null out-pointers are rejected.
    let hr = unsafe {
        (DEVICE_EXT_VTBL.base.get_temporal_scaler_scale_range)(
            ext_this,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: null `desc` is rejected.
    let hr = unsafe {
        (DEVICE_EXT_VTBL.base.create_temporal_scaler)(ext_this, ptr::null(), &raw mut out)
    };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: `ext_this` is a valid companion pointer (`mtl` is `None` in dummy device -> 0).
    let supported = unsafe { (DEVICE_EXT_VTBL.supports_temporal_denoised_scaler)(ext_this) };
    assert_eq!(supported, 0);

    // SAFETY: null `desc` is rejected.
    let hr = unsafe {
        (DEVICE_EXT_VTBL.create_temporal_denoised_scaler)(ext_this, ptr::null(), &raw mut out)
    };
    assert_eq!(hr, E_INVALIDARG);

    // Final release drops the boxed `Device`.
    // SAFETY: `this` has refcount 1 and was allocated by `Box::into_raw`.
    let final_rc = unsafe { (DEVICE_VTBL.base.release)(this) };
    assert_eq!(final_rc, 0);
}
