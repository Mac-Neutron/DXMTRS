//! D3D10 resource-view vtables (`d3d10_view.cpp`).

use core::ffi::c_void;

use super::{
    stub_add_ref, stub_get_desc, stub_get_device, stub_get_private_data, stub_qi, stub_release,
    stub_set_private_data, stub_set_private_data_interface,
};
use crate::abi::vtbl::{
    ID3D10DepthStencilViewVtbl, ID3D10RenderTargetViewVtbl, ID3D10ShaderResourceView1Vtbl,
};

const unsafe extern "system-unwind" fn get_resource(_this: *mut c_void, _out: *mut *mut c_void) {}

/// `ID3D10ShaderResourceView1` vtable for embedded `MTLD3D10ShaderResourceView` wrappers.
pub static SRV_VTBL: ID3D10ShaderResourceView1Vtbl = ID3D10ShaderResourceView1Vtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_resource,
    get_desc: stub_get_desc,
    get_desc1: stub_get_desc,
};

/// `ID3D10RenderTargetView` vtable for embedded `MTLD3D10RenderTargetView` wrappers.
pub static RTV_VTBL: ID3D10RenderTargetViewVtbl = ID3D10RenderTargetViewVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_resource,
    get_desc: stub_get_desc,
};

/// `ID3D10DepthStencilView` vtable for embedded `MTLD3D10DepthStencilView` wrappers.
pub static DSV_VTBL: ID3D10DepthStencilViewVtbl = ID3D10DepthStencilViewVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_resource,
    get_desc: stub_get_desc,
};
