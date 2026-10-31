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
pub mod private_data;
pub mod wsi;

use dxmt_util::log::Logger;

/// This DLL's log file.
///
/// Process-wide resource: one log file per DLL, opened by the first message that passes the
/// level, and the exports have no object to hold it. It is DXMT's `Logger::s_instance`.
static LOG: Logger = Logger::new("dxgi.log");
