//! DXMT's `dxgi.dll` and `dxgi.dylib`: the DXGI factory, adapter and output over Metal.

// Public while the lanes fill the stubs, so that an item with no caller yet builds under
// `-D warnings`: a private module's unused item is dead code. The exports stay private: their names
// are the DLL's, not the crate's.
pub mod abi;
pub mod adapter;
pub mod com;
pub mod config;
mod exports;
pub mod factory;
pub mod ffi;
pub mod kmt;
pub mod modes;
pub mod nvext;
pub mod output;
pub use dxmt_util::{private_data, wsi};

dxmt_util::dxmt_dll!(log "dxgi.log", allocator);
