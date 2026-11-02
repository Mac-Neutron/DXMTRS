//! D3D10 resource-view vtables (`d3d10_view.{hpp,cpp}`).

use core::{ffi::c_void, mem::MaybeUninit};

use super::util::{
    child_add_ref, child_get_device, child_get_private_data, child_query_interface, child_release,
    child_set_private_data, child_set_private_data_interface, d11_to_d10_dsv_desc,
    d11_to_d10_rtv_desc, d11_to_d10_srv_desc, d11_to_d10_srv_desc1, view_get_resource,
    wrapper_d3d11,
};
use crate::abi::{
    records::{
        D3d10DepthStencilViewDesc, D3d10RenderTargetViewDesc, D3d10ShaderResourceViewDesc,
        D3d10ShaderResourceViewDesc1, D3d11DepthStencilViewDesc, D3d11RenderTargetViewDesc,
        D3d11ShaderResourceViewDesc,
    },
    vtbl::{
        ID3D10DepthStencilViewVtbl, ID3D10RenderTargetViewVtbl, ID3D10ShaderResourceView1Vtbl,
        ID3D11DepthStencilViewVtbl, ID3D11RenderTargetViewVtbl, ID3D11ShaderResourceViewVtbl,
    },
};

unsafe extern "system-unwind" fn srv_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10ShaderResourceView1Vtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11ShaderResourceView` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11ShaderResourceViewVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live SRV vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11ShaderResourceViewDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_srv_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10ShaderResourceViewDesc`.
    unsafe { *p_desc.cast::<D3d10ShaderResourceViewDesc>() = d10_desc };
}

unsafe extern "system-unwind" fn srv_get_desc1(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10ShaderResourceView1Vtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11ShaderResourceView` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11ShaderResourceViewVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live SRV vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11ShaderResourceViewDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_srv_desc1(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10ShaderResourceViewDesc1`.
    unsafe { *p_desc.cast::<D3d10ShaderResourceViewDesc1>() = d10_desc };
}

unsafe extern "system-unwind" fn rtv_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10RenderTargetViewVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11RenderTargetView` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11RenderTargetViewVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live RTV vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11RenderTargetViewDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_rtv_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10RenderTargetViewDesc`.
    unsafe { *p_desc.cast::<D3d10RenderTargetViewDesc>() = d10_desc };
}

unsafe extern "system-unwind" fn dsv_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10DepthStencilViewVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11DepthStencilView` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11DepthStencilViewVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live DSV vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11DepthStencilViewDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_dsv_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10DepthStencilViewDesc`.
    unsafe { *p_desc.cast::<D3d10DepthStencilViewDesc>() = d10_desc };
}

/// `ID3D10ShaderResourceView1` vtable for embedded `MTLD3D10ShaderResourceView` wrappers.
pub static SRV_VTBL: ID3D10ShaderResourceView1Vtbl = ID3D10ShaderResourceView1Vtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_resource: view_get_resource,
    get_desc: srv_get_desc,
    get_desc1: srv_get_desc1,
};

/// `ID3D10RenderTargetView` vtable for embedded `MTLD3D10RenderTargetView` wrappers.
pub static RTV_VTBL: ID3D10RenderTargetViewVtbl = ID3D10RenderTargetViewVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_resource: view_get_resource,
    get_desc: rtv_get_desc,
};

/// `ID3D10DepthStencilView` vtable for embedded `MTLD3D10DepthStencilView` wrappers.
pub static DSV_VTBL: ID3D10DepthStencilViewVtbl = ID3D10DepthStencilViewVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_resource: view_get_resource,
    get_desc: dsv_get_desc,
};
