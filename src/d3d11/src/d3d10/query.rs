//! `MTLD3D10Query` (`ID3D10Query` wrapper, `d3d10_query.hpp`).

use core::{ffi::c_void, mem::MaybeUninit};

use super::util::{
    child_add_ref, child_get_device, child_get_private_data, child_query_interface, child_release,
    child_set_private_data, child_set_private_data_interface, d11_to_d10_query_desc,
    wrapper_context, wrapper_d3d11,
};
use crate::abi::{
    records::{D3d10QueryDesc, D3d11QueryDesc},
    vtbl::{ID3D10QueryVtbl, ID3D11DeviceContextVtbl, ID3D11QueryVtbl},
};

unsafe extern "system-unwind" fn begin(this: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a valid `ID3D11DeviceContext` COM interface pointer.
    let vtbl_ptr = unsafe { *ctx.cast::<*const ID3D11DeviceContextVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live context vtable.
    let begin_fn = unsafe { (*vtbl_ptr).begin };
    // SAFETY: `ctx` and `d3d11` are valid COM pointers.
    unsafe { begin_fn(ctx, d3d11) };
}

unsafe extern "system-unwind" fn end(this: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a valid `ID3D11DeviceContext` COM interface pointer.
    let vtbl_ptr = unsafe { *ctx.cast::<*const ID3D11DeviceContextVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live context vtable.
    let end_fn = unsafe { (*vtbl_ptr).end };
    // SAFETY: `ctx` and `d3d11` are valid COM pointers.
    unsafe { end_fn(ctx, d3d11) };
}

unsafe extern "system-unwind" fn get_data(
    this: *mut c_void,
    p_data: *mut c_void,
    data_size: u32,
    get_data_flags: u32,
) -> i32 {
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a valid `ID3D11DeviceContext` COM interface pointer.
    let vtbl_ptr = unsafe { *ctx.cast::<*const ID3D11DeviceContextVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live context vtable.
    let get_data_fn = unsafe { (*vtbl_ptr).get_data };
    // SAFETY: `ctx` and `d3d11` are valid COM pointers; `p_data` and `data_size` come from caller.
    unsafe { get_data_fn(ctx, d3d11, p_data, data_size, get_data_flags) }
}

unsafe extern "system-unwind" fn get_data_size(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11Query` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11QueryVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live query vtable.
    let get_data_size_fn = unsafe { (*vtbl_ptr).get_data_size };
    // SAFETY: `d3d11` is a valid COM pointer.
    unsafe { get_data_size_fn(d3d11) }
}

unsafe extern "system-unwind" fn get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10QueryVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11Query` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11QueryVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live query vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11QueryDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_query_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10QueryDesc`.
    unsafe { *p_desc.cast::<D3d10QueryDesc>() = d10_desc };
}

/// `ID3D10Query` vtable for embedded `MTLD3D10Query` wrappers.
pub static QUERY_VTBL: ID3D10QueryVtbl = ID3D10QueryVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    begin,
    end,
    get_data,
    get_data_size,
    get_desc,
};
