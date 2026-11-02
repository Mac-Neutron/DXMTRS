//! D3D10 ↔ D3D11 descriptor and flag conversions (`d3d10_util.cpp`).

use core::{ffi::c_void, ptr};

use super::Wrapper;
use crate::{
    abi::{
        IID_ID3D10_DEVICE, IID_ID3D10_RESOURCE,
        records::{
            D3d10BlendDesc, D3d10BufferDesc, D3d10BufferRtv, D3d10BufferSrv,
            D3d10DepthStencilViewDesc, D3d10DepthStencilViewDescUnion, D3d10QueryDesc,
            D3d10RenderTargetViewDesc, D3d10RenderTargetViewDescUnion, D3d10SamplerDesc,
            D3d10ShaderResourceViewDesc, D3d10ShaderResourceViewDesc1,
            D3d10ShaderResourceViewDesc1Union, D3d10ShaderResourceViewDescUnion,
            D3d10Tex1dArrayDsv, D3d10Tex1dArrayRtv, D3d10Tex1dArraySrv, D3d10Tex1dDsv,
            D3d10Tex1dRtv, D3d10Tex1dSrv, D3d10Tex2dArrayDsv, D3d10Tex2dArrayRtv,
            D3d10Tex2dArraySrv, D3d10Tex2dDsv, D3d10Tex2dRtv, D3d10Tex2dSrv, D3d10Tex2dmsArrayDsv,
            D3d10Tex2dmsArrayRtv, D3d10Tex2dmsArraySrv, D3d10Tex2dmsDsv, D3d10Tex2dmsRtv,
            D3d10Tex2dmsSrv, D3d10Tex3dRtv, D3d10Tex3dSrv, D3d10TexcubeArraySrv1, D3d10TexcubeSrv,
            D3d10Texture1dDesc, D3d10Texture2dDesc, D3d10Texture3dDesc, D3d11BlendDesc,
            D3d11BufferDesc, D3d11DepthStencilViewDesc, D3d11DepthStencilViewDescUnion,
            D3d11QueryDesc, D3d11RenderTargetBlendDesc, D3d11RenderTargetViewDesc,
            D3d11SamplerDesc, D3d11ShaderResourceViewDesc, D3d11Tex1dArrayDsv, D3d11Tex1dDsv,
            D3d11Tex2dArrayDsv, D3d11Tex2dDsv, D3d11Tex2dmsArrayDsv, D3d11Tex2dmsDsv,
            D3d11Texture1dDesc, D3d11Texture2dDesc, D3d11Texture3dDesc,
        },
        vtbl::{ID3D11DeviceChildVtbl, ID3D11ResourceVtbl, ID3D11ViewVtbl},
    },
    com::{self, IUnknownVtbl},
};

/// `D3D10_RESOURCE_MISC_GENERATE_MIPS`.
pub const D3D10_RESOURCE_MISC_GENERATE_MIPS: u32 = 0x1;
/// `D3D10_RESOURCE_MISC_SHARED`.
pub const D3D10_RESOURCE_MISC_SHARED: u32 = 0x2;
/// `D3D10_RESOURCE_MISC_TEXTURECUBE`.
pub const D3D10_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;
/// `D3D10_RESOURCE_MISC_SHARED_KEYEDMUTEX`.
pub const D3D10_RESOURCE_MISC_SHARED_KEYEDMUTEX: u32 = 0x10;
/// `D3D10_RESOURCE_MISC_GDI_COMPATIBLE`.
pub const D3D10_RESOURCE_MISC_GDI_COMPATIBLE: u32 = 0x20;

/// `D3D11_RESOURCE_MISC_GENERATE_MIPS`.
pub const D3D11_RESOURCE_MISC_GENERATE_MIPS: u32 = 0x1;
/// `D3D11_RESOURCE_MISC_SHARED`.
pub const D3D11_RESOURCE_MISC_SHARED: u32 = 0x2;
/// `D3D11_RESOURCE_MISC_TEXTURECUBE`.
pub const D3D11_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;
/// `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`.
pub const D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX: u32 = 0x100;
/// `D3D11_RESOURCE_MISC_GDI_COMPATIBLE`.
pub const D3D11_RESOURCE_MISC_GDI_COMPATIBLE: u32 = 0x200;

