//! What DXMT's Rust DLLs (`nvngx.dll`, `nvapi64.dll`, `d3d10core.dll`, `dxgi.dll`) share.

pub mod com;
pub mod env;
pub mod float_cast;
pub mod log;
pub mod nvext;
pub mod sync;

/// The LUID DXMT gives the Metal device with `registry_id`: the id's bytes in reverse order.
///
/// DXGI's adapter and `ID3D12Device::GetAdapterLuid` answer it, and NVAPI's GPU queries.
#[must_use]
pub const fn luid_from_registry_id(registry_id: u64) -> [u8; 8] {
    registry_id.swap_bytes().to_ne_bytes()
}

#[cfg(test)]
mod tests;
