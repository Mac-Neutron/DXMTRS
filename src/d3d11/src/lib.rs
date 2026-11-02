//! DXMT's `d3d11.dll` and `d3d11.dylib`: the D3D11 and D3D10 front end over Metal.
//!
//! The static library also carries DXMT's core, whose `extern "C"` functions the C++ front end
//! calls while `front-end` is off.

use dxmt_core as _;
use dxmt_util::log::Logger;

pub mod abi;
pub mod annotation;
pub mod class_linkage;
pub mod com;
pub mod context;
pub mod d3d10;
pub mod device;
pub mod exports;
pub mod fence;
pub mod input_layout;
pub mod inspection;
pub mod keyed_mutex;
pub mod kmt;
pub mod multithread;
pub mod names;
pub mod pipeline;
pub mod query;
pub mod resource;
pub mod scheduler;
pub mod shader;
pub mod state;
pub mod subresource;
pub mod surface;
pub mod swapchain;
pub mod view;

/// This DLL's log file (`Logger::s_instance("d3d11.log")`).
pub static LOG: Logger = Logger::new("d3d11.log");

/// The `type_info` vtable under its MSVC decorated name (`std::thread::spawn` landing pad).
#[cfg(windows)]
#[unsafe(export_name = "??_7type_info@@6B@")]
pub static TYPE_INFO_VFTABLE: [usize; 1] = [0];

/// The core's log writer (`dxmt_core::log`), forwarding to [`LOG`].
#[cfg(feature = "front-end")]
#[unsafe(export_name = "dxmt_log")]
extern "C" fn dxmt_log(level: u32, message: *const u8, length: usize) {
    if message.is_null() {
        return;
    }
    // SAFETY: `dxmt_core::log` passes a valid UTF-8 slice pointer and length for the call.
    let bytes = unsafe { core::slice::from_raw_parts(message, length) };
    let Ok(text) = core::str::from_utf8(bytes) else {
        return;
    };
    match level {
        0 => LOG.trace(text),
        1 => LOG.debug(text),
        2 => LOG.info(text),
        3 => LOG.warn(text),
        _ => LOG.err(text),
    }
}
