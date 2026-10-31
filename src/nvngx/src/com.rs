//! The COM interfaces nvngx calls, and the structures they take.
//!
//! `IUnknown`, the D3D12 slots nvngx uses, and DXMT's private extensions, which
//! `dxmt_util::ext` holds (`d3d11_interfaces.hpp`, `d3d12_interfaces.hpp`); a slot nvngx never
//! calls is an opaque pointer. Layouts are the C++ headers', checked by the asserts below.

use core::ffi::c_void;

pub use dxmt_util::com::{Guid, Hresult, IUnknownVtbl, query, release, vtbl};

/// `ID3D12Device`'s IID.
pub static IID_ID3D12_DEVICE: Guid = Guid::new(
    0x1898_19f1,
    0x1db6,
    0x4b57,
    [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7],
);

/// `ID3D12GraphicsCommandList` up to `GetDevice` (`ID3D12DeviceChild`'s).
#[repr(C)]
pub struct CommandList12Vtbl {
    pub unknown: IUnknownVtbl,
    _object: [*const c_void; 4],
    pub get_device:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
}

/// `D3D12_RESOURCE_DESC`.
#[repr(C)]
#[derive(Default)]
pub struct ResourceDesc12 {
    pub dimension: u32,
    pub alignment: u64,
    pub width: u64,
    pub height: u32,
    pub depth_or_array_size: u16,
    pub mip_levels: u16,
    pub format: u32,
    pub sample_count: u32,
    pub sample_quality: u32,
    pub layout: u32,
    pub flags: u32,
}

/// `ID3D12Resource` up to `GetDesc`, whose struct return comes back through a pointer after `this`.
#[repr(C)]
pub struct Resource12Vtbl {
    pub unknown: IUnknownVtbl,
    _object_child_map_unmap: [*const c_void; 7],
    pub get_desc:
        unsafe extern "system" fn(*mut c_void, *mut ResourceDesc12) -> *mut ResourceDesc12,
}

const _: () = {
    use core::mem::offset_of;
    let slot = size_of::<usize>();
    assert!(size_of::<ResourceDesc12>() == 56 && offset_of!(ResourceDesc12, width) == 16);
    assert!(offset_of!(ResourceDesc12, height) == 24 && offset_of!(ResourceDesc12, format) == 32);
    assert!(offset_of!(CommandList12Vtbl, get_device) == 7 * slot);
    assert!(offset_of!(Resource12Vtbl, get_desc) == 10 * slot);
};
