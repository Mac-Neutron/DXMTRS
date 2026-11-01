//! Feature queries, allocation info, heap properties, and footprints (`d3d12_device.cpp`).

use core::ffi::c_void;

use crate::com::Hresult;

/// `ID3D12Device::CheckFeatureSupport` (`d3d12_device.cpp:544`).
///
/// # Safety
///
/// `this` and `data` are caller-supplied pointers.
pub unsafe fn check_feature_support(
    _this: *mut c_void,
    _feature: u32,
    _data: *mut c_void,
    _size: u32,
) -> Hresult {
    crate::scope!("device.CheckFeatureSupport");
    crate::not_implemented!("ID3D12Device::CheckFeatureSupport")
}

/// `ID3D12Device::GetResourceAllocationInfo` (`d3d12_device.cpp:956`).
///
/// # Safety
///
/// `this`, `ret`, and `descs` are caller-supplied pointers.
pub unsafe fn get_resource_allocation_info(
    _this: *mut c_void,
    ret: *mut c_void,
    _visible_mask: u32,
    _num_descs: u32,
    _descs: *const c_void,
) -> *mut c_void {
    crate::scope!("device.GetResourceAllocationInfo");
    let (): () = crate::not_implemented!("ID3D12Device::GetResourceAllocationInfo");
    ret
}

/// `ID3D12Device::GetCustomHeapProperties` (`d3d12_device.cpp:964`).
///
/// # Safety
///
/// `this` and `ret` are caller-supplied pointers.
pub unsafe fn get_custom_heap_properties(
    _this: *mut c_void,
    ret: *mut c_void,
    _node_mask: u32,
    _heap_type: u32,
) -> *mut c_void {
    crate::scope!("device.GetCustomHeapProperties");
    let (): () = crate::not_implemented!("ID3D12Device::GetCustomHeapProperties");
    ret
}

/// `ID3D12Device::GetCopyableFootprints` (`d3d12_device.cpp:1291`).
///
/// # Safety
///
/// `this`, `desc`, `layouts`, `num_rows`, `row_sizes`, and `total_bytes` are caller-supplied
/// pointers.
pub unsafe extern "system-unwind" fn get_copyable_footprints(
    _this: *mut c_void,
    _desc: *const c_void,
    _first_sub: u32,
    _num_sub: u32,
    _base_offset: u64,
    _layouts: *mut c_void,
    _num_rows: *mut u32,
    _row_sizes: *mut u64,
    _total_bytes: *mut u64,
) {
    crate::scope!("device.GetCopyableFootprints");
    let (): () = crate::not_implemented!("ID3D12Device::GetCopyableFootprints");
}

/// `ID3D12Device::GetResourceTiling` (`d3d12_device.cpp:1463`).
///
/// # Safety
///
/// `this` and the out-parameter pointers are caller-supplied.
pub unsafe extern "system-unwind" fn get_resource_tiling(
    _this: *mut c_void,
    _tiled_resource: *mut c_void,
    _num_tiles: *mut u32,
    _packed_mip_desc: *mut c_void,
    _standard_tile_shape: *mut c_void,
    _num_sub_tilings: *mut u32,
    _first_sub_tiling: u32,
    _sub_tilings: *mut c_void,
) {
    crate::scope!("device.GetResourceTiling");
    let (): () = crate::not_implemented!("ID3D12Device::GetResourceTiling");
}

/// `ID3D12Device4::GetResourceAllocationInfo1` (`d3d12_device.cpp:1632`).
///
/// # Safety
///
/// `this`, `ret`, `descs`, and `info1` are caller-supplied pointers.
pub unsafe fn get_resource_allocation_info1(
    _this: *mut c_void,
    ret: *mut c_void,
    _visible_mask: u32,
    _num_descs: u32,
    _descs: *const c_void,
    _info1: *mut c_void,
) -> *mut c_void {
    crate::scope!("device.GetResourceAllocationInfo1");
    let (): () = crate::not_implemented!("ID3D12Device::GetResourceAllocationInfo1");
    ret
}

/// `ID3D12Device8::GetResourceAllocationInfo2` (`d3d12_device.cpp:1777`).
///
/// # Safety
///
/// `this`, `ret`, `descs`, and `info1` are caller-supplied pointers.
pub unsafe fn get_resource_allocation_info2(
    _this: *mut c_void,
    ret: *mut c_void,
    _visible_mask: u32,
    _num_descs: u32,
    _descs: *const c_void,
    _info1: *mut c_void,
) -> *mut c_void {
    crate::scope!("device.GetResourceAllocationInfo2");
    let (): () = crate::not_implemented!("ID3D12Device::GetResourceAllocationInfo2");
    ret
}

/// `ID3D12Device8::GetCopyableFootprints1` (`d3d12_device.cpp:1798`).
///
/// # Safety
///
/// `this`, `desc`, `layouts`, `num_rows`, `row_sizes`, and `total_bytes` are caller-supplied
/// pointers.
pub unsafe extern "system-unwind" fn get_copyable_footprints1(
    _this: *mut c_void,
    _desc: *const c_void,
    _first_sub: u32,
    _num_sub: u32,
    _base_offset: u64,
    _layouts: *mut c_void,
    _num_rows: *mut u32,
    _row_sizes: *mut u64,
    _total_bytes: *mut u64,
) {
    crate::scope!("device.GetCopyableFootprints1");
    let (): () = crate::not_implemented!("ID3D12Device::GetCopyableFootprints1");
}
