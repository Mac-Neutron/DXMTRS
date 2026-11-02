//! `MTLD3D10Buffer` (`ID3D10Buffer` wrapper, `d3d10_buffer.{hpp,cpp}`).

use core::{ffi::c_void, mem::MaybeUninit};

use super::util::{
    child_add_ref, child_get_device, child_get_private_data, child_query_interface, child_release,
    child_set_private_data, child_set_private_data_interface, d11_to_d10_buffer_desc,
    res_get_eviction_priority, res_get_type, res_set_eviction_priority, wrapper_context,
    wrapper_d3d11,
};
use crate::{
    abi::{
        records::{D3d10BufferDesc, D3d11BufferDesc, D3d11MappedSubresource},
        vtbl::{ID3D10BufferVtbl, ID3D11BufferVtbl, ID3D11DeviceContextVtbl},
    },
    com::S_OK,
};

unsafe extern "system-unwind" fn map(
    this: *mut c_void,
    map_type: u32,
    map_flags: u32,
    pp_data: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Wrapper<ID3D10BufferVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10BufferVtbl>`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a valid `ID3D11DeviceContext` COM interface pointer.
    let vtbl_ptr = unsafe { *ctx.cast::<*const ID3D11DeviceContextVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live context vtable.
    let map_fn = unsafe { (*vtbl_ptr).map };
    let mut subres = MaybeUninit::<D3d11MappedSubresource>::zeroed();
    // SAFETY: `ctx` and `d3d11` are valid COM pointers; `subres` is writable.
    let hr = unsafe {
        map_fn(
            ctx,
            d3d11,
            0,
            map_type,
            map_flags,
            subres.as_mut_ptr().cast::<c_void>(),
        )
    };
    if hr == S_OK {
        // SAFETY: `Map` succeeded and initialized `subres`.
        let mapped = unsafe { subres.assume_init() };
        // SAFETY: `pp_data` is caller-provided output storage on `Map`.
        unsafe { *pp_data = mapped.p_data };
    }
    hr
}

unsafe extern "system-unwind" fn unmap(this: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10BufferVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10BufferVtbl>`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a valid `ID3D11DeviceContext` COM interface pointer.
    let vtbl_ptr = unsafe { *ctx.cast::<*const ID3D11DeviceContextVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live context vtable.
    let unmap_fn = unsafe { (*vtbl_ptr).unmap };
    // SAFETY: `ctx` and `d3d11` are valid COM pointers.
    unsafe { unmap_fn(ctx, d3d11, 0) };
}

unsafe extern "system-unwind" fn get_desc(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10BufferVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11Buffer` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11BufferVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live buffer vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11BufferDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_buffer_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10BufferDesc`.
    unsafe { *p_desc.cast::<D3d10BufferDesc>() = d10_desc };
}

/// `ID3D10Buffer` vtable for embedded `MTLD3D10Buffer` wrappers.
pub static BUFFER_VTBL: ID3D10BufferVtbl = ID3D10BufferVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
    get_type: res_get_type,
    set_eviction_priority: res_set_eviction_priority,
    get_eviction_priority: res_get_eviction_priority,
    map,
    unmap,
    get_desc,
};