/// Converts `D3D11_RESOURCE_MISC_FLAG` bits to `D3D10_RESOURCE_MISC_FLAG` (`d3d10_util.cpp:20-34`).
#[must_use]
pub const fn convert_d3d11_resource_flags(misc_flags: u32) -> u32 {
    let mut ret = 0;
    if (misc_flags & D3D11_RESOURCE_MISC_GENERATE_MIPS) != 0 {
        ret |= D3D10_RESOURCE_MISC_GENERATE_MIPS;
    }
    if (misc_flags & D3D11_RESOURCE_MISC_SHARED) != 0 {
        ret |= D3D10_RESOURCE_MISC_SHARED;
    }
    if (misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) != 0 {
        ret |= D3D10_RESOURCE_MISC_TEXTURECUBE;
    }
    if (misc_flags & D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX) != 0 {
        ret |= D3D10_RESOURCE_MISC_SHARED_KEYEDMUTEX;
    }
    if (misc_flags & D3D11_RESOURCE_MISC_GDI_COMPATIBLE) != 0 {
        ret |= D3D10_RESOURCE_MISC_GDI_COMPATIBLE;
    }
    ret
}

/// Converts `D3D10_RESOURCE_MISC_FLAG` bits to `D3D11_RESOURCE_MISC_FLAG` (`d3d10_util.cpp:36-50`).
#[must_use]
pub const fn convert_d3d10_resource_flags(misc_flags: u32) -> u32 {
    let mut ret = 0;
    if (misc_flags & D3D10_RESOURCE_MISC_GENERATE_MIPS) != 0 {
        ret |= D3D11_RESOURCE_MISC_GENERATE_MIPS;
    }
    if (misc_flags & D3D10_RESOURCE_MISC_SHARED) != 0 {
        ret |= D3D11_RESOURCE_MISC_SHARED;
    }
    if (misc_flags & D3D10_RESOURCE_MISC_TEXTURECUBE) != 0 {
        ret |= D3D11_RESOURCE_MISC_TEXTURECUBE;
    }
    if (misc_flags & D3D10_RESOURCE_MISC_SHARED_KEYEDMUTEX) != 0 {
        ret |= D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX;
    }
    if (misc_flags & D3D10_RESOURCE_MISC_GDI_COMPATIBLE) != 0 {
        ret |= D3D11_RESOURCE_MISC_GDI_COMPATIBLE;
    }
    ret
}

/// Queries the `ID3D10Device` for an `ID3D11DeviceChild` (`GetD3D10Device`, `d3d10_util.cpp:6-11`).
///
/// # Safety
///
/// `d3d11` must be a live `ID3D11DeviceChild` COM pointer, and `pp_device` must be a valid
/// writable out-pointer.
pub unsafe fn get_d3d10_device(d3d11: *mut c_void, pp_device: *mut *mut c_void) {
    let mut d3d11_device = ptr::null_mut();
    // SAFETY: `d3d11` is a live `ID3D11DeviceChild` COM object.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceChildVtbl>(d3d11) };
    // SAFETY: `d3d11` is live and `d3d11_device` is a local out-slot.
    unsafe { (vtbl.get_device)(d3d11, &raw mut d3d11_device) };
    if !d3d11_device.is_null() {
        // SAFETY: `d3d11_device` was returned by `GetDevice` with a live reference.
        let dev_vtbl = unsafe { com::vtbl::<IUnknownVtbl>(d3d11_device) };
        // SAFETY: `d3d11_device` is live and `pp_device` is the caller's out-slot.
        unsafe { (dev_vtbl.query_interface)(d3d11_device, &IID_ID3D10_DEVICE, pp_device) };
        // SAFETY: releases the temporary reference from `GetDevice`.
        unsafe { (dev_vtbl.release)(d3d11_device) };
    }
}

/// Queries the `ID3D10Resource` for an `ID3D11View` (`GetD3D10Resource`, `d3d10_util.cpp:13-18`).
///
/// # Safety
///
/// `d3d11` must be a live `ID3D11View` COM pointer, and `pp_resource` must be a valid writable
/// out-pointer.
pub unsafe fn get_d3d10_resource(d3d11: *mut c_void, pp_resource: *mut *mut c_void) {
    let mut d3d11_resource = ptr::null_mut();
    // SAFETY: `d3d11` is a live `ID3D11View` COM object.
    let vtbl = unsafe { com::vtbl::<ID3D11ViewVtbl>(d3d11) };
    // SAFETY: `d3d11` is live and `d3d11_resource` is a local out-slot.
    unsafe { (vtbl.get_resource)(d3d11, &raw mut d3d11_resource) };
    if !d3d11_resource.is_null() {
        // SAFETY: `d3d11_resource` was returned by `GetResource` with a live reference.
        let res_vtbl = unsafe { com::vtbl::<IUnknownVtbl>(d3d11_resource) };
        // SAFETY: `d3d11_resource` is live and `pp_resource` is the caller's out-slot.
        unsafe { (res_vtbl.query_interface)(d3d11_resource, &IID_ID3D10_RESOURCE, pp_resource) };
        // SAFETY: releases the temporary reference from `GetResource`.
        unsafe { (res_vtbl.release)(d3d11_resource) };
    }
}

