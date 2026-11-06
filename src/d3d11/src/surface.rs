//! `IDXGISurface2` implementation and standalone subresource surface (`d3d11_surface.cpp`).

use core::ffi::c_void;

use dxmt_util::{
    com::{Qi, QiMiss, hresult::DXGI_ERROR_INVALID_CALL, qi_table},
    dxgi_consts::{DXGI_FORMAT_UNKNOWN, DXGI_MAP_DISCARD, DXGI_MAP_READ, DXGI_MAP_WRITE},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
};

use crate::{
    abi::{
        IID_IDXGI_DEVICE_SUB_OBJECT, IID_IDXGI_OBJECT, IID_IDXGI_SURFACE, IID_IDXGI_SURFACE_1,
        IID_IDXGI_SURFACE_2, IID_IUNKNOWN,
        records::{D3d11MappedSubresource, DxgiMappedRect, DxgiSampleDesc, DxgiSurfaceDesc},
        vtbl::{
            ID3D11DeviceContextVtbl, ID3D11DeviceVtbl, ID3D11ResourceVtbl, IDXGISurface2Vtbl,
            IUnknownVtbl,
        },
    },
    com::{
        ComObjectWithInitialRef, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Hresult, S_OK, TearOff,
        add_ref, release, vtbl,
    },
    resource::{Resource, ResourceDesc, ResourceTearOff},
};

const D3D11_MAP_READ: u32 = 1;
const D3D11_MAP_WRITE: u32 = 2;
const D3D11_MAP_READ_WRITE: u32 = 3;
const D3D11_MAP_WRITE_DISCARD: u32 = 4;

/// Standalone `IDXGISurface2` created by `IDXGIResource1::CreateSubresourceSurface`.
#[repr(C)]
pub struct SubresourceSurface {
    pub vtbl: &'static IDXGISurface2Vtbl,
    pub refs: ComObjectWithInitialRef,
    pub resource: *mut c_void,
    pub subresource: u32,
}

impl Drop for SubresourceSurface {
    fn drop(&mut self) {
        // SAFETY: `self.resource` was retained in `create_subresource_surface`.
        unsafe { release(self.resource) };
    }
}

fn create_subresource_surface_inner(
    resource: *mut c_void,
    index: u32,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `out` was just checked to be non-null.
    unsafe { *out = core::ptr::null_mut() };
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a live `ID3D11Resource` COM pointer.
    unsafe { add_ref(resource) };
    let surface = Box::new(SubresourceSurface {
        vtbl: &SUBRESOURCE_SURFACE_VTBL,
        refs: ComObjectWithInitialRef::new(),
        resource,
        subresource: index,
    });
    // SAFETY: `out` is non-null and writable.
    unsafe { *out = Box::into_raw(surface).cast::<c_void>() };
    S_OK
}

/// Creates a standalone subresource surface for `resource` at `index` (`CreateSubresourceSurface`).
#[must_use]
pub fn create_subresource_surface(
    resource: *mut c_void,
    index: u32,
    out: *mut *mut c_void,
) -> Hresult {
    create_subresource_surface_inner(resource, index, out)
}

