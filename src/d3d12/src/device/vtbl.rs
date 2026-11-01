//! `ID3D12Device10` and `IMTLD3D12DeviceExt` vtable dispatch into the lane modules.

use core::ffi::c_void;

use crate::{
    abi::{
        ext::{D3D12DeviceExtVtbl, D3D12ScalerDesc},
        vtbl::{D3d12CpuDescriptorHandle, ID3D12Device10Vtbl, IUnknownVtbl},
    },
    allocator,
    com::{self, Hresult},
    command_signature, descriptor_heap,
    device::{Device, factory, features, residency, temporal},
    fence, heap, list, pipeline, query_heap, queue, resource, root_signature, shared_fence, slot,
};

/// Primary `ID3D12Device10` vtable for [`Device`], dispatching each slot into its owning lane.
pub static DEVICE_VTBL: ID3D12Device10Vtbl = ID3D12Device10Vtbl {
    query_interface: slot!(
        (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::query_interface(this, riid, ppv_object) }
        }
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Device` COM pointer.
        unsafe { com::add_ref_com(this.cast::<Device>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Device` COM pointer.
        unsafe { com::release_com(this.cast::<Device>()) }
    }),
    get_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::get_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::set_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data_interface: slot!(
        (this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::set_private_data_interface(this, guid, data) }
        }
    ),
    set_name: slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { factory::set_name(this, name) }
    }),
    get_node_count: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { factory::get_node_count(this) }
    }),
    create_command_queue: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, queue: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { queue::create_command_queue(this, desc, riid, queue) }
        }
    ),
    create_command_allocator: slot!(
        (this: *mut c_void, list_type: u32, riid: *const c_void, allocator: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { allocator::create_command_allocator(this, list_type, riid, allocator) }
        }
    ),
    create_graphics_pipeline_state: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, pso: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { pipeline::graphics::create_graphics_pipeline_state(this, desc, riid, pso) }
        }
    ),
    create_compute_pipeline_state: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, pso: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { pipeline::compute::create_compute_pipeline_state(this, desc, riid, pso) }
        }
    ),
    create_command_list: slot!(
        (
            this: *mut c_void,
            node_mask: u32,
            list_type: u32,
            allocator: *mut c_void,
            initial_state: *mut c_void,
            riid: *const c_void,
            list: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                list::state::create_command_list(
                    this,
                    node_mask,
                    list_type,
                    allocator,
                    initial_state,
                    riid,
                    list,
                )
            }
        }
    ),
    check_feature_support: slot!(
        (this: *mut c_void, feature: u32, data: *mut c_void, size: u32) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { features::check_feature_support(this, feature, data, size) }
        }
    ),
    create_descriptor_heap: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, heap: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { descriptor_heap::create_descriptor_heap(this, desc, riid, heap) }
        }
    ),
    get_descriptor_handle_increment_size: slot!(
        (this: *mut c_void, heap_type: u32) -> u32 => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { descriptor_heap::get_descriptor_handle_increment_size(this, heap_type) }
        }
    ),
    create_root_signature: slot!(
        (
            this: *mut c_void,
            node_mask: u32,
            blob: *const c_void,
            length: usize,
            riid: *const c_void,
            root_sig: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                root_signature::object::create_root_signature(
                    this,
                    node_mask,
                    blob,
                    length,
                    riid,
                    root_sig,
                )
            }
        }
    ),
    create_constant_buffer_view: slot!(
        (this: *mut c_void, desc: *const c_void, dest: D3d12CpuDescriptorHandle) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resource::create_constant_buffer_view(this, desc, dest) }
        }
    ),
    create_shader_resource_view: slot!(
        (this: *mut c_void, resource: *mut c_void, desc: *const c_void, dest: D3d12CpuDescriptorHandle) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resource::create_shader_resource_view(this, resource, desc, dest) }
        }
    ),
    create_unordered_access_view: slot!(
        (
            this: *mut c_void,
            resource: *mut c_void,
            counter: *mut c_void,
            desc: *const c_void,
            dest: D3d12CpuDescriptorHandle
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resource::create_unordered_access_view(this, resource, counter, desc, dest) }
        }
    ),
    create_render_target_view: slot!(
        (this: *mut c_void, resource: *mut c_void, desc: *const c_void, dest: D3d12CpuDescriptorHandle) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resource::create_render_target_view(this, resource, desc, dest) }
        }
    ),
    create_depth_stencil_view: slot!(
        (this: *mut c_void, resource: *mut c_void, desc: *const c_void, dest: D3d12CpuDescriptorHandle) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resource::create_depth_stencil_view(this, resource, desc, dest) }
        }
    ),
    create_sampler: slot!(
        (this: *mut c_void, desc: *const c_void, dest: D3d12CpuDescriptorHandle) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { descriptor_heap::create_sampler(this, desc, dest) }
        }
    ),
    copy_descriptors: slot!(
        (
            this: *mut c_void,
            num_dest: u32,
            dest_starts: *const c_void,
            dest_sizes: *const u32,
            num_src: u32,
            src_starts: *const c_void,
            src_sizes: *const u32,
            heap_type: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                descriptor_heap::copy_descriptors(
                    this,
                    num_dest,
                    dest_starts,
                    dest_sizes,
                    num_src,
                    src_starts,
                    src_sizes,
                    heap_type,
                );
            }
        }
    ),
    copy_descriptors_simple: slot!(
        (
            this: *mut c_void,
            num_descriptors: u32,
            dest_start: D3d12CpuDescriptorHandle,
            src_start: D3d12CpuDescriptorHandle,
            heap_type: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                descriptor_heap::copy_descriptors_simple(
                    this,
                    num_descriptors,
                    dest_start,
                    src_start,
                    heap_type,
                );
            }
        }
    ),
    get_resource_allocation_info: slot!(
        (
            this: *mut c_void,
            ret: *mut c_void,
            visible_mask: u32,
            num_descs: u32,
            descs: *const c_void
        ) -> *mut c_void => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { features::get_resource_allocation_info(this, ret, visible_mask, num_descs, descs) }
        }
    ),
    get_custom_heap_properties: slot!(
        (this: *mut c_void, ret: *mut c_void, node_mask: u32, heap_type: u32) -> *mut c_void => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { features::get_custom_heap_properties(this, ret, node_mask, heap_type) }
        }
    ),
    create_committed_resource: slot!(
        (
            this: *mut c_void,
            heap_props: *const c_void,
            heap_flags: u32,
            desc: *const c_void,
            initial_state: u32,
            optimized_clear: *const c_void,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_committed_resource(
                    this,
                    heap_props,
                    heap_flags,
                    desc,
                    initial_state,
                    optimized_clear,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_heap: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, heap: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { heap::create_heap(this, desc, riid, heap) }
        }
    ),
    create_placed_resource: slot!(
        (
            this: *mut c_void,
            heap: *mut c_void,
            heap_offset: u64,
            desc: *const c_void,
            initial_state: u32,
            optimized_clear: *const c_void,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_placed_resource(
                    this,
                    heap,
                    heap_offset,
                    desc,
                    initial_state,
                    optimized_clear,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_reserved_resource: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            initial_state: u32,
            optimized_clear: *const c_void,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_reserved_resource(
                    this,
                    desc,
                    initial_state,
                    optimized_clear,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_shared_handle: slot!(
        (
            this: *mut c_void,
            object: *mut c_void,
            attributes: *const c_void,
            access: u32,
            name: *const c_void,
            handle: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { shared_fence::create_shared_handle(this, object, attributes, access, name, handle) }
        }
    ),
    open_shared_handle: slot!(
        (this: *mut c_void, nt_handle: *mut c_void, riid: *const c_void, obj: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { shared_fence::open_shared_handle(this, nt_handle, riid, obj) }
        }
    ),
    open_shared_handle_by_name: slot!(
        (this: *mut c_void, name: *const c_void, access: u32, nt_handle: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { shared_fence::open_shared_handle_by_name(this, name, access, nt_handle) }
        }
    ),
    make_resident: slot!(
        (this: *mut c_void, num_objects: u32, objects: *const *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { residency::make_resident(this, num_objects, objects) }
        }
    ),
    evict: slot!(
        (this: *mut c_void, num_objects: u32, objects: *const *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { residency::evict(this, num_objects, objects) }
        }
    ),
    create_fence: slot!(
        (
            this: *mut c_void,
            initial_value: u64,
            flags: u32,
            riid: *const c_void,
            fence: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { fence::create_fence(this, initial_value, flags, riid, fence) }
        }
    ),
    get_device_removed_reason: slot!((this: *mut c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { factory::get_device_removed_reason(this) }
    }),
    get_copyable_footprints: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            first_sub: u32,
            num_sub: u32,
            base_offset: u64,
            layouts: *mut c_void,
            num_rows: *mut u32,
            row_sizes: *mut u64,
            total_bytes: *mut u64
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                features::get_copyable_footprints(
                    this,
                    desc,
                    first_sub,
                    num_sub,
                    base_offset,
                    layouts,
                    num_rows,
                    row_sizes,
                    total_bytes,
                );
            }
        }
    ),
    create_query_heap: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, heap: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { query_heap::create_query_heap(this, desc, riid, heap) }
        }
    ),
    set_stable_power_state: slot!((this: *mut c_void, enable: i32) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { factory::set_stable_power_state(this, enable) }
    }),
    create_command_signature: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            root_sig: *mut c_void,
            riid: *const c_void,
            cmd_sig: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { command_signature::create_command_signature(this, desc, root_sig, riid, cmd_sig) }
        }
    ),
    get_resource_tiling: slot!(
        (
            this: *mut c_void,
            tiled_resource: *mut c_void,
            num_tiles: *mut u32,
            packed_mip_desc: *mut c_void,
            standard_tile_shape: *mut c_void,
            num_sub_tilings: *mut u32,
            first_sub_tiling: u32,
            sub_tilings: *mut c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                features::get_resource_tiling(
                    this,
                    tiled_resource,
                    num_tiles,
                    packed_mip_desc,
                    standard_tile_shape,
                    num_sub_tilings,
                    first_sub_tiling,
                    sub_tilings,
                );
            }
        }
    ),
    get_adapter_luid: slot!((this: *mut c_void, ret: *mut c_void) -> *mut c_void => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { factory::get_adapter_luid(this, ret) }
    }),
    create_pipeline_library: slot!(
        (
            this: *mut c_void,
            blob: *const c_void,
            blob_len: usize,
            riid: *const c_void,
            library: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { pipeline::library::create_pipeline_library(this, blob, blob_len, riid, library) }
        }
    ),
    set_event_on_multiple_fence_completion: slot!(
        (
            this: *mut c_void,
            fences: *const *mut c_void,
            values: *const u64,
            num_fences: u32,
            flags: u32,
            event: *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                fence::set_event_on_multiple_fence_completion(
                    this,
                    fences,
                    values,
                    num_fences,
                    flags,
                    event,
                )
            }
        }
    ),
    set_residency_priority: slot!(
        (
            this: *mut c_void,
            num_objects: u32,
            objects: *const *mut c_void,
            priorities: *const u32
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { residency::set_residency_priority(this, num_objects, objects, priorities) }
        }
    ),
    create_pipeline_state: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, pso: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { pipeline::create_pipeline_state(this, desc, riid, pso) }
        }
    ),
    open_existing_heap_from_address: slot!(
        (this: *mut c_void, address: *const c_void, riid: *const c_void, heap: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { heap::open_existing_heap_from_address(this, address, riid, heap) }
        }
    ),
    open_existing_heap_from_file_mapping: slot!(
        (this: *mut c_void, file_mapping: *mut c_void, riid: *const c_void, heap: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { heap::open_existing_heap_from_file_mapping(this, file_mapping, riid, heap) }
        }
    ),
    enqueue_make_resident: slot!(
        (
            this: *mut c_void,
            flags: u32,
            num_objects: u32,
            objects: *const *mut c_void,
            fence: *mut c_void,
            fence_value: u64
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                residency::enqueue_make_resident(
                    this,
                    flags,
                    num_objects,
                    objects,
                    fence,
                    fence_value,
                )
            }
        }
    ),
    create_command_list1: slot!(
        (
            this: *mut c_void,
            node_mask: u32,
            list_type: u32,
            flags: u32,
            riid: *const c_void,
            list: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { list::state::create_command_list1(this, node_mask, list_type, flags, riid, list) }
        }
    ),
    create_protected_resource_session: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, session: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::create_protected_resource_session(this, desc, riid, session) }
        }
    ),
    create_committed_resource1: slot!(
        (
            this: *mut c_void,
            heap_props: *const c_void,
            heap_flags: u32,
            desc: *const c_void,
            initial_state: u32,
            optimized_clear: *const c_void,
            protected_session: *mut c_void,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_committed_resource1(
                    this,
                    heap_props,
                    heap_flags,
                    desc,
                    initial_state,
                    optimized_clear,
                    protected_session,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_heap1: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            protected_session: *mut c_void,
            riid: *const c_void,
            heap: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { heap::create_heap1(this, desc, protected_session, riid, heap) }
        }
    ),
    create_reserved_resource1: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            initial_state: u32,
            optimized_clear: *const c_void,
            protected_session: *mut c_void,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_reserved_resource1(
                    this,
                    desc,
                    initial_state,
                    optimized_clear,
                    protected_session,
                    riid,
                    resource,
                )
            }
        }
    ),
    get_resource_allocation_info1: slot!(
        (
            this: *mut c_void,
            ret: *mut c_void,
            visible_mask: u32,
            num_descs: u32,
            descs: *const c_void,
            info1: *mut c_void
        ) -> *mut c_void => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                features::get_resource_allocation_info1(this, ret, visible_mask, num_descs, descs, info1)
            }
        }
    ),
    create_lifetime_tracker: slot!(
        (this: *mut c_void, owner: *mut c_void, riid: *const c_void, tracker: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::create_lifetime_tracker(this, owner, riid, tracker) }
        }
    ),
    remove_device: slot!((this: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { factory::remove_device(this) }
    }),
    enumerate_meta_commands: slot!(
        (this: *mut c_void, num_meta_commands: *mut u32, descs: *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::enumerate_meta_commands(this, num_meta_commands, descs) }
        }
    ),
    enumerate_meta_command_parameters: slot!(
        (
            this: *mut c_void,
            command_id: *const c_void,
            stage: u32,
            total_size: *mut u32,
            param_count: *mut u32,
            param_descs: *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                factory::enumerate_meta_command_parameters(
                    this,
                    command_id,
                    stage,
                    total_size,
                    param_count,
                    param_descs,
                )
            }
        }
    ),
    create_meta_command: slot!(
        (
            this: *mut c_void,
            command_id: *const c_void,
            node_mask: u32,
            params: *const c_void,
            params_size: usize,
            riid: *const c_void,
            meta_command: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                factory::create_meta_command(
                    this,
                    command_id,
                    node_mask,
                    params,
                    params_size,
                    riid,
                    meta_command,
                )
            }
        }
    ),
    create_state_object: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, state_object: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::create_state_object(this, desc, riid, state_object) }
        }
    ),
    get_raytracing_acceleration_structure_prebuild_info: slot!(
        (this: *mut c_void, desc: *const c_void, info: *mut c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::get_raytracing_acceleration_structure_prebuild_info(this, desc, info) }
        }
    ),
    check_driver_matching_identifier: slot!(
        (this: *mut c_void, data_type: u32, identifier: *const c_void) -> u32 => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::check_driver_matching_identifier(this, data_type, identifier) }
        }
    ),
    set_background_processing_mode: slot!(
        (
            this: *mut c_void,
            mode: u32,
            action: u32,
            event: *mut c_void,
            further_desired: *mut i32
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                factory::set_background_processing_mode(this, mode, action, event, further_desired)
            }
        }
    ),
    add_to_state_object: slot!(
        (
            this: *mut c_void,
            addition: *const c_void,
            grow_from: *mut c_void,
            riid: *const c_void,
            new_state_object: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::add_to_state_object(this, addition, grow_from, riid, new_state_object) }
        }
    ),
    create_protected_resource_session1: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, session: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::create_protected_resource_session1(this, desc, riid, session) }
        }
    ),
    get_resource_allocation_info2: slot!(
        (
            this: *mut c_void,
            ret: *mut c_void,
            visible_mask: u32,
            num_descs: u32,
            descs: *const c_void,
            info1: *mut c_void
        ) -> *mut c_void => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                features::get_resource_allocation_info2(this, ret, visible_mask, num_descs, descs, info1)
            }
        }
    ),
    create_committed_resource2: slot!(
        (
            this: *mut c_void,
            heap_props: *const c_void,
            heap_flags: u32,
            desc: *const c_void,
            initial_state: u32,
            optimized_clear: *const c_void,
            protected_session: *mut c_void,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_committed_resource2(
                    this,
                    heap_props,
                    heap_flags,
                    desc,
                    initial_state,
                    optimized_clear,
                    protected_session,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_placed_resource1: slot!(
        (
            this: *mut c_void,
            heap: *mut c_void,
            heap_offset: u64,
            desc: *const c_void,
            initial_state: u32,
            optimized_clear: *const c_void,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_placed_resource1(
                    this,
                    heap,
                    heap_offset,
                    desc,
                    initial_state,
                    optimized_clear,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_sampler_feedback_unordered_access_view: slot!(
        (
            this: *mut c_void,
            targeted_resource: *mut c_void,
            feedback_resource: *mut c_void,
            dest: D3d12CpuDescriptorHandle
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_sampler_feedback_unordered_access_view(
                    this,
                    targeted_resource,
                    feedback_resource,
                    dest,
                );
            }
        }
    ),
    get_copyable_footprints1: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            first_sub: u32,
            num_sub: u32,
            base_offset: u64,
            layouts: *mut c_void,
            num_rows: *mut u32,
            row_sizes: *mut u64,
            total_bytes: *mut u64
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                features::get_copyable_footprints1(
                    this,
                    desc,
                    first_sub,
                    num_sub,
                    base_offset,
                    layouts,
                    num_rows,
                    row_sizes,
                    total_bytes,
                );
            }
        }
    ),
    create_shader_cache_session: slot!(
        (this: *mut c_void, desc: *const c_void, riid: *const c_void, session: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::create_shader_cache_session(this, desc, riid, session) }
        }
    ),
    shader_cache_control: slot!(
        (this: *mut c_void, kinds: u32, flags: u32) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { factory::shader_cache_control(this, kinds, flags) }
        }
    ),
    create_command_queue1: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            creator_id: *const c_void,
            riid: *const c_void,
            queue: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { queue::create_command_queue1(this, desc, creator_id, riid, queue) }
        }
    ),
    create_committed_resource3: slot!(
        (
            this: *mut c_void,
            heap_props: *const c_void,
            heap_flags: u32,
            desc: *const c_void,
            initial_layout: u32,
            optimized_clear: *const c_void,
            protected_session: *mut c_void,
            num_castable: u32,
            castable_formats: *mut u32,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_committed_resource3(
                    this,
                    heap_props,
                    heap_flags,
                    desc,
                    initial_layout,
                    optimized_clear,
                    protected_session,
                    num_castable,
                    castable_formats,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_placed_resource2: slot!(
        (
            this: *mut c_void,
            heap: *mut c_void,
            heap_offset: u64,
            desc: *const c_void,
            initial_layout: u32,
            optimized_clear: *const c_void,
            num_castable: u32,
            castable_formats: *mut u32,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_placed_resource2(
                    this,
                    heap,
                    heap_offset,
                    desc,
                    initial_layout,
                    optimized_clear,
                    num_castable,
                    castable_formats,
                    riid,
                    resource,
                )
            }
        }
    ),
    create_reserved_resource2: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            initial_layout: u32,
            optimized_clear: *const c_void,
            protected_session: *mut c_void,
            num_castable: u32,
            castable_formats: *mut u32,
            riid: *const c_void,
            resource: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                resource::create_reserved_resource2(
                    this,
                    desc,
                    initial_layout,
                    optimized_clear,
                    protected_session,
                    num_castable,
                    castable_formats,
                    riid,
                    resource,
                )
            }
        }
    ),
};