/// Reads `d3d11` from a live `Wrapper<V>` COM `this` pointer.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
#[must_use]
pub const unsafe fn wrapper_d3d11(this: *mut c_void) -> *mut c_void {
    // SAFETY: `this` points to a live `Wrapper<V>`.
    unsafe { (*this.cast::<Wrapper<u8>>()).d3d11 }
}

/// Reads `context` from a live `Wrapper<V>` COM `this` pointer.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
#[must_use]
pub const unsafe fn wrapper_context(this: *mut c_void) -> *mut c_void {
    // SAFETY: `this` points to a live `Wrapper<V>`.
    unsafe { (*this.cast::<Wrapper<u8>>()).context }
}

/// Forwards `IUnknown::QueryInterface` from a D3D10 child wrapper to its D3D11 object.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn child_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live COM object.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceChildVtbl>(d3d11) };
    // SAFETY: forwards `QueryInterface` to the underlying D3D11 object.
    unsafe { (vtbl.query_interface)(d3d11, riid, ppv_object) }
}

/// Forwards `IUnknown::AddRef` from a D3D10 child wrapper to its D3D11 object.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn child_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live COM object.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceChildVtbl>(d3d11) };
    // SAFETY: forwards `AddRef` to the underlying D3D11 object.
    unsafe { (vtbl.add_ref)(d3d11) }
}

/// Forwards `IUnknown::Release` from a D3D10 child wrapper to its D3D11 object.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn child_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live COM object.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceChildVtbl>(d3d11) };
    // SAFETY: forwards `Release` to the underlying D3D11 object.
    unsafe { (vtbl.release)(d3d11) }
}

/// Implements `ID3D10DeviceChild::GetDevice` via [`get_d3d10_device`].
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`] and `pp_device` must be writable.
pub unsafe extern "system-unwind" fn child_get_device(
    this: *mut c_void,
    pp_device: *mut *mut c_void,
) {
    // SAFETY: `this` is a live `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11DeviceChild` and `pp_device` is the caller's out-slot.
    unsafe { get_d3d10_device(d3d11, pp_device) };
}

/// Forwards `ID3D10DeviceChild::GetPrivateData` to the underlying D3D11 object.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn child_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    p_data_size: *mut u32,
    p_data: *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11DeviceChild`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceChildVtbl>(d3d11) };
    // SAFETY: forwards `GetPrivateData` to the underlying D3D11 object.
    unsafe { (vtbl.get_private_data)(d3d11, guid, p_data_size, p_data) }
}

/// Forwards `ID3D10DeviceChild::SetPrivateData` to the underlying D3D11 object.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn child_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    p_data: *const c_void,
) -> i32 {
    // SAFETY: `this` is a live `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11DeviceChild`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceChildVtbl>(d3d11) };
    // SAFETY: forwards `SetPrivateData` to the underlying D3D11 object.
    unsafe { (vtbl.set_private_data)(d3d11, guid, data_size, p_data) }
}

/// Forwards `ID3D10DeviceChild::SetPrivateDataInterface` to the underlying D3D11 object.
///
/// # Safety
///
/// `this` must point to a live [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn child_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    p_data: *const c_void,
) -> i32 {
    // SAFETY: `this` is a live `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11DeviceChild`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceChildVtbl>(d3d11) };
    // SAFETY: forwards `SetPrivateDataInterface` to the underlying D3D11 object.
    unsafe { (vtbl.set_private_data_interface)(d3d11, guid, p_data) }
}

/// Forwards `ID3D10Resource::GetType` to the underlying `ID3D11Resource`.
///
/// # Safety
///
/// `this` must point to a live resource [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn res_get_type(this: *mut c_void, r_type: *mut u32) {
    // SAFETY: `this` is a live resource `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Resource`.
    let vtbl = unsafe { com::vtbl::<ID3D11ResourceVtbl>(d3d11) };
    // SAFETY: forwards `GetType` to the underlying D3D11 resource.
    unsafe { (vtbl.get_type)(d3d11, r_type) };
}