unsafe fn resource_get_device(
    resource: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` is a live `ID3D11Resource`.
    let rv = unsafe { vtbl::<ID3D11ResourceVtbl>(resource) };
    let mut device: *mut c_void = core::ptr::null_mut();
    // SAFETY: retrieves the owning `ID3D11Device*`.
    unsafe { (rv.get_device)(resource, &raw mut device) };
    if device.is_null() {
        return E_NOINTERFACE;
    }
    // SAFETY: `device` is a live COM object.
    let unk = unsafe { vtbl::<IUnknownVtbl>(device) };
    // SAFETY: queries `device` for `riid`.
    let hr = unsafe { (unk.query_interface)(device, riid, out) };
    // SAFETY: balances `get_device`'s `AddRef`.
    unsafe { release(device) };
    hr
}

const fn shifted_dim(base: u32, mip: u32) -> u32 {
    match base.checked_shr(mip) {
        Some(v) if v > 1 => v,
        _ => 1,
    }
}

unsafe fn surface_get_desc(
    resource: *mut c_void,
    subresource: u32,
    from_subresource: bool,
    desc_ptr: *mut c_void,
) -> Hresult {
    if desc_ptr.is_null() || resource.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `resource` is a live `Resource` pointer.
    let res = unsafe { Resource::from_ptr(resource) };
    let surface_desc = if from_subresource {
        match res.desc {
            ResourceDesc::Texture1d(d) => {
                let mip = subresource % d.mip_levels.max(1);
                DxgiSurfaceDesc {
                    width: shifted_dim(d.width, mip),
                    height: 1,
                    format: d.format,
                    sample_desc: DxgiSampleDesc {
                        count: 1,
                        quality: 0,
                    },
                }
            }
            ResourceDesc::Texture2d(d) => {
                let mip = subresource % d.mip_levels.max(1);
                DxgiSurfaceDesc {
                    width: shifted_dim(d.width, mip),
                    height: shifted_dim(d.height, mip),
                    format: d.format,
                    sample_desc: d.sample_desc,
                }
            }
            ResourceDesc::Texture3d(d) => {
                let mip = subresource % d.mip_levels.max(1);
                DxgiSurfaceDesc {
                    width: shifted_dim(d.width, mip),
                    height: shifted_dim(d.height, mip),
                    format: d.format,
                    sample_desc: DxgiSampleDesc {
                        count: 1,
                        quality: 0,
                    },
                }
            }
            ResourceDesc::Buffer(d) => DxgiSurfaceDesc {
                width: d.byte_width,
                height: 1,
                format: DXGI_FORMAT_UNKNOWN,
                sample_desc: DxgiSampleDesc {
                    count: 1,
                    quality: 0,
                },
            },
        }
    } else {
        if res.common.texture.is_none()
            && res.staging_subresources.is_empty()
            && res.common.dynamic_texture.is_none()
            && res.dynamic_subresources.is_empty()
        {
            return DXGI_ERROR_INVALID_CALL;
        }
        match res.desc {
            ResourceDesc::Texture1d(d) => DxgiSurfaceDesc {
                width: d.width,
                height: 1,
                format: d.format,
                sample_desc: DxgiSampleDesc {
                    count: 1,
                    quality: 0,
                },
            },
            ResourceDesc::Texture2d(d) => DxgiSurfaceDesc {
                width: d.width,
                height: d.height,
                format: d.format,
                sample_desc: d.sample_desc,
            },
            ResourceDesc::Texture3d(d) => DxgiSurfaceDesc {
                width: d.width,
                height: d.height,
                format: d.format,
                sample_desc: DxgiSampleDesc {
                    count: 1,
                    quality: 0,
                },
            },
            ResourceDesc::Buffer(_) => return DXGI_ERROR_INVALID_CALL,
        }
    };
    // SAFETY: `desc_ptr` is non-null and addresses a writable `DxgiSurfaceDesc`.
    unsafe { *desc_ptr.cast::<DxgiSurfaceDesc>() = surface_desc };
    S_OK
}

unsafe fn surface_map(
    resource: *mut c_void,
    subresource: u32,
    locked_rect: *mut c_void,
    map_flags: u32,
) -> Hresult {
    let rect_ptr = locked_rect.cast::<DxgiMappedRect>();
    if !rect_ptr.is_null() {
        // SAFETY: `rect_ptr` is non-null and writable.
        unsafe {
            *rect_ptr = DxgiMappedRect {
                pitch: 0,
                p_bits: core::ptr::null_mut(),
            };
        }
    }

    let map_type = if (map_flags & DXGI_MAP_READ) != 0 && (map_flags & DXGI_MAP_WRITE) != 0 {
        D3D11_MAP_READ_WRITE
    } else if (map_flags & DXGI_MAP_READ) != 0 {
        D3D11_MAP_READ
    } else if (map_flags & DXGI_MAP_WRITE) != 0 && (map_flags & DXGI_MAP_DISCARD) != 0 {
        D3D11_MAP_WRITE_DISCARD
    } else if (map_flags & DXGI_MAP_WRITE) != 0 {
        D3D11_MAP_WRITE
    } else {
        return DXGI_ERROR_INVALID_CALL;
    };

    // SAFETY: `resource` is a live `ID3D11Resource`.
    let rv = unsafe { vtbl::<ID3D11ResourceVtbl>(resource) };
    let mut device: *mut c_void = core::ptr::null_mut();
    // SAFETY: retrieves the parent device.
    unsafe { (rv.get_device)(resource, &raw mut device) };
    if device.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `device` is a live `ID3D11Device`.
    let dv = unsafe { vtbl::<ID3D11DeviceVtbl>(device) };
    let mut context: *mut c_void = core::ptr::null_mut();
    // SAFETY: retrieves the immediate context.
    unsafe { (dv.get_immediate_context)(device, &raw mut context) };
    // SAFETY: releases the temporary device reference.
    unsafe { release(device) };
    if context.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }

    let mut sr = D3d11MappedSubresource {
        p_data: core::ptr::null_mut(),
        row_pitch: 0,
        depth_pitch: 0,
    };
    let sr_ptr = if rect_ptr.is_null() {
        core::ptr::null_mut()
    } else {
        (&raw mut sr).cast::<c_void>()
    };
    // SAFETY: `context` is a live `ID3D11DeviceContext`.
    let cv = unsafe { vtbl::<ID3D11DeviceContextVtbl>(context) };
    // SAFETY: maps `subresource` on the immediate context.
    let hr = unsafe { (cv.map)(context, resource, subresource, map_type, 0, sr_ptr) };
    // SAFETY: releases the temporary context reference.
    unsafe { release(context) };

    if hr != S_OK {
        return hr;
    }
    if !rect_ptr.is_null() {
        // SAFETY: `rect_ptr` is non-null and writable.
        unsafe {
            *rect_ptr = DxgiMappedRect {
                pitch: sr.row_pitch.cast_signed(),
                p_bits: sr.p_data.cast::<u8>(),
            };
        }
    }
    S_OK
}

unsafe fn surface_unmap(resource: *mut c_void, subresource: u32) -> Hresult {
    // SAFETY: `resource` is a live `ID3D11Resource`.
    let rv = unsafe { vtbl::<ID3D11ResourceVtbl>(resource) };
    let mut device: *mut c_void = core::ptr::null_mut();
    // SAFETY: retrieves the parent device.
    unsafe { (rv.get_device)(resource, &raw mut device) };
    if device.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `device` is a live `ID3D11Device`.
    let dv = unsafe { vtbl::<ID3D11DeviceVtbl>(device) };
    let mut context: *mut c_void = core::ptr::null_mut();
    // SAFETY: retrieves the immediate context.
    unsafe { (dv.get_immediate_context)(device, &raw mut context) };
    // SAFETY: releases the temporary device reference.
    unsafe { release(device) };
    if context.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `context` is a live `ID3D11DeviceContext`.
    let cv = unsafe { vtbl::<ID3D11DeviceContextVtbl>(context) };
    // SAFETY: unmaps `subresource` on the immediate context.
    unsafe { (cv.unmap)(context, resource, subresource) };
    // SAFETY: releases the temporary context reference.
    unsafe { release(context) };
    S_OK
}

unsafe fn surface_get_resource(
    resource: *mut c_void,
    subresource: u32,
    riid: *const c_void,
    parent_resource: *mut *mut c_void,
    subresource_index: *mut u32,
) -> Hresult {
    if parent_resource.is_null() {
        return E_POINTER;
    }
    // SAFETY: `parent_resource` was checked non-null.
    unsafe { *parent_resource = core::ptr::null_mut() };
    // SAFETY: `resource` is a live COM object.
    let unk = unsafe { vtbl::<IUnknownVtbl>(resource) };
    // SAFETY: queries `resource` for `riid`.
    let hr = unsafe { (unk.query_interface)(resource, riid, parent_resource) };
    if hr >= 0 && !subresource_index.is_null() {
        // SAFETY: `subresource_index` was checked non-null.
        unsafe { *subresource_index = subresource };
    }
    hr
}

// --- Tear-off thunks (`Resource::dxgi_surface`) ---

unsafe extern "system-unwind" fn tearoff_get_device(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a `TearOff<IDXGISurface2Vtbl>`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGISurface2Vtbl>>()).outer };
    // SAFETY: forwards to `outer`.
    unsafe { resource_get_device(outer, riid, out) }
}

unsafe extern "system-unwind" fn tearoff_get_desc(this: *mut c_void, desc: *mut c_void) -> Hresult {
    // SAFETY: `this` is a `TearOff<IDXGISurface2Vtbl>`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGISurface2Vtbl>>()).outer };
    // SAFETY: forwards to `outer` at subresource 0.
    unsafe { surface_get_desc(outer, 0, false, desc) }
}

unsafe extern "system-unwind" fn tearoff_map(
    this: *mut c_void,
    locked_rect: *mut c_void,
    map_flags: u32,
) -> Hresult {
    // SAFETY: `this` is a `TearOff<IDXGISurface2Vtbl>`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGISurface2Vtbl>>()).outer };
    // SAFETY: maps subresource 0 of `outer`.
    unsafe { surface_map(outer, 0, locked_rect, map_flags) }
}

unsafe extern "system-unwind" fn tearoff_unmap(this: *mut c_void) -> Hresult {
    // SAFETY: `this` is a `TearOff<IDXGISurface2Vtbl>`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGISurface2Vtbl>>()).outer };
    // SAFETY: unmaps subresource 0 of `outer`.
    unsafe { surface_unmap(outer, 0) }
}

const unsafe extern "system-unwind" fn surface_get_dc(
    _this: *mut c_void,
    _discard: i32,
    _hdc: *mut *mut c_void,
) -> Hresult {
    DXGI_ERROR_INVALID_CALL
}

const unsafe extern "system-unwind" fn surface_release_dc(
    _this: *mut c_void,
    _dirty_rect: *mut c_void,
) -> Hresult {
    DXGI_ERROR_INVALID_CALL
}

unsafe extern "system-unwind" fn tearoff_get_resource(
    this: *mut c_void,
    riid: *const c_void,
    parent_resource: *mut *mut c_void,
    subresource_index: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a `TearOff<IDXGISurface2Vtbl>`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGISurface2Vtbl>>()).outer };
    // SAFETY: forwards to `outer` at subresource 0.
    unsafe { surface_get_resource(outer, 0, riid, parent_resource, subresource_index) }
}

/// `IDXGISurface2` vtable for the embedded `Resource::dxgi_surface` tear-off.
pub static SURFACE_VTBL: IDXGISurface2Vtbl = IDXGISurface2Vtbl {
    query_interface: TearOff::query_interface,
    add_ref: TearOff::add_ref,
    release: TearOff::release,
    set_private_data: set_private_data::<ResourceTearOff>,
    set_private_data_interface: set_private_data_interface::<ResourceTearOff>,
    get_private_data: get_private_data::<ResourceTearOff>,
    get_parent: tearoff_get_device,
    get_device: tearoff_get_device,
    get_desc: tearoff_get_desc,
    map: tearoff_map,
    unmap: tearoff_unmap,
    get_dc: surface_get_dc,
    release_dc: surface_release_dc,
    get_resource: tearoff_get_resource,
};

// --- Standalone `SubresourceSurface` thunks ---

unsafe extern "system-unwind" fn sub_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let take_ref = || {
        // SAFETY: `this` is a live `SubresourceSurface`.
        unsafe { sub_add_ref(this) };
    };
    let qi = Qi {
        null_riid: E_NOINTERFACE,
        silent: &[],
        log: &crate::LOG,
        miss: QiMiss::Quiet,
    };
    // SAFETY: the application's `riid` and `out`, null or valid.
    unsafe {
        qi_table(
            riid,
            out,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_IDXGI_OBJECT,
                    IID_IDXGI_DEVICE_SUB_OBJECT,
                    IID_IDXGI_SURFACE,
                    IID_IDXGI_SURFACE_1,
                    IID_IDXGI_SURFACE_2,
                ],
                this,
            )],
            take_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn sub_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `SubresourceSurface`.
    let s = unsafe { &*this.cast::<SubresourceSurface>() };
    s.refs.add_ref()
}

unsafe extern "system-unwind" fn sub_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `SubresourceSurface`.
    let (rem, free) = unsafe { (*this.cast::<SubresourceSurface>()).refs.release_and_free() };
    if free {
        // SAFETY: last reference was released; reclaim `Box<SubresourceSurface>`.
        drop(unsafe { Box::from_raw(this.cast::<SubresourceSurface>()) });
    }
    rem
}

impl HasPrivateData for SubresourceSurface {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `SubresourceSurface`.
        let resource = unsafe { (*this.cast::<Self>()).resource };
        // SAFETY: `resource` is a live `Resource`.
        &unsafe { Resource::from_ptr(resource) }.private_data
    }
}

unsafe extern "system-unwind" fn sub_get_device(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SubresourceSurface`.
    let res = unsafe { (*this.cast::<SubresourceSurface>()).resource };
    // SAFETY: forwards to `res`.
    unsafe { resource_get_device(res, riid, out) }
}

