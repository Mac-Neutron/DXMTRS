use core::{ffi::c_void, ptr};

use crate::{
    abi::vtbl::D3d12CpuDescriptorHandle,
    com::E_NOTIMPL,
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
    let rc_up = unsafe { (DEVICE_VTBL.add_ref)(this) };
    assert_eq!(rc_up, 2);
    // SAFETY: `this` has refcount 2.
    let rc_down = unsafe { (DEVICE_VTBL.release)(this) };
    assert_eq!(rc_down, 1);

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.query_interface)(this, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::QueryInterface");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.get_private_data)(this, n, ptr::null_mut(), nm) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::GetPrivateData");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.set_private_data)(this, n, 0, n) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::SetPrivateData");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.set_private_data_interface)(this, n, n) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::SetPrivateDataInterface");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.set_name)(this, n) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::SetName");

    // SAFETY: stub only logs and returns `0`.
    let _ = unsafe { (DEVICE_VTBL.get_node_count)(this) };
    assert_reported("ID3D12Device::GetNodeCount");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_command_queue)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommandQueue");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_command_allocator)(this, 0, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommandAllocator");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_graphics_pipeline_state)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateGraphicsPipelineState");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_compute_pipeline_state)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateComputePipelineState");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_command_list)(this, 0, 0, nm, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommandList");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.check_feature_support)(this, 0, nm, 0) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CheckFeatureSupport");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_descriptor_heap)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateDescriptorHeap");

    // SAFETY: stub only logs and returns `0`.
    let _ = unsafe { (DEVICE_VTBL.get_descriptor_handle_increment_size)(this, 0) };
    assert_reported("ID3D12Device::GetDescriptorHandleIncrementSize");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_root_signature)(this, 0, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateRootSignature");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.create_constant_buffer_view)(this, n, ZERO_HANDLE) };
    assert_reported("ID3D12Device::CreateConstantBufferView");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.create_shader_resource_view)(this, nm, n, ZERO_HANDLE) };
    assert_reported("ID3D12Device::CreateShaderResourceView");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.create_unordered_access_view)(this, nm, nm, n, ZERO_HANDLE) };
    assert_reported("ID3D12Device::CreateUnorderedAccessView");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.create_render_target_view)(this, nm, n, ZERO_HANDLE) };
    assert_reported("ID3D12Device::CreateRenderTargetView");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.create_depth_stencil_view)(this, nm, n, ZERO_HANDLE) };
    assert_reported("ID3D12Device::CreateDepthStencilView");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.create_sampler)(this, n, ZERO_HANDLE) };
    assert_reported("ID3D12Device::CreateSampler");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.copy_descriptors)(this, 0, n, ptr::null(), 0, n, ptr::null(), 0) };
    assert_reported("ID3D12Device::CopyDescriptors");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.copy_descriptors_simple)(this, 0, ZERO_HANDLE, ZERO_HANDLE, 0) };
    assert_reported("ID3D12Device::CopyDescriptorsSimple");

    // SAFETY: stub only logs and returns `ret`.
    let _ = unsafe { (DEVICE_VTBL.get_resource_allocation_info)(this, nm, 0, 0, n) };
    assert_reported("ID3D12Device::GetResourceAllocationInfo");

    // SAFETY: stub only logs and returns `ret`.
    let _ = unsafe { (DEVICE_VTBL.get_custom_heap_properties)(this, nm, 0, 0) };
    assert_reported("ID3D12Device::GetCustomHeapProperties");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.create_committed_resource)(this, n, 0, n, 0, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommittedResource");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_heap)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateHeap");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_placed_resource)(this, nm, 0, n, 0, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreatePlacedResource");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_reserved_resource)(this, n, 0, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateReservedResource");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_shared_handle)(this, nm, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateSharedHandle");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.open_shared_handle)(this, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::OpenSharedHandle");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.open_shared_handle_by_name)(this, n, 0, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::OpenSharedHandleByName");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.make_resident)(this, 0, ptr::null()) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::MakeResident");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.evict)(this, 0, ptr::null()) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::Evict");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_fence)(this, 0, 0, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateFence");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.get_device_removed_reason)(this) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::GetDeviceRemovedReason");

    // SAFETY: stub only logs.
    unsafe {
        (DEVICE_VTBL.get_copyable_footprints)(
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
    assert_reported("ID3D12Device::GetCopyableFootprints");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_query_heap)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateQueryHeap");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.set_stable_power_state)(this, 0) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::SetStablePowerState");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_command_signature)(this, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommandSignature");

    // SAFETY: stub only logs.
    unsafe {
        (DEVICE_VTBL.get_resource_tiling)(
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

    // SAFETY: stub only logs and returns `ret`.
    let _ = unsafe { (DEVICE_VTBL.get_adapter_luid)(this, nm) };
    assert_reported("ID3D12Device::GetAdapterLuid");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_pipeline_library)(this, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreatePipelineLibrary");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.set_event_on_multiple_fence_completion)(
            this,
            ptr::null(),
            ptr::null(),
            0,
            0,
            nm,
        )
    };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::SetEventOnMultipleFenceCompletion");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.set_residency_priority)(this, 0, ptr::null(), ptr::null()) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::SetResidencyPriority");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_pipeline_state)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreatePipelineState");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.open_existing_heap_from_address)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::OpenExistingHeapFromAddress");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.open_existing_heap_from_file_mapping)(this, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::OpenExistingHeapFromFileMapping");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.enqueue_make_resident)(this, 0, 0, ptr::null(), nm, 0) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::EnqueueMakeResident");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_command_list1)(this, 0, 0, 0, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommandList1");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_protected_resource_session)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateProtectedResourceSession");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.create_committed_resource1)(this, n, 0, n, 0, n, nm, n, &raw mut out)
    };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommittedResource1");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_heap1)(this, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateHeap1");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_reserved_resource1)(this, n, 0, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateReservedResource1");

    // SAFETY: stub only logs and returns `ret`.
    let _ = unsafe { (DEVICE_VTBL.get_resource_allocation_info1)(this, nm, 0, 0, n, nm) };
    assert_reported("ID3D12Device::GetResourceAllocationInfo1");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_lifetime_tracker)(this, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateLifetimeTracker");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.remove_device)(this) };
    assert_reported("ID3D12Device::RemoveDevice");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.enumerate_meta_commands)(this, ptr::null_mut(), nm) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::EnumerateMetaCommands");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.enumerate_meta_command_parameters)(
            this,
            n,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            nm,
        )
    };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::EnumerateMetaCommandParameters");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_meta_command)(this, n, 0, n, 0, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateMetaCommand");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_state_object)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateStateObject");

    // SAFETY: stub only logs.
    unsafe { (DEVICE_VTBL.get_raytracing_acceleration_structure_prebuild_info)(this, n, nm) };
    assert_reported("ID3D12Device::GetRaytracingAccelerationStructurePrebuildInfo");

    // SAFETY: stub only logs and returns `0`.
    let _ = unsafe { (DEVICE_VTBL.check_driver_matching_identifier)(this, 0, n) };
    assert_reported("ID3D12Device::CheckDriverMatchingIdentifier");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.set_background_processing_mode)(this, 0, 0, nm, ptr::null_mut()) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::SetBackgroundProcessingMode");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.add_to_state_object)(this, n, nm, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::AddToStateObject");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_protected_resource_session1)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateProtectedResourceSession1");

    // SAFETY: stub only logs and returns `ret`.
    let _ = unsafe { (DEVICE_VTBL.get_resource_allocation_info2)(this, nm, 0, 0, n, nm) };
    assert_reported("ID3D12Device::GetResourceAllocationInfo2");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.create_committed_resource2)(this, n, 0, n, 0, n, nm, n, &raw mut out)
    };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommittedResource2");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_VTBL.create_placed_resource1)(this, nm, 0, n, 0, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreatePlacedResource1");

    // SAFETY: stub only logs.
    unsafe {
        (DEVICE_VTBL.create_sampler_feedback_unordered_access_view)(this, nm, nm, ZERO_HANDLE);
    };
    assert_reported("ID3D12Device::CreateSamplerFeedbackUnorderedAccessView");

    // SAFETY: stub only logs.
    unsafe {
        (DEVICE_VTBL.get_copyable_footprints1)(
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
    assert_reported("ID3D12Device::GetCopyableFootprints1");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_shader_cache_session)(this, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateShaderCacheSession");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.shader_cache_control)(this, 0, 0) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::ShaderCacheControl");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe { (DEVICE_VTBL.create_command_queue1)(this, n, n, n, &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommandQueue1");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.create_committed_resource3)(
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
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreateCommittedResource3");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.create_placed_resource2)(
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
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::CreatePlacedResource2");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_VTBL.create_reserved_resource2)(
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
    assert_reported("ID3D12Device::CreateReservedResource2");

    // Companion vtable (`IMTLD3D12DeviceExt`) seated at `Device::EXT_OFFSET`.
    // SAFETY: `this` points to a live `Device`; `EXT_OFFSET` is within `Device`.
    let ext_this = unsafe { this.byte_add(Device::EXT_OFFSET) };

    // SAFETY: `ext_this` is a valid companion pointer on `Device`.
    let ext_up = unsafe { (DEVICE_EXT_VTBL.unknown.add_ref)(ext_this) };
    assert_eq!(ext_up, 2);
    // SAFETY: `ext_this` has refcount 2.
    let ext_down = unsafe { (DEVICE_EXT_VTBL.unknown.release)(ext_this) };
    assert_eq!(ext_down, 1);

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_EXT_VTBL.unknown.query_interface)(ext_this, ptr::null(), &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("ID3D12Device::QueryInterface");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr = unsafe {
        (DEVICE_EXT_VTBL.get_temporal_scaler_scale_range)(
            ext_this,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("IMTLD3D12DeviceExt::GetTemporalScalerScaleRange");

    // SAFETY: stub only logs and returns `E_NOTIMPL`.
    let hr =
        unsafe { (DEVICE_EXT_VTBL.create_temporal_scaler)(ext_this, ptr::null(), &raw mut out) };
    assert_eq!(hr, E_NOTIMPL);
    assert_reported("IMTLD3D12DeviceExt::CreateTemporalScaler");

    // Final release drops the boxed `Device`.
    // SAFETY: `this` has refcount 1 and was allocated by `Box::into_raw`.
    let final_rc = unsafe { (DEVICE_VTBL.release)(this) };
    assert_eq!(final_rc, 0);
}