/// Forwards `ID3D10Resource::SetEvictionPriority` to the underlying `ID3D11Resource`.
///
/// # Safety
///
/// `this` must point to a live resource [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn res_set_eviction_priority(
    this: *mut c_void,
    eviction_priority: u32,
) {
    // SAFETY: `this` is a live resource `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Resource`.
    let vtbl = unsafe { com::vtbl::<ID3D11ResourceVtbl>(d3d11) };
    // SAFETY: forwards `SetEvictionPriority` to the underlying D3D11 resource.
    unsafe { (vtbl.set_eviction_priority)(d3d11, eviction_priority) };
}

/// Forwards `ID3D10Resource::GetEvictionPriority` to the underlying `ID3D11Resource`.
///
/// # Safety
///
/// `this` must point to a live resource [`Wrapper<V>`].
pub unsafe extern "system-unwind" fn res_get_eviction_priority(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live resource `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Resource`.
    let vtbl = unsafe { com::vtbl::<ID3D11ResourceVtbl>(d3d11) };
    // SAFETY: forwards `GetEvictionPriority` to the underlying D3D11 resource.
    unsafe { (vtbl.get_eviction_priority)(d3d11) }
}

/// Implements `ID3D10View::GetResource` via [`get_d3d10_resource`].
///
/// # Safety
///
/// `this` must point to a live view [`Wrapper<V>`] and `pp_resource` must be writable.
pub unsafe extern "system-unwind" fn view_get_resource(
    this: *mut c_void,
    pp_resource: *mut *mut c_void,
) {
    // SAFETY: `this` is a live view `Wrapper`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11View` and `pp_resource` is the caller's out-slot.
    unsafe { get_d3d10_resource(d3d11, pp_resource) };
}

/// Converts a `D3D10_BUFFER_DESC` into a `D3D11_BUFFER_DESC` (`d3d10_device.cpp:756-762`).
#[must_use]
pub const fn d10_to_d11_buffer_desc(desc: &D3d10BufferDesc) -> D3d11BufferDesc {
    D3d11BufferDesc {
        byte_width: desc.byte_width,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d10_resource_flags(desc.misc_flags),
        structure_byte_stride: 0,
    }
}

/// Converts a `D3D11_BUFFER_DESC` into a `D3D10_BUFFER_DESC` (`d3d10_buffer.cpp:20-29`).
#[must_use]
pub const fn d11_to_d10_buffer_desc(desc: &D3d11BufferDesc) -> D3d10BufferDesc {
    D3d10BufferDesc {
        byte_width: desc.byte_width,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d11_resource_flags(desc.misc_flags),
    }
}

/// Converts a `D3D10_TEXTURE1D_DESC` into a `D3D11_TEXTURE1D_DESC` (`d3d10_device.cpp:784-792`).
#[must_use]
pub const fn d10_to_d11_tex1d_desc(desc: &D3d10Texture1dDesc) -> D3d11Texture1dDesc {
    D3d11Texture1dDesc {
        width: desc.width,
        mip_levels: desc.mip_levels,
        array_size: desc.array_size,
        format: desc.format,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d10_resource_flags(desc.misc_flags),
    }
}

/// Converts a `D3D11_TEXTURE1D_DESC` into a `D3D10_TEXTURE1D_DESC` (`d3d10_texture.cpp:20-32`).
#[must_use]
pub const fn d11_to_d10_tex1d_desc(desc: &D3d11Texture1dDesc) -> D3d10Texture1dDesc {
    D3d10Texture1dDesc {
        width: desc.width,
        mip_levels: desc.mip_levels,
        array_size: desc.array_size,
        format: desc.format,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d11_resource_flags(desc.misc_flags),
    }
}

/// Converts a `D3D10_TEXTURE2D_DESC` into a `D3D11_TEXTURE2D_DESC` (`d3d10_device.cpp:815-825`).
#[must_use]
pub const fn d10_to_d11_tex2d_desc(desc: &D3d10Texture2dDesc) -> D3d11Texture2dDesc {
    D3d11Texture2dDesc {
        width: desc.width,
        height: desc.height,
        mip_levels: desc.mip_levels,
        array_size: desc.array_size,
        format: desc.format,
        sample_desc: desc.sample_desc,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d10_resource_flags(desc.misc_flags),
    }
}

