//! `MTLD3D10Buffer` (`ID3D10Buffer` wrapper, `d3d10_buffer.hpp`).

use core::ffi::c_void;

use super::{
    stub_add_ref, stub_get_desc, stub_get_device, stub_get_private_data, stub_qi, stub_release,
    stub_set_private_data, stub_set_private_data_interface,
};
use crate::{abi::vtbl::ID3D10BufferVtbl, com::E_NOTIMPL};

const unsafe extern "system-unwind" fn get_type(_this: *mut c_void, _out: *mut u32) {}
const unsafe extern "system-unwind" fn set_eviction_priority(_this: *mut c_void, _prio: u32) {}
const unsafe extern "system-unwind" fn get_eviction_priority(_this: *mut c_void) -> u32 {
    0
}
const unsafe extern "system-unwind" fn map(
    _this: *mut c_void,
    _map_type: u32,
    _flags: u32,
    _data: *mut *mut c_void,
) -> i32 {
    E_NOTIMPL
}
const unsafe extern "system-unwind" fn unmap(_this: *mut c_void) {}

/// `ID3D10Buffer` vtable for embedded `MTLD3D10Buffer` wrappers.
pub static BUFFER_VTBL: ID3D10BufferVtbl = ID3D10BufferVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_type,
    set_eviction_priority,
    get_eviction_priority,
    map,
    unmap,
    get_desc: stub_get_desc,
};
