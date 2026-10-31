//! The interfaces `d3d10core` calls: two IIDs, the slots it uses and the values it passes.
//!
//! `src/dxgi/src/abi.rs` is the home of the DXGI and D3D10 ABI; a staticlib crate cannot depend on
//! another's exports, so `d3d10core` states these itself. A slot it never calls is an opaque
//! pointer. Layouts are the SDK headers', checked by the asserts below.

use core::ffi::c_void;

pub use dxmt_util::com::{E_INVALIDARG, E_NOTIMPL};
use dxmt_util::com::{Guid, Hresult, IUnknownVtbl};

/// `ID3D10Device`'s IID.
pub static IID_ID3D10_DEVICE: Guid = Guid::new(
    0x9b7e_4c0f,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);
/// `ID3D10Multithread`'s IID.
pub static IID_ID3D10_MULTITHREAD: Guid = Guid::new(
    0x9b7e_4e00,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `D3D10_CREATE_DEVICE_SINGLETHREADED`.
pub const D3D10_CREATE_DEVICE_SINGLETHREADED: u32 = 1;
/// DXMT's private `D3D11CoreCreateDevice` flag (`DXMT_D3D10_DEVICE`): the device is a D3D10 one.
pub const DXMT_D3D10_DEVICE: u32 = 0x8000_0000;

/// `IDXGIAdapter` up to `CheckInterfaceSupport`.
#[repr(C)]
pub struct AdapterCheckVtbl {
    pub unknown: IUnknownVtbl,
    /// `IDXGIObject`'s four slots, then `EnumOutputs` and `GetDesc`.
    _object_and_adapter: [*const c_void; 6],
    pub check_interface_support:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut i64) -> Hresult,
}

/// `ID3D10Multithread` up to `SetMultithreadProtected`.
#[repr(C)]
pub struct MultithreadVtbl {
    pub unknown: IUnknownVtbl,
    /// `Enter` and `Leave`.
    _lock: [*const c_void; 2],
    pub set_multithread_protected: unsafe extern "system" fn(*mut c_void, i32) -> i32,
}

const _: () = {
    use core::mem::offset_of;
    let slot = size_of::<usize>();
    assert!(offset_of!(AdapterCheckVtbl, check_interface_support) == 9 * slot);
    assert!(offset_of!(MultithreadVtbl, set_multithread_protected) == 5 * slot);
};

#[cfg(test)]
mod tests;