/// Converts a `D3D11_TEXTURE2D_DESC` into a `D3D10_TEXTURE2D_DESC` (`d3d10_texture.cpp:51-65`).
#[must_use]
pub const fn d11_to_d10_tex2d_desc(desc: &D3d11Texture2dDesc) -> D3d10Texture2dDesc {
    D3d10Texture2dDesc {
        width: desc.width,
        height: desc.height,
        mip_levels: desc.mip_levels,
        array_size: desc.array_size,
        format: desc.format,
        sample_desc: desc.sample_desc,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d11_resource_flags(desc.misc_flags),
    }
}

/// Converts a `D3D10_TEXTURE3D_DESC` into a `D3D11_TEXTURE3D_DESC` (`d3d10_device.cpp:847-856`).
#[must_use]
pub const fn d10_to_d11_tex3d_desc(desc: &D3d10Texture3dDesc) -> D3d11Texture3dDesc {
    D3d11Texture3dDesc {
        width: desc.width,
        height: desc.height,
        depth: desc.depth,
        mip_levels: desc.mip_levels,
        format: desc.format,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d10_resource_flags(desc.misc_flags),
    }
}

/// Converts a `D3D11_TEXTURE3D_DESC` into a `D3D10_TEXTURE3D_DESC` (`d3d10_texture.cpp:85-98`).
#[must_use]
pub const fn d11_to_d10_tex3d_desc(desc: &D3d11Texture3dDesc) -> D3d10Texture3dDesc {
    D3d10Texture3dDesc {
        width: desc.width,
        height: desc.height,
        depth: desc.depth,
        mip_levels: desc.mip_levels,
        format: desc.format,
        usage: desc.usage,
        bind_flags: desc.bind_flags,
        cpu_access_flags: desc.cpu_access_flags,
        misc_flags: convert_d3d11_resource_flags(desc.misc_flags),
    }
}

