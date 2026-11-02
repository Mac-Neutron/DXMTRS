//! D3D10 state-object vtables (`d3d10_state_object.{hpp,cpp}`).

use core::{ffi::c_void, mem::MaybeUninit};

use super::util::{
    child_add_ref, child_get_device, child_get_private_data, child_query_interface, child_release,
    child_set_private_data, child_set_private_data_interface, d11_to_d10_blend_desc,
    d11_to_d10_sampler_desc, wrapper_d3d11,
};
use crate::abi::{
    records::{
        D3d10BlendDesc, D3d10BlendDesc1, D3d10DepthStencilDesc, D3d10RasterizerDesc,
        D3d10SamplerDesc, D3d11BlendDesc, D3d11DepthStencilDesc, D3d11RasterizerDesc,
        D3d11SamplerDesc,
    },
    vtbl::{
        ID3D10BlendState1Vtbl, ID3D10DepthStencilStateVtbl, ID3D10RasterizerStateVtbl,
        ID3D10SamplerStateVtbl, ID3D11BlendStateVtbl, ID3D11DepthStencilStateVtbl,
        ID3D11RasterizerStateVtbl, ID3D11SamplerStateVtbl,
    },
};

const _: () = assert!(size_of::<D3d10BlendDesc1>() == size_of::<D3d11BlendDesc>());
const _: () = assert!(size_of::<D3d10DepthStencilDesc>() == size_of::<D3d11DepthStencilDesc>());
const _: () = assert!(size_of::<D3d10RasterizerDesc>() == size_of::<D3d11RasterizerDesc>());

unsafe extern "system-unwind" fn blend_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10BlendState1Vtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11BlendState` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11BlendStateVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live blend state vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11BlendDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_blend_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10BlendDesc`.
    unsafe { *p_desc.cast::<D3d10BlendDesc>() = d10_desc };
}

unsafe extern "system-unwind" fn blend_get_desc1(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10BlendState1Vtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11BlendState` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11BlendStateVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live blend state vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    // SAFETY: `D3d10BlendDesc1` and `D3d11BlendDesc` have identical layout.
    unsafe { get_desc_fn(d3d11, p_desc) };
}

unsafe extern "system-unwind" fn depth_stencil_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10DepthStencilStateVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11DepthStencilState` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11DepthStencilStateVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live depth-stencil state vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    // SAFETY: `D3d10DepthStencilDesc` and `D3d11DepthStencilDesc` have identical layout.
    unsafe { get_desc_fn(d3d11, p_desc) };
}

unsafe extern "system-unwind" fn rasterizer_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10RasterizerStateVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11RasterizerState` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11RasterizerStateVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live rasterizer state vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    // SAFETY: `D3d10RasterizerDesc` and `D3d11RasterizerDesc` have identical layout.
    unsafe { get_desc_fn(d3d11, p_desc) };
}

unsafe extern "system-unwind" fn sampler_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10SamplerStateVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11SamplerState` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11SamplerStateVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live sampler state vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11SamplerDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_sampler_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10SamplerDesc`.
    unsafe { *p_desc.cast::<D3d10SamplerDesc>() = d10_desc };
}

/// `ID3D10BlendState1` vtable for embedded `MTLD3D10BlendState` wrappers.
pub static BLEND_STATE_VTBL: ID3D10BlendState1Vtbl = ID3D10BlendState1Vtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_desc: blend_get_desc,
    get_desc1: blend_get_desc1,
};

/// `ID3D10RasterizerState` vtable for embedded `MTLD3D10RasterizerState` wrappers.
pub static RASTERIZER_STATE_VTBL: ID3D10RasterizerStateVtbl = ID3D10RasterizerStateVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_desc: rasterizer_get_desc,
};

/// `ID3D10SamplerState` vtable for embedded `MTLD3D10SamplerState` wrappers.
pub static SAMPLER_STATE_VTBL: ID3D10SamplerStateVtbl = ID3D10SamplerStateVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_desc: sampler_get_desc,
};

/// `ID3D10DepthStencilState` vtable for embedded `MTLD3D10DepthStencilState` wrappers.
pub static DEPTH_STENCIL_STATE_VTBL: ID3D10DepthStencilStateVtbl = ID3D10DepthStencilStateVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_desc: depth_stencil_get_desc,
};
