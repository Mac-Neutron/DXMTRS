//! `MTLD3D10Texture1D`, `MTLD3D10Texture2D` and `MTLD3D10Texture3D` (`d3d10_texture.hpp`).

use core::ffi::c_void;

use super::{
    stub_add_ref, stub_get_desc, stub_get_device, stub_get_private_data, stub_qi, stub_release,
    stub_set_private_data, stub_set_private_data_interface,
};
use crate::{
    abi::vtbl::{ID3D10Texture1DVtbl, ID3D10Texture2DVtbl, ID3D10Texture3DVtbl},
    com::E_NOTIMPL,
};

const unsafe extern "system-unwind" fn get_type(_this: *mut c_void, _out: *mut u32) {}
const unsafe extern "system-unwind" fn set_eviction_priority(_this: *mut c_void, _prio: u32) {}
const unsafe extern "system-unwind" fn get_eviction_priority(_this: *mut c_void) -> u32 {
    0
}
const unsafe extern "system-unwind" fn map_1d(
    _this: *mut c_void,
    _subresource: u32,
    _map_type: u32,
    _flags: u32,
    _data: *mut *mut c_void,
) -> i32 {
    E_NOTIMPL
}
const unsafe extern "system-unwind" fn map_2d_3d(
    _this: *mut c_void,
    _subresource: u32,
    _map_type: u32,
    _flags: u32,
    _mapped: *mut c_void,
) -> i32 {
    E_NOTIMPL
}
const unsafe extern "system-unwind" fn unmap(_this: *mut c_void, _subresource: u32) {}

/// `ID3D10Texture1D` vtable for embedded `MTLD3D10Texture1D` wrappers.
pub static TEXTURE1D_VTBL: ID3D10Texture1DVtbl = ID3D10Texture1DVtbl {
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
    map: map_1d,
    unmap,
    get_desc: stub_get_desc,
};

/// `ID3D10Texture2D` vtable for embedded `MTLD3D10Texture2D` wrappers.
pub static TEXTURE2D_VTBL: ID3D10Texture2DVtbl = ID3D10Texture2DVtbl {
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
    map: map_2d_3d,
    unmap,
    get_desc: stub_get_desc,
};

/// `ID3D10Texture3D` vtable for embedded `MTLD3D10Texture3D` wrappers.
pub static TEXTURE3D_VTBL: ID3D10Texture3DVtbl = ID3D10Texture3DVtbl {
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
    map: map_2d_3d,
    unmap,
    get_desc: stub_get_desc,
};
