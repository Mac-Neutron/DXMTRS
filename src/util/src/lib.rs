//! What DXMT's Rust DLLs (`nvngx.dll`, `nvapi64.dll`, `d3d10core.dll`, `dxgi.dll`) and D3D9 share.

pub mod boot;
pub mod com;
pub mod config;
pub mod dxgi_consts;
#[cfg(any(windows, target_os = "macos"))]
pub mod dxgi_vtbl;
pub mod env;
#[cfg(any(windows, target_os = "macos"))]
pub mod ext;
pub mod float_cast;
mod gamma;
pub mod log;
pub mod nvext;
pub mod private_data;
#[cfg(any(windows, target_os = "macos"))]
pub mod qpc;
pub mod sha1;
pub mod sync;
#[cfg(any(windows, target_os = "macos"))]
pub mod wsi;

pub use gamma::{GAMMA_CP_COUNT, GammaRamp};
#[cfg(any(windows, target_os = "macos"))]
pub use qpc::qpc;

/// The LUID DXMT gives the Metal device with `registry_id`: the id's bytes in reverse order.
///
/// DXGI's adapter and `ID3D12Device::GetAdapterLuid` answer it, and NVAPI's GPU queries.
#[must_use]
pub const fn luid_from_registry_id(registry_id: u64) -> [u8; 8] {
    registry_id.swap_bytes().to_ne_bytes()
}

#[cfg(test)]
mod tests;
