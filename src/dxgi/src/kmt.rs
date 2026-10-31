//! D3DKMT's adapter handle for an adapter's LUID; stubs for now.

use crate::abi::Luid;

/// Opens the D3DKMT adapter of `luid`: its handle, or 0.
#[must_use]
pub const fn open(_luid: Luid) -> u32 {
    0
}

/// Closes a handle `open` gave.
pub const fn close(_handle: u32) {}
