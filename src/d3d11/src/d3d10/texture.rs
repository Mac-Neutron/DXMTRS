//! `MTLD3D10Texture1D`, `MTLD3D10Texture2D` and `MTLD3D10Texture3D` (`d3d10_texture.{hpp,cpp}`).

use core::{ffi::c_void, mem::MaybeUninit};

use super::util::{
    child_add_ref, child_get_device, child_get_private_data, child_query_interface, child_release,
    child_set_private_data, child_set_private_data_interface, d11_to_d10_tex1d_desc,
    d11_to_d10_tex2d_desc, d11_to_d10_tex3d_desc, res_get_eviction_priority, res_get_type,
    res_set_eviction_priority, wrapper_context, wrapper_d3d11,
};
use crate::{
    abi::{
        records::{
            D3d10MappedTexture2d, D3d10MappedTexture3d, D3d10Texture1dDesc, D3d10Texture2dDesc,
            D3d10Texture3dDesc, D3d11MappedSubresource, D3d11Texture1dDesc, D3d11Texture2dDesc,
            D3d11Texture3dDesc,
        },
        vtbl::{
            ID3D10Texture1DVtbl, ID3D10Texture2DVtbl, ID3D10Texture3DVtbl, ID3D11DeviceContextVtbl,
            ID3D11Texture1DVtbl, ID3D11Texture2DVtbl, ID3D11Texture3DVtbl,
        },
    },
    com::S_OK,
};

unsafe extern "system-unwind" fn map_1d(
    this: *mut c_void,
    subresource: u32,
    map_type: u32,
    map_flags: u32,
    pp_data: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture1DVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture1DVtbl>`.
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
            subresource,
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

unsafe extern "system-unwind" fn map_2d(
    this: *mut c_void,
    subresource: u32,
    map_type: u32,
    map_flags: u32,
    p_mapped_tex2d: *mut c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture2DVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture2DVtbl>`.
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
            subresource,
            map_type,
            map_flags,
            subres.as_mut_ptr().cast::<c_void>(),
        )
    };
    if hr == S_OK {
        // SAFETY: `Map` succeeded and initialized `subres`.
        let mapped = unsafe { subres.assume_init() };
        // SAFETY: `p_mapped_tex2d` points to caller-allocated `D3d10MappedTexture2d`.
        unsafe {
            *p_mapped_tex2d.cast::<D3d10MappedTexture2d>() = D3d10MappedTexture2d {
                p_data: mapped.p_data,
                row_pitch: mapped.row_pitch,
            };
        }
    }
    hr
}

unsafe extern "system-unwind" fn map_3d(
    this: *mut c_void,
    subresource: u32,
    map_type: u32,
    map_flags: u32,
    p_mapped_tex3d: *mut c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture3DVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture3DVtbl>`.
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
            subresource,
            map_type,
            map_flags,
            subres.as_mut_ptr().cast::<c_void>(),
        )
    };
    if hr == S_OK {
        // SAFETY: `Map` succeeded and initialized `subres`.
        let mapped = unsafe { subres.assume_init() };
        // SAFETY: `p_mapped_tex3d` points to caller-allocated `D3d10MappedTexture3d`.
        unsafe {
            *p_mapped_tex3d.cast::<D3d10MappedTexture3d>() = D3d10MappedTexture3d {
                p_data: mapped.p_data,
                row_pitch: mapped.row_pitch,
                depth_pitch: mapped.depth_pitch,
            };
        }
    }
    hr
}

unsafe extern "system-unwind" fn unmap(this: *mut c_void, subresource: u32) {
    // SAFETY: `this` is a valid texture `Wrapper<V>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `this` is a valid texture `Wrapper<V>`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a valid `ID3D11DeviceContext` COM interface pointer.
    let vtbl_ptr = unsafe { *ctx.cast::<*const ID3D11DeviceContextVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live context vtable.
    let unmap_fn = unsafe { (*vtbl_ptr).unmap };
    // SAFETY: `ctx` and `d3d11` are valid COM pointers.
    unsafe { unmap_fn(ctx, d3d11, subresource) };
}

unsafe extern "system-unwind" fn get_desc_1d(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture1DVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11Texture1D` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11Texture1DVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live texture1d vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11Texture1dDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_tex1d_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10Texture1dDesc`.
    unsafe { *p_desc.cast::<D3d10Texture1dDesc>() = d10_desc };
}

unsafe extern "system-unwind" fn get_desc_2d(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture2DVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11Texture2D` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11Texture2DVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live texture2d vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11Texture2dDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_tex2d_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10Texture2dDesc`.
    unsafe { *p_desc.cast::<D3d10Texture2dDesc>() = d10_desc };
}

unsafe extern "system-unwind" fn get_desc_3d(this: *mut c_void, p_desc: *mut c_void) {
    // SAFETY: `this` is a valid `Wrapper<ID3D10Texture3DVtbl>`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a valid `ID3D11Texture3D` COM interface pointer.
    let vtbl_ptr = unsafe { *d3d11.cast::<*const ID3D11Texture3DVtbl>() };
    // SAFETY: `vtbl_ptr` points to the live texture3d vtable.
    let get_desc_fn = unsafe { (*vtbl_ptr).get_desc };
    let mut d11_desc = MaybeUninit::<D3d11Texture3dDesc>::zeroed();
    // SAFETY: `d3d11` is valid and `d11_desc` is writable.
    unsafe { get_desc_fn(d3d11, d11_desc.as_mut_ptr().cast::<c_void>()) };
    // SAFETY: `GetDesc` unconditionally initializes `d11_desc`.
    let d11_desc = unsafe { d11_desc.assume_init() };
    let d10_desc = d11_to_d10_tex3d_desc(&d11_desc);
    // SAFETY: `p_desc` points to caller-allocated `D3d10Texture3dDesc`.
    unsafe { *p_desc.cast::<D3d10Texture3dDesc>() = d10_desc };
}

/// `ID3D10Texture1D` vtable for embedded `MTLD3D10Texture1D` wrappers.
pub static TEXTURE1D_VTBL: ID3D10Texture1DVtbl = ID3D10Texture1DVtbl {
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
    map: map_1d,
    unmap,
    get_desc: get_desc_1d,
};

/// `ID3D10Texture2D` vtable for embedded `MTLD3D10Texture2D` wrappers.
pub static TEXTURE2D_VTBL: ID3D10Texture2DVtbl = ID3D10Texture2DVtbl {
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
    map: map_2d,
    unmap,
    get_desc: get_desc_2d,
};

/// `ID3D10Texture3D` vtable for embedded `MTLD3D10Texture3D` wrappers.
pub static TEXTURE3D_VTBL: ID3D10Texture3DVtbl = ID3D10Texture3DVtbl {
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
    map: map_3d,
    unmap,
    get_desc: get_desc_3d,
};
