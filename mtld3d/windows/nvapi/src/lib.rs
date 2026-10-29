//! DXMT's `nvapi64.dll`: NVIDIA's NVAPI, answered for Metal GPUs.

pub mod d3d;
pub mod disp;
pub mod ffi;
pub mod gpu;
pub mod nv;
pub mod sys;

use core::ffi::c_void;
use dxmt_util::log::Logger;

/// This DLL's log file.
///
/// Process-wide resource: one log file per DLL, opened by the first message that passes the
/// level, and the exports have no object to hold it. It is DXMT's `Logger::s_instance`.
static LOG: Logger = Logger::new("nvapi.log");

/// NVIDIA's table of function names and IDs.
const INTERFACE_TABLE: &str = include_str!("../../../../external/nvapi/nvapi_interface.h");

/// The IDs DXMT answers with null and no log line: IDs the table does not name that games probe.
const UNNAMED_IDS: [u32; 8] =
    [0x33c7_358c, 0x593e_8644, 0xad29_8d3f, 0xd7c6_1344, 0xd44d_3c4e, 0x6d67_533c, 0xb7fb_dbfa, 0xc99f_4a67];

/// The table's entries: a function's name and its ID.
///
/// Each entry is a line like `{ "NvAPI_Initialize", 0x0150e828 },`.
fn table() -> impl Iterator<Item = (&'static str, u32)> {
    INTERFACE_TABLE.lines().filter_map(|line| {
        let (_, rest) = line.split_once('"')?;
        let (name, rest) = rest.split_once('"')?;
        let (_, hex) = rest.split_once("0x")?;
        let hex = hex.split(|c: char| !c.is_ascii_hexdigit()).next()?;
        Some((name, u32::from_str_radix(hex, 16).ok()?))
    })
}

/// The name NVIDIA's table gives `id`.
fn name_of(id: u32) -> Option<&'static str> {
    table().find(|&(_, entry)| entry == id).map(|(name, _)| name)
}

/// The error line for a function DXMT does not provide: its name, or its ID if the table has none.
fn not_implemented(id: u32) -> String {
    name_of(id).map_or_else(
        || format!("nvapi: function id {id} not implemented"),
        |name| format!("nvapi: function {name} not implemented"),
    )
}

/// NVAPI's one export: the address of the function `id` names, or null.
///
/// A null answer is logged as an error (the game asked for a function DXMT lacks), except for
/// the IDs games probe.
#[unsafe(no_mangle)]
pub extern "C" fn nvapi_QueryInterface(id: u32) -> *mut c_void {
    if UNNAMED_IDS.contains(&id) {
        return core::ptr::null_mut();
    }
    for query in [sys::query, gpu::query, disp::query, d3d::query] {
        let function = query(id);
        if !function.is_null() {
            return function;
        }
    }
    LOG.err(&not_implemented(id));
    core::ptr::null_mut()
}

#[cfg(test)]
mod tests;