/// Companion `IMTLD3D12DeviceExt` vtable for [`Device`], seated at [`Device::EXT_OFFSET`].
pub static DEVICE_EXT_VTBL: D3D12DeviceExtVtbl = D3D12DeviceExtVtbl {
    unknown: IUnknownVtbl {
        query_interface: slot!(
            (this: *mut c_void, riid: *const c_void, object: *mut *mut c_void) -> Hresult => {
                // SAFETY: caller forwards COM slot arguments.
                unsafe { factory::ext_query_interface(this, riid, object) }
            }
        ),
        add_ref: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `Device` companion pointer at `Device::EXT_OFFSET`.
            unsafe { factory::ext_add_ref(this) }
        }),
        release: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `Device` companion pointer at `Device::EXT_OFFSET`.
            unsafe { factory::ext_release(this) }
        }),
    },
    get_temporal_scaler_scale_range: slot!(
        (this: *mut c_void, min_scale: *mut f32, max_scale: *mut f32) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { temporal::get_temporal_scaler_scale_range(this, min_scale, max_scale) }
        }
    ),
    create_temporal_scaler: slot!(
        (
            this: *mut c_void,
            desc: *const D3D12ScalerDesc,
            scaler: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { temporal::create_temporal_scaler(this, desc, scaler) }
        }
    ),
};

#[cfg(test)]
mod tests;