unsafe extern "system-unwind" fn sub_get_desc(this: *mut c_void, desc: *mut c_void) -> Hresult {
    // SAFETY: `this` is a live `SubresourceSurface`.
    let s = unsafe { &*this.cast::<SubresourceSurface>() };
    // SAFETY: forwards to `s.resource` at `s.subresource`.
    unsafe { surface_get_desc(s.resource, s.subresource, true, desc) }
}

unsafe extern "system-unwind" fn sub_map(
    this: *mut c_void,
    locked_rect: *mut c_void,
    map_flags: u32,
) -> Hresult {
    // SAFETY: `this` is a live `SubresourceSurface`.
    let s = unsafe { &*this.cast::<SubresourceSurface>() };
    // SAFETY: maps `s.subresource` of `s.resource`.
    unsafe { surface_map(s.resource, s.subresource, locked_rect, map_flags) }
}

unsafe extern "system-unwind" fn sub_unmap(this: *mut c_void) -> Hresult {
    // SAFETY: `this` is a live `SubresourceSurface`.
    let s = unsafe { &*this.cast::<SubresourceSurface>() };
    // SAFETY: unmaps `s.subresource` of `s.resource`.
    unsafe { surface_unmap(s.resource, s.subresource) }
}

unsafe extern "system-unwind" fn sub_get_resource(
    this: *mut c_void,
    riid: *const c_void,
    parent_resource: *mut *mut c_void,
    subresource_index: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a live `SubresourceSurface`.
    let s = unsafe { &*this.cast::<SubresourceSurface>() };
    // SAFETY: forwards to `s.resource` at `s.subresource`.
    unsafe {
        surface_get_resource(
            s.resource,
            s.subresource,
            riid,
            parent_resource,
            subresource_index,
        )
    }
}

/// `IDXGISurface2` vtable for standalone [`SubresourceSurface`] instances.
pub static SUBRESOURCE_SURFACE_VTBL: IDXGISurface2Vtbl = IDXGISurface2Vtbl {
    query_interface: sub_query_interface,
    add_ref: sub_add_ref,
    release: sub_release,
    set_private_data: set_private_data::<SubresourceSurface>,
    set_private_data_interface: set_private_data_interface::<SubresourceSurface>,
    get_private_data: get_private_data::<SubresourceSurface>,
    get_parent: sub_get_device,
    get_device: sub_get_device,
    get_desc: sub_get_desc,
    map: sub_map,
    unmap: sub_unmap,
    get_dc: surface_get_dc,
    release_dc: surface_release_dc,
    get_resource: sub_get_resource,
};