/// Converts a D3D11 SRV descriptor into a D3D10 SRV descriptor (`d3d10_view.cpp:6-56`).
#[must_use]
pub const fn d11_to_d10_srv_desc(
    desc: &D3d11ShaderResourceViewDesc,
) -> D3d10ShaderResourceViewDesc {
    let mut view_dimension = desc.view_dimension;
    let mut u = D3d10ShaderResourceViewDescUnion {
        texture1d_array: D3d10Tex1dArraySrv {
            most_detailed_mip: 0,
            mip_levels: 0,
            first_array_slice: 0,
            array_size: 0,
        },
    };
    match desc.view_dimension {
        1 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_BUFFER`.
            let src = unsafe { desc.u.buffer };
            u.buffer = D3d10BufferSrv {
                first_element: src.first_element,
                num_elements: src.num_elements,
            };
        }
        2 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE1D`.
            let src = unsafe { desc.u.texture1d };
            u.texture1d = D3d10Tex1dSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        3 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE1DARRAY`.
            let src = unsafe { desc.u.texture1d_array };
            u.texture1d_array = D3d10Tex1dArraySrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        4 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2D`.
            let src = unsafe { desc.u.texture2d };
            u.texture2d = D3d10Tex2dSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        5 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2DARRAY`.
            let src = unsafe { desc.u.texture2d_array };
            u.texture2d_array = D3d10Tex2dArraySrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        6 => {
            u.texture2dms = D3d10Tex2dmsSrv {
                unused_field_nothing_to_define: 0,
            };
        }
        7 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2DMSARRAY`.
            let src = unsafe { desc.u.texture2dms_array };
            u.texture2dms_array = D3d10Tex2dmsArraySrv {
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        8 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE3D`.
            let src = unsafe { desc.u.texture3d };
            u.texture3d = D3d10Tex3dSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        9 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURECUBE`.
            let src = unsafe { desc.u.texture_cube };
            u.texture_cube = D3d10TexcubeSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        _ => {
            view_dimension = 0;
        }
    }
    D3d10ShaderResourceViewDesc {
        format: desc.format,
        view_dimension,
        u,
    }
}

/// Converts a D3D11 SRV descriptor into a D3D10.1 SRV descriptor (`d3d10_view.cpp:59-115`).
#[must_use]
pub const fn d11_to_d10_srv_desc1(
    desc: &D3d11ShaderResourceViewDesc,
) -> D3d10ShaderResourceViewDesc1 {
    let mut view_dimension = desc.view_dimension;
    let mut u = D3d10ShaderResourceViewDesc1Union {
        texture_cube_array: D3d10TexcubeArraySrv1 {
            most_detailed_mip: 0,
            mip_levels: 0,
            first2d_array_face: 0,
            num_cubes: 0,
        },
    };
    match desc.view_dimension {
        1 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_BUFFER`.
            let src = unsafe { desc.u.buffer };
            u.buffer = D3d10BufferSrv {
                first_element: src.first_element,
                num_elements: src.num_elements,
            };
        }
        2 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE1D`.
            let src = unsafe { desc.u.texture1d };
            u.texture1d = D3d10Tex1dSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        3 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE1DARRAY`.
            let src = unsafe { desc.u.texture1d_array };
            u.texture1d_array = D3d10Tex1dArraySrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        4 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2D`.
            let src = unsafe { desc.u.texture2d };
            u.texture2d = D3d10Tex2dSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        5 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2DARRAY`.
            let src = unsafe { desc.u.texture2d_array };
            u.texture2d_array = D3d10Tex2dArraySrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        6 => {
            u.texture2dms = D3d10Tex2dmsSrv {
                unused_field_nothing_to_define: 0,
            };
        }
        7 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2DMSARRAY`.
            let src = unsafe { desc.u.texture2dms_array };
            u.texture2dms_array = D3d10Tex2dmsArraySrv {
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        8 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE3D`.
            let src = unsafe { desc.u.texture3d };
            u.texture3d = D3d10Tex3dSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        9 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURECUBE`.
            let src = unsafe { desc.u.texture_cube };
            u.texture_cube = D3d10TexcubeSrv {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
            };
        }
        10 => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURECUBEARRAY`.
            let src = unsafe { desc.u.texture_cube_array };
            u.texture_cube_array = D3d10TexcubeArraySrv1 {
                most_detailed_mip: src.most_detailed_mip,
                mip_levels: src.mip_levels,
                first2d_array_face: src.first2d_array_face,
                num_cubes: src.num_cubes,
            };
        }
        _ => {
            view_dimension = 0;
        }
    }
    D3d10ShaderResourceViewDesc1 {
        format: desc.format,
        view_dimension,
        u,
    }
}

/// Converts a D3D11 RTV descriptor into a D3D10 RTV descriptor (`d3d10_view.cpp:118-161`).
#[must_use]
pub const fn d11_to_d10_rtv_desc(desc: &D3d11RenderTargetViewDesc) -> D3d10RenderTargetViewDesc {
    let mut view_dimension = desc.view_dimension;
    let mut u = D3d10RenderTargetViewDescUnion {
        texture1d_array: D3d10Tex1dArrayRtv {
            mip_slice: 0,
            first_array_slice: 0,
            array_size: 0,
        },
    };
    match desc.view_dimension {
        1 => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_BUFFER`.
            let src = unsafe { desc.u.buffer };
            u.buffer = D3d10BufferRtv {
                first_element: src.first_element,
                num_elements: src.num_elements,
            };
        }
        2 => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE1D`.
            let src = unsafe { desc.u.texture1d };
            u.texture1d = D3d10Tex1dRtv {
                mip_slice: src.mip_slice,
            };
        }
        3 => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE1DARRAY`.
            let src = unsafe { desc.u.texture1d_array };
            u.texture1d_array = D3d10Tex1dArrayRtv {
                mip_slice: src.mip_slice,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        4 => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE2D`.
            let src = unsafe { desc.u.texture2d };
            u.texture2d = D3d10Tex2dRtv {
                mip_slice: src.mip_slice,
            };
        }
        5 => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE2DARRAY`.
            let src = unsafe { desc.u.texture2d_array };
            u.texture2d_array = D3d10Tex2dArrayRtv {
                mip_slice: src.mip_slice,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        6 => {
            u.texture2dms = D3d10Tex2dmsRtv {
                unused_field_nothing_to_define: 0,
            };
        }
        7 => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE2DMSARRAY`.
            let src = unsafe { desc.u.texture2dms_array };
            u.texture2dms_array = D3d10Tex2dmsArrayRtv {
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        8 => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE3D`.
            let src = unsafe { desc.u.texture3d };
            u.texture3d = D3d10Tex3dRtv {
                mip_slice: src.mip_slice,
                first_w_slice: src.first_w_slice,
                w_size: src.w_size,
            };
        }
        _ => {
            view_dimension = 0;
        }
    }
    D3d10RenderTargetViewDesc {
        format: desc.format,
        view_dimension,
        u,
    }
}

/// Converts a D3D10 DSV descriptor into a D3D11 DSV descriptor (`d3d10_device.cpp:924-961`).
#[must_use]
pub const fn d10_to_d11_dsv_desc(desc: &D3d10DepthStencilViewDesc) -> D3d11DepthStencilViewDesc {
    let mut u = D3d11DepthStencilViewDescUnion {
        texture1d_array: D3d11Tex1dArrayDsv {
            mip_slice: 0,
            first_array_slice: 0,
            array_size: 0,
        },
    };
    match desc.view_dimension {
        1 => {
            // SAFETY: union variant matches `D3D10_DSV_DIMENSION_TEXTURE1D`.
            let src = unsafe { desc.u.texture1d };
            u.texture1d = D3d11Tex1dDsv {
                mip_slice: src.mip_slice,
            };
        }
        2 => {
            // SAFETY: union variant matches `D3D10_DSV_DIMENSION_TEXTURE1DARRAY`.
            let src = unsafe { desc.u.texture1d_array };
            u.texture1d_array = D3d11Tex1dArrayDsv {
                mip_slice: src.mip_slice,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        3 => {
            // SAFETY: union variant matches `D3D10_DSV_DIMENSION_TEXTURE2D`.
            let src = unsafe { desc.u.texture2d };
            u.texture2d = D3d11Tex2dDsv {
                mip_slice: src.mip_slice,
            };
        }
        4 => {
            // SAFETY: union variant matches `D3D10_DSV_DIMENSION_TEXTURE2DARRAY`.
            let src = unsafe { desc.u.texture2d_array };
            u.texture2d_array = D3d11Tex2dArrayDsv {
                mip_slice: src.mip_slice,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        5 => {
            u.texture2dms = D3d11Tex2dmsDsv {
                unused_field_nothing_to_define: 0,
            };
        }
        6 => {
            // SAFETY: union variant matches `D3D10_DSV_DIMENSION_TEXTURE2DMSARRAY`.
            let src = unsafe { desc.u.texture2dms_array };
            u.texture2dms_array = D3d11Tex2dmsArrayDsv {
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        _ => {}
    }
    D3d11DepthStencilViewDesc {
        format: desc.format,
        view_dimension: desc.view_dimension,
        flags: 0,
        u,
    }
}

/// Converts a D3D11 DSV descriptor into a D3D10 DSV descriptor (`d3d10_view.cpp:164-198`).
#[must_use]
pub const fn d11_to_d10_dsv_desc(desc: &D3d11DepthStencilViewDesc) -> D3d10DepthStencilViewDesc {
    let mut view_dimension = desc.view_dimension;
    let mut u = D3d10DepthStencilViewDescUnion {
        texture1d_array: D3d10Tex1dArrayDsv {
            mip_slice: 0,
            first_array_slice: 0,
            array_size: 0,
        },
    };
    match desc.view_dimension {
        1 => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE1D`.
            let src = unsafe { desc.u.texture1d };
            u.texture1d = D3d10Tex1dDsv {
                mip_slice: src.mip_slice,
            };
        }
        2 => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE1DARRAY`.
            let src = unsafe { desc.u.texture1d_array };
            u.texture1d_array = D3d10Tex1dArrayDsv {
                mip_slice: src.mip_slice,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        3 => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE2D`.
            let src = unsafe { desc.u.texture2d };
            u.texture2d = D3d10Tex2dDsv {
                mip_slice: src.mip_slice,
            };
        }
        4 => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE2DARRAY`.
            let src = unsafe { desc.u.texture2d_array };
            u.texture2d_array = D3d10Tex2dArrayDsv {
                mip_slice: src.mip_slice,
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        5 => {
            u.texture2dms = D3d10Tex2dmsDsv {
                unused_field_nothing_to_define: 0,
            };
        }
        6 => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE2DMSARRAY`.
            let src = unsafe { desc.u.texture2dms_array };
            u.texture2dms_array = D3d10Tex2dmsArrayDsv {
                first_array_slice: src.first_array_slice,
                array_size: src.array_size,
            };
        }
        _ => {
            view_dimension = 0;
        }
    }
    D3d10DepthStencilViewDesc {
        format: desc.format,
        view_dimension,
        u,
    }
}

/// Converts a `D3D10_BLEND_DESC` into a `D3D11_BLEND_DESC` (`d3d10_device.cpp:1088-1102`).
#[must_use]
pub const fn d10_to_d11_blend_desc(desc: &D3d10BlendDesc) -> D3d11BlendDesc {
    let mut render_target = [D3d11RenderTargetBlendDesc {
        blend_enable: 0,
        src_blend: 0,
        dest_blend: 0,
        blend_op: 0,
        src_blend_alpha: 0,
        dest_blend_alpha: 0,
        blend_op_alpha: 0,
        render_target_write_mask: 0,
    }; 8];
    let mut i = 0;
    while i < 8 {
        render_target[i] = D3d11RenderTargetBlendDesc {
            blend_enable: desc.blend_enable[i],
            src_blend: desc.src_blend,
            dest_blend: desc.dest_blend,
            blend_op: desc.blend_op,
            src_blend_alpha: desc.src_blend_alpha,
            dest_blend_alpha: desc.dest_blend_alpha,
            blend_op_alpha: desc.blend_op_alpha,
            render_target_write_mask: desc.render_target_write_mask[i],
        };
        i += 1;
    }
    D3d11BlendDesc {
        alpha_to_coverage_enable: desc.alpha_to_coverage_enable,
        independent_blend_enable: 1,
        render_target,
    }
}

/// Converts a `D3D11_BLEND_DESC` into a `D3D10_BLEND_DESC` (`d3d10_state_object.cpp:6-23`).
#[must_use]
pub const fn d11_to_d10_blend_desc(desc: &D3d11BlendDesc) -> D3d10BlendDesc {
    let mut blend_enable = [0; 8];
    let mut render_target_write_mask = [0; 8];
    let mut i = 0;
    while i < 8 {
        let id = if desc.independent_blend_enable != 0 {
            i
        } else {
            0
        };
        blend_enable[i] = desc.render_target[id].blend_enable;
        render_target_write_mask[i] = desc.render_target[id].render_target_write_mask;
        i += 1;
    }
    D3d10BlendDesc {
        alpha_to_coverage_enable: desc.alpha_to_coverage_enable,
        blend_enable,
        src_blend: desc.render_target[0].src_blend,
        dest_blend: desc.render_target[0].dest_blend,
        blend_op: desc.render_target[0].blend_op,
        src_blend_alpha: desc.render_target[0].src_blend_alpha,
        dest_blend_alpha: desc.render_target[0].dest_blend_alpha,
        blend_op_alpha: desc.render_target[0].blend_op_alpha,
        render_target_write_mask,
    }
}

/// Converts a `D3D10_SAMPLER_DESC` into a `D3D11_SAMPLER_DESC` (`d3d10_device.cpp:1155-1168`).
#[must_use]
pub const fn d10_to_d11_sampler_desc(desc: &D3d10SamplerDesc) -> D3d11SamplerDesc {
    D3d11SamplerDesc {
        filter: desc.filter,
        address_u: desc.address_u,
        address_v: desc.address_v,
        address_w: desc.address_w,
        mip_lod_bias: desc.mip_lod_bias,
        max_anisotropy: desc.max_anisotropy,
        comparison_func: desc.comparison_func,
        border_color: desc.border_color,
        min_lod: desc.min_lod,
        max_lod: desc.max_lod,
    }
}

/// Converts a `D3D11_SAMPLER_DESC` into a `D3D10_SAMPLER_DESC` (`d3d10_state_object.cpp:38-54`).
#[must_use]
pub const fn d11_to_d10_sampler_desc(desc: &D3d11SamplerDesc) -> D3d10SamplerDesc {
    D3d10SamplerDesc {
        filter: desc.filter,
        address_u: desc.address_u,
        address_v: desc.address_v,
        address_w: desc.address_w,
        mip_lod_bias: desc.mip_lod_bias,
        max_anisotropy: desc.max_anisotropy,
        comparison_func: desc.comparison_func,
        border_color: desc.border_color,
        min_lod: desc.min_lod,
        max_lod: desc.max_lod,
    }
}

/// Converts a `D3D10_QUERY_DESC` into a `D3D11_QUERY_DESC` (`d3d10_device.cpp:1185-1187`).
#[must_use]
pub const fn d10_to_d11_query_desc(desc: &D3d10QueryDesc) -> D3d11QueryDesc {
    D3d11QueryDesc {
        query: desc.query,
        misc_flags: desc.misc_flags,
    }
}

/// Converts a `D3D11_QUERY_DESC` into a `D3D10_QUERY_DESC` (`d3d10_query.hpp:71-77`).
#[must_use]
pub const fn d11_to_d10_query_desc(desc: &D3d11QueryDesc) -> D3d10QueryDesc {
    D3d10QueryDesc {
        query: desc.query,
        misc_flags: desc.misc_flags,
    }
}
