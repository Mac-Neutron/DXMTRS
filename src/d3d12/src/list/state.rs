//! Command-list creation, lifecycle (`Close`/`Reset`), and state setters
//!
//! (`d3d12_command_list.cpp`).

use core::ffi::c_void;

use crate::{
    abi::vtbl::D3d12GpuDescriptorHandle,
    com::{self, Hresult},
    device::Device,
    list::List,
};

/// Creates a closed command list (`CreateClosedCommandList`, `d3d12_command_list.cpp:2513`).
///
/// # Safety
///
/// `device`, `riid`, and `out` are caller-supplied COM pointers.
pub unsafe fn create_closed(
    _device: *mut Device,
    _riid: *const c_void,
    _out: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("list::state::create_closed")
}

/// `ID3D12Device::CreateCommandList` (`d3d12_device.cpp:526`).
///
/// # Safety
///
/// `this`, `allocator`, `initial_state`, `riid`, and `list` are caller-supplied COM pointers.
pub unsafe fn create_command_list(
    _this: *mut c_void,
    _node_mask: u32,
    _list_type: u32,
    _allocator: *mut c_void,
    _initial_state: *mut c_void,
    _riid: *const c_void,
    _list: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandList");
    crate::not_implemented!("ID3D12Device::CreateCommandList")
}

/// `ID3D12Device4::CreateCommandList1` (`d3d12_device.cpp:1577`).
///
/// # Safety
///
/// `this`, `riid`, and `list` are caller-supplied COM pointers.
pub unsafe fn create_command_list1(
    _this: *mut c_void,
    _node_mask: u32,
    _list_type: u32,
    _flags: u32,
    _riid: *const c_void,
    _list: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandList1");
    crate::not_implemented!("ID3D12Device::CreateCommandList1")
}

/// `ID3D12GraphicsCommandList::QueryInterface` (`d3d12_command_list.cpp:327`).
///
/// # Safety
///
/// `this`, `riid`, and `ppv_object` are caller-supplied COM pointers.
pub unsafe fn query_interface(
    _this: *mut c_void,
    _riid: *const c_void,
    _ppv_object: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::QueryInterface")
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
    _this: *mut c_void,
    _guid: *const c_void,
    _data_size: *mut u32,
    _data: *mut c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::GetPrivateData")
}

/// `ID3D12Object::SetPrivateData`.
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied pointers.
pub unsafe fn set_private_data(
    _this: *mut c_void,
    _guid: *const c_void,
    _data_size: u32,
    _data: *const c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::SetPrivateData")
}

/// `ID3D12Object::SetPrivateDataInterface`.
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied pointers.
pub unsafe fn set_private_data_interface(
    _this: *mut c_void,
    _guid: *const c_void,
    _data: *const c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::SetPrivateDataInterface")
}

/// `ID3D12Object::SetName`.
///
/// # Safety
///
/// `this` and `name` are caller-supplied pointers.
pub unsafe fn set_name(_this: *mut c_void, _name: *const c_void) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::SetName")
}

/// `ID3D12DeviceChild::GetDevice`.
///
/// # Safety
///
/// `this`, `riid`, and `device` are caller-supplied pointers.
pub unsafe fn get_device(
    _this: *mut c_void,
    _riid: *const c_void,
    _device: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::GetDevice")
}

/// `ID3D12CommandList::GetType` (`d3d12_command_list.cpp:344`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn get_type(_this: *mut c_void) -> u32 {
    crate::not_implemented!("ID3D12GraphicsCommandList::GetType")
}

/// `ID3D12GraphicsCommandList::Close` (`d3d12_command_list.cpp:349`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn close(_this: *mut c_void) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::Close")
}

/// `ID3D12GraphicsCommandList::Reset` (`d3d12_command_list.cpp:355`).
///
/// # Safety
///
/// `this`, `allocator`, and `initial_state` are caller-supplied pointers.
pub unsafe fn reset(
    _this: *mut c_void,
    _allocator: *mut c_void,
    _initial_state: *mut c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12GraphicsCommandList::Reset")
}

/// `ID3D12GraphicsCommandList::ClearState` (`d3d12_command_list.cpp:367`).
///
/// # Safety
///
/// `this` and `pipeline_state` are caller-supplied pointers.
pub unsafe fn clear_state(_this: *mut c_void, _pipeline_state: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ClearState");
}

/// `ID3D12GraphicsCommandList::IASetPrimitiveTopology` (`d3d12_command_list.cpp:1244`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn ia_set_primitive_topology(_this: *mut c_void, _topology: u32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::IASetPrimitiveTopology");
}

/// `ID3D12GraphicsCommandList::RSSetViewports` (`d3d12_command_list.cpp:1252`).
///
/// # Safety
///
/// `this` and `viewports` are caller-supplied pointers.
pub unsafe fn rs_set_viewports(_this: *mut c_void, _num_viewports: u32, _viewports: *const c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::RSSetViewports");
}

