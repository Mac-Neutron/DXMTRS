//! `PreDraw`, `PreDispatch`, draw/dispatch commands, and raytracing/mesh stubs
//!
//! (`d3d12_command_list.cpp`).

use core::ffi::c_void;

use crate::list::{DrawCallStatus, List};

/// Flushes dirty graphics state into the active
///
/// [`RenderEncoderData`](crate::encoder::RenderEncoderData)
/// (`PreDraw`, `d3d12_command_list.cpp:1428`).
pub fn pre_draw(_list: &mut List) -> DrawCallStatus {
    let (): () = crate::not_implemented!("list::draw::pre_draw");
    DrawCallStatus::Invalid
}

/// Flushes dirty compute state into the active
///
/// [`ComputeEncoderData`](crate::encoder::ComputeEncoderData)
/// (`PreDispatch`, `d3d12_command_list.cpp:1661`).
pub fn pre_dispatch(_list: &mut List) -> DrawCallStatus {
    let (): () = crate::not_implemented!("list::draw::pre_dispatch");
    DrawCallStatus::Invalid
}

/// `ID3D12GraphicsCommandList::DrawInstanced` (`d3d12_command_list.cpp:421`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn draw_instanced(
    _this: *mut c_void,
    _vertex_count_per_instance: u32,
    _instance_count: u32,
    _start_vertex_location: u32,
    _start_instance_location: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::DrawInstanced");
}

/// `ID3D12GraphicsCommandList::DrawIndexedInstanced` (`d3d12_command_list.cpp:469`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn draw_indexed_instanced(
    _this: *mut c_void,
    _index_count_per_instance: u32,
    _instance_count: u32,
    _start_index_location: u32,
    _base_vertex_location: i32,
    _start_instance_location: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::DrawIndexedInstanced");
}

/// `ID3D12GraphicsCommandList::Dispatch` (`d3d12_command_list.cpp:524`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn dispatch(_this: *mut c_void, _x: u32, _y: u32, _z: u32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::Dispatch");
}

/// `ID3D12GraphicsCommandList4::InitializeMetaCommand` (`d3d12_command_list.cpp:2408`).
///
/// # Safety
///
/// `this`, `meta_command`, and `initialization_parameters_data` are caller-supplied pointers.
pub unsafe fn initialize_meta_command(
    _this: *mut c_void,
    _meta_command: *mut c_void,
    _initialization_parameters_data: *const c_void,
    _initialization_parameters_data_size_in_bytes: usize,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::InitializeMetaCommand");
}

/// `ID3D12GraphicsCommandList4::ExecuteMetaCommand` (`d3d12_command_list.cpp:2417`).
///
/// # Safety
///
/// `this`, `meta_command`, and `execution_parameters_data` are caller-supplied pointers.
pub unsafe fn execute_meta_command(
    _this: *mut c_void,
    _meta_command: *mut c_void,
    _execution_parameters_data: *const c_void,
    _execution_parameters_data_size_in_bytes: usize,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ExecuteMetaCommand");
}

/// `ID3D12GraphicsCommandList4::BuildRaytracingAccelerationStructure`
///
/// (`d3d12_command_list.cpp:2426`).
///
/// # Safety
///
/// `this`, `desc`, and `postbuild_info_descs` are caller-supplied pointers.
pub unsafe fn build_raytracing_acceleration_structure(
    _this: *mut c_void,
    _desc: *const c_void,
    _num_postbuild_info_descs: u32,
    _postbuild_info_descs: *const c_void,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::BuildRaytracingAccelerationStructure");
}

/// `ID3D12GraphicsCommandList4::EmitRaytracingAccelerationStructurePostbuildInfo`
///
/// (`d3d12_command_list.cpp:2435`).
///
/// # Safety
///
/// `this`, `desc`, and `source_acceleration_structure_data` are caller-supplied pointers.
pub unsafe fn emit_raytracing_acceleration_structure_postbuild_info(
    _this: *mut c_void,
    _desc: *const c_void,
    _num_source_acceleration_structures: u32,
    _source_acceleration_structure_data: *const u64,
) {
    let (): () =
        crate::not_implemented!("CommandList::EmitRaytracingAccelerationStructurePostbuildInfo");
}

/// `ID3D12GraphicsCommandList4::CopyRaytracingAccelerationStructure`
///
/// (`d3d12_command_list.cpp:2440`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn copy_raytracing_acceleration_structure(
    _this: *mut c_void,
    _dest_acceleration_structure_data: u64,
    _source_acceleration_structure_data: u64,
    _mode: u32,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::CopyRaytracingAccelerationStructure");
}

/// `ID3D12GraphicsCommandList4::DispatchRays` (`d3d12_command_list.cpp:2454`).
///
/// # Safety
///
/// `this` and `desc` are caller-supplied pointers.
pub unsafe fn dispatch_rays(_this: *mut c_void, _desc: *const c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::DispatchRays");
}

/// `ID3D12GraphicsCommandList6::DispatchMesh` (`d3d12_command_list.cpp:2476`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn dispatch_mesh(_this: *mut c_void, _x: u32, _y: u32, _z: u32) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::DispatchMesh");
}
