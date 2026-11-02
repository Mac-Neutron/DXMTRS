//! `ID3D11ClassLinkage` and `ID3D11ClassInstance` (`d3d11_class_linkage.cpp`).

use core::{
    ffi::{c_char, c_void},
    ptr,
};

use dxmt_util::{
    com::{
        ComObject, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, Guid, Hresult, IID_IUNKNOWN,
        S_OK, add_ref, qi_log, release, write_out,
    },
    private_data::PrivateData,
};

use crate::{
    LOG,
    abi::{IID_ID3D11_CLASS_LINKAGE, IID_ID3D11_DEVICE_CHILD, vtbl::ID3D11ClassLinkageVtbl},
};

/// `ID3D11ClassLinkage` implementation matching `MTLD3D11ClassLinkage`.
#[repr(C)]
pub struct ClassLinkage {
    /// Primary `ID3D11ClassLinkage` vtable pointer.
    pub vtbl: *const ID3D11ClassLinkageVtbl,
    /// Reference count (`MTLD3D11DeviceChild` starts at 1 on creation).
    pub refs: ComObject,
    /// Per-object `PrivateData` storage.
    pub private_data: PrivateData,
    /// Retained parent `ID3D11Device*` interface pointer.
    pub device: *mut c_void,
}

impl Drop for ClassLinkage {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained with `add_ref` on creation.
            unsafe {
                release(self.device);
            }
        }
    }
}

/// Allocates a new `ID3D11ClassLinkage` bound to `device` and writes it to `out`.
///
/// # Safety
///
/// `device` must be null or a valid `ID3D11Device*` COM pointer; `out` must be null or writable.
#[must_use]
pub unsafe fn create_class_linkage(device: *mut c_void, out: *mut *mut c_void) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    if !device.is_null() {
        // SAFETY: `device` is a live `ID3D11Device*` COM pointer.
        unsafe {
            add_ref(device);
        }
    }
    let linkage = Box::new(ClassLinkage {
        vtbl: &raw const VTBL,
        refs: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device,
    });
    let raw = Box::into_raw(linkage).cast::<c_void>();
    // SAFETY: `out` was checked non-null above.
    unsafe {
        write_out(out, raw);
    }
    S_OK
}

static VTBL: ID3D11ClassLinkageVtbl = ID3D11ClassLinkageVtbl {
    query_interface,
    add_ref: linkage_add_ref,
    release: linkage_release,
    get_device,
    get_private_data,
    set_private_data,
    set_private_data_interface,
    get_class_instance,
    create_class_instance,
};

unsafe extern "system-unwind" fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` is non-null.
    unsafe {
        write_out(out, ptr::null_mut());
    }
    // SAFETY: `riid` is a `GUID` pointer passed by the COM caller.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    if iid == IID_IUNKNOWN || iid == IID_ID3D11_DEVICE_CHILD || iid == IID_ID3D11_CLASS_LINKAGE {
        // SAFETY: `this` is a valid `ClassLinkage` pointer.
        let obj = unsafe { &*this.cast::<ClassLinkage>() };
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, this);
        }
        return S_OK;
    }
    if qi_log(&IID_ID3D11_CLASS_LINKAGE, &iid) {
        LOG.warn(&format!(
            "D3D11ClassLinkage::QueryInterface: Unknown interface query {iid}"
        ));
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn linkage_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `ClassLinkage` pointer.
    let obj = unsafe { &*this.cast::<ClassLinkage>() };
    obj.refs.add_ref()
}

unsafe extern "system-unwind" fn linkage_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `ClassLinkage` pointer.
    let (rc, free) = unsafe { (*this.cast::<ClassLinkage>()).refs.release_and_free() };
    if free {
        // SAFETY: `this` was allocated by `Box::into_raw` in `create_class_linkage` and refcount hit 0.
        drop(unsafe { Box::from_raw(this.cast::<ClassLinkage>()) });
    }
    rc
}

unsafe extern "system-unwind" fn get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `ClassLinkage` pointer.
    let obj = unsafe { &*this.cast::<ClassLinkage>() };
    if !obj.device.is_null() {
        // SAFETY: `obj.device` is a valid `ID3D11Device*` COM pointer.
        unsafe {
            add_ref(obj.device);
        }
    }
    // SAFETY: `out` is non-null.
    unsafe {
        write_out(out, obj.device);
    }
}

unsafe extern "system-unwind" fn get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `ClassLinkage` pointer.
    let obj = unsafe { &*this.cast::<ClassLinkage>() };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::get_data`.
    unsafe { obj.private_data.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `ClassLinkage` pointer.
    let obj = unsafe { &*this.cast::<ClassLinkage>() };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::set_data`.
    unsafe { obj.private_data.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    unknown: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `ClassLinkage` pointer.
    let obj = unsafe { &*this.cast::<ClassLinkage>() };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::set_interface`.
    unsafe { obj.private_data.set_interface(g, unknown.cast_mut()) }
}

unsafe extern "system-unwind" fn get_class_instance(
    _this: *mut c_void,
    _name: *const c_char,
    _idx: u32,
    _out: *mut *mut c_void,
) -> Hresult {
    LOG.err("GetClassInstance: Stub");
    E_NOTIMPL
}

unsafe extern "system-unwind" fn create_class_instance(
    _this: *mut c_void,
    _type_name: *const c_char,
    _cb_offset: u32,
    _cv_offset: u32,
    _tex_offset: u32,
    _samp_offset: u32,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    LOG.err("CreateClassInstance: Stub");
    E_NOTIMPL
}