/// `ID3D12GraphicsCommandList::RSSetScissorRects` (`d3d12_command_list.cpp:1272`).
///
/// # Safety
///
/// `this` and `rects` are caller-supplied pointers.
pub unsafe fn rs_set_scissor_rects(_this: *mut c_void, _num_rects: u32, _rects: *const c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::RSSetScissorRects");
}

/// `ID3D12GraphicsCommandList::OMSetBlendFactor` (`d3d12_command_list.cpp:1296`).
///
/// # Safety
///
/// `this` and `blend_factor` are caller-supplied pointers.
pub unsafe fn om_set_blend_factor(_this: *mut c_void, _blend_factor: *const f32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::OMSetBlendFactor");
}

/// `ID3D12GraphicsCommandList::OMSetStencilRef` (`d3d12_command_list.cpp:1310`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn om_set_stencil_ref(_this: *mut c_void, _stencil_ref: u32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::OMSetStencilRef");
}

/// `ID3D12GraphicsCommandList::SetPipelineState` (`d3d12_command_list.cpp:1319`).
///
/// # Safety
///
/// `this` and `pipeline_state` are caller-supplied pointers.
pub unsafe fn set_pipeline_state(_this: *mut c_void, _pipeline_state: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetPipelineState");
}

/// `ID3D12GraphicsCommandList::ExecuteBundle` (`d3d12_command_list.cpp:1344`).
///
/// # Safety
///
/// `this` and `command_list` are caller-supplied pointers.
pub unsafe fn execute_bundle(_this: *mut c_void, _command_list: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ExecuteBundle");
}

/// `ID3D12GraphicsCommandList::SetDescriptorHeaps` (`d3d12_command_list.cpp:1351`).
///
/// # Safety
///
/// `this` and `descriptor_heaps` are caller-supplied pointers.
pub unsafe fn set_descriptor_heaps(
    _this: *mut c_void,
    _num_descriptor_heaps: u32,
    _descriptor_heaps: *const *mut c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetDescriptorHeaps");
}

/// `ID3D12GraphicsCommandList::SetComputeRootSignature` (`d3d12_command_list.cpp:1376`).
///
/// # Safety
///
/// `this` and `root_signature` are caller-supplied pointers.
pub unsafe fn set_compute_root_signature(_this: *mut c_void, _root_signature: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetComputeRootSignature");
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootSignature` (`d3d12_command_list.cpp:1389`).
///
/// # Safety
///
/// `this` and `root_signature` are caller-supplied pointers.
pub unsafe fn set_graphics_root_signature(_this: *mut c_void, _root_signature: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetGraphicsRootSignature");
}

/// `ID3D12GraphicsCommandList::SetComputeRootDescriptorTable` (`d3d12_command_list.cpp:1402`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_descriptor_table(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _base_descriptor: D3d12GpuDescriptorHandle,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetComputeRootDescriptorTable");
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootDescriptorTable` (`d3d12_command_list.cpp:1414`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_descriptor_table(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _base_descriptor: D3d12GpuDescriptorHandle,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetGraphicsRootDescriptorTable");
}

/// `ID3D12GraphicsCommandList::SetComputeRoot32BitConstant` (`d3d12_command_list.cpp:1426`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root32_bit_constant(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _src_data: u32,
    _dest_offset_in32_bit_values: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetComputeRoot32BitConstant");
}

/// `ID3D12GraphicsCommandList::SetGraphicsRoot32BitConstant` (`d3d12_command_list.cpp:1438`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root32_bit_constant(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _src_data: u32,
    _dest_offset_in32_bit_values: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetGraphicsRoot32BitConstant");
}

/// `ID3D12GraphicsCommandList::SetComputeRoot32BitConstants` (`d3d12_command_list.cpp:1450`).
///
/// # Safety
///
/// `this` and `src_data` are caller-supplied pointers.
pub unsafe fn set_compute_root32_bit_constants(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _num32_bit_values_to_set: u32,
    _src_data: *const c_void,
    _dest_offset_in32_bit_values: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetComputeRoot32BitConstants");
}

/// `ID3D12GraphicsCommandList::SetGraphicsRoot32BitConstants` (`d3d12_command_list.cpp:1464`).
///
/// # Safety
///
/// `this` and `src_data` are caller-supplied pointers.
pub unsafe fn set_graphics_root32_bit_constants(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _num32_bit_values_to_set: u32,
    _src_data: *const c_void,
    _dest_offset_in32_bit_values: u32,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetGraphicsRoot32BitConstants");
}

/// `ID3D12GraphicsCommandList::SetComputeRootConstantBufferView` (`d3d12_command_list.cpp:1478`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_constant_buffer_view(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _buffer_location: u64,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetComputeRootConstantBufferView");
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootConstantBufferView` (`d3d12_command_list.cpp:1490`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_constant_buffer_view(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _buffer_location: u64,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetGraphicsRootConstantBufferView");
}

/// `ID3D12GraphicsCommandList::SetComputeRootShaderResourceView` (`d3d12_command_list.cpp:1502`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_shader_resource_view(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _buffer_location: u64,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetComputeRootShaderResourceView");
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootShaderResourceView` (`d3d12_command_list.cpp:1514`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_shader_resource_view(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _buffer_location: u64,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetGraphicsRootShaderResourceView");
}

/// `ID3D12GraphicsCommandList::SetComputeRootUnorderedAccessView` (`d3d12_command_list.cpp:1526`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_compute_root_unordered_access_view(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _buffer_location: u64,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetComputeRootUnorderedAccessView");
}

