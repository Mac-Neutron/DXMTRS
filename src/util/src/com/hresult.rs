//! Every `HRESULT` code the Rust front ends name.
//!
//! The generic COM codes are defined in `com.rs` (next to [`Hresult`]); this
//! module re-exports them beside the `DXGI_*` and `D3D12_*` ones, so a front end imports its
//! codes from here and defines none itself.

use super::Hresult;
pub use super::{
    DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL,
    E_OUTOFMEMORY, E_POINTER, S_FALSE, S_OK,
};

/// `DXGI_STATUS_OCCLUDED`.
pub const DXGI_STATUS_OCCLUDED: Hresult = 0x087a_0001;
/// `DXGI_STATUS_MODE_CHANGE_IN_PROGRESS`.
pub const DXGI_STATUS_MODE_CHANGE_IN_PROGRESS: Hresult = 0x087a_0008;
/// `DXGI_STATUS_PRESENT_REQUIRED`.
pub const DXGI_STATUS_PRESENT_REQUIRED: Hresult = 0x087a_0025;
/// `DXGI_ERROR_INVALID_CALL`.
pub const DXGI_ERROR_INVALID_CALL: Hresult = 0x887a_0001_u32.cast_signed();
/// `DXGI_ERROR_UNSUPPORTED`.
pub const DXGI_ERROR_UNSUPPORTED: Hresult = 0x887a_0004_u32.cast_signed();
/// `DXGI_ERROR_WAS_STILL_DRAWING`.
pub const DXGI_ERROR_WAS_STILL_DRAWING: Hresult = 0x887a_000a_u32.cast_signed();
/// `DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`.
pub const DXGI_ERROR_NOT_CURRENTLY_AVAILABLE: Hresult = 0x887a_0022_u32.cast_signed();
/// `DXGI_ERROR_ALREADY_EXISTS`.
pub const DXGI_ERROR_ALREADY_EXISTS: Hresult = 0x887a_0036_u32.cast_signed();
/// `D3D12_ERROR_DRIVER_VERSION_MISMATCH`.
pub const D3D12_ERROR_DRIVER_VERSION_MISMATCH: Hresult = 0x887e_0002_u32.cast_signed();

#[cfg(test)]
mod tests;
