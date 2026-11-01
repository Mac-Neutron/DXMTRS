//! `MTLD3D10Query` (`ID3D10Query` wrapper, `d3d10_query.hpp`).

use core::ffi::c_void;

use super::{
    stub_add_ref, stub_get_desc, stub_get_device, stub_get_private_data, stub_qi, stub_release,
    stub_set_private_data, stub_set_private_data_interface,
};
use crate::{abi::vtbl::ID3D10QueryVtbl, com::E_NOTIMPL};

const unsafe extern "system-unwind" fn begin(_this: *mut c_void) {}
const unsafe extern "system-unwind" fn end(_this: *mut c_void) {}
const unsafe extern "system-unwind" fn get_data(
    _this: *mut c_void,
    _data: *mut c_void,
    _size: u32,
    _flags: u32,
) -> i32 {
    E_NOTIMPL
}
const unsafe extern "system-unwind" fn get_data_size(_this: *mut c_void) -> u32 {
    0
}

/// `ID3D10Query` vtable for embedded `MTLD3D10Query` wrappers.
pub static QUERY_VTBL: ID3D10QueryVtbl = ID3D10QueryVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    begin,
    end,
    get_data,
    get_data_size,
    get_desc: stub_get_desc,
};