/// `ID3D12GraphicsCommandList::SetGraphicsRootUnorderedAccessView` (`d3d12_command_list.cpp:1538`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_graphics_root_unordered_access_view(
    _this: *mut c_void,
    _root_parameter_index: u32,
    _buffer_location: u64,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::SetGraphicsRootUnorderedAccessView");
}

/// `ID3D12GraphicsCommandList::IASetIndexBuffer` (`d3d12_command_list.cpp:1550`).
///
/// # Safety
///
/// `this` and `view` are caller-supplied pointers.
pub unsafe fn ia_set_index_buffer(_this: *mut c_void, _view: *const c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::IASetIndexBuffer");
}

/// `ID3D12GraphicsCommandList::IASetVertexBuffers` (`d3d12_command_list.cpp:1580`).
///
/// # Safety
///
/// `this` and `views` are caller-supplied pointers.
pub unsafe fn ia_set_vertex_buffers(
    _this: *mut c_void,
    _start_slot: u32,
    _num_views: u32,
    _views: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::IASetVertexBuffers");
}

/// `ID3D12GraphicsCommandList::SOSetTargets` (`d3d12_command_list.cpp:1600`).
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
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SOSetTargets");
}

/// `ID3D12GraphicsCommandList::OMSetRenderTargets` (`d3d12_command_list.cpp:1615`).
///
/// # Safety
///
/// `this`, `render_target_descriptors`, and `depth_stencil_descriptor` are caller-supplied
/// pointers.
pub unsafe fn om_set_render_targets(
    _this: *mut c_void,
    _num_render_target_descriptors: u32,
    _render_target_descriptors: *const c_void,
    _rts_single_handle_to_descriptor_range: i32,
    _depth_stencil_descriptor: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::OMSetRenderTargets");
}

/// `ID3D12GraphicsCommandList::SetPredication` (`d3d12_command_list.cpp:2002`).
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
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetPredication");
}

/// `ID3D12GraphicsCommandList::SetMarker` (`d3d12_command_list.cpp:2010`).
///
/// # Safety
///
/// `this` and `data` are caller-supplied pointers.
pub unsafe fn set_marker(_this: *mut c_void, _metadata: u32, _data: *const c_void, _size: u32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetMarker");
}

/// `ID3D12GraphicsCommandList::BeginEvent` (`d3d12_command_list.cpp:2016`).
///
/// # Safety
///
/// `this` and `data` are caller-supplied pointers.
pub unsafe fn begin_event(_this: *mut c_void, _metadata: u32, _data: *const c_void, _size: u32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::BeginEvent");
}

/// `ID3D12GraphicsCommandList::EndEvent` (`d3d12_command_list.cpp:2022`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn end_event(_this: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::EndEvent");
}

/// `ID3D12GraphicsCommandList1::OMSetDepthBounds` (`d3d12_command_list.cpp:2114`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn om_set_depth_bounds(_this: *mut c_void, _min: f32, _max: f32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::OMSetDepthBounds");
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
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetSamplePositions");
}

/// `ID3D12GraphicsCommandList1::SetViewInstanceMask` (`d3d12_command_list.cpp:2268`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn set_view_instance_mask(_this: *mut c_void, _mask: u32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetViewInstanceMask");
}

/// `ID3D12GraphicsCommandList3::SetProtectedResourceSession` (`d3d12_command_list.cpp:2284`).
///
/// # Safety
///
/// `this` and `session` are caller-supplied pointers.
pub unsafe fn set_protected_resource_session(_this: *mut c_void, _session: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetProtectedResourceSession");
}

/// `ID3D12GraphicsCommandList4::SetPipelineState1` (`d3d12_command_list.cpp:2447`).
///
/// # Safety
///
/// `this` and `state_object` are caller-supplied pointers.
pub unsafe fn set_pipeline_state1(_this: *mut c_void, _state_object: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::SetPipelineState1");
}

/// `ID3D12GraphicsCommandList5::RSSetShadingRate` (`d3d12_command_list.cpp:2461`).
///
/// # Safety
///
/// `this` and `combiners` are caller-supplied pointers.
pub unsafe fn rs_set_shading_rate(
    _this: *mut c_void,
    _base_shading_rate: u32,
    _combiners: *const u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::RSSetShadingRate");
}

/// `ID3D12GraphicsCommandList5::RSSetShadingRateImage` (`d3d12_command_list.cpp:2469`).
///
/// # Safety
///
/// `this` and `shading_rate_image` are caller-supplied pointers.
pub unsafe fn rs_set_shading_rate_image(_this: *mut c_void, _shading_rate_image: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::RSSetShadingRateImage");
}
