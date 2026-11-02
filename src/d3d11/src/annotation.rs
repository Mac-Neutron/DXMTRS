//! `ID3DUserDefinedAnnotation` companion on [`crate::context::Context`] (`d3d11_annotation.hpp`).

use core::ffi::c_void;

use crate::{
    abi::vtbl::{ID3DUserDefinedAnnotationVtbl, IUnknownVtbl},
    com::TearOff,
};

/// `ID3DUserDefinedAnnotation` vtable embedded at field 2 (`annotation_vtbl`) of `Context<M>`.
///
/// `Context<M>` is `#[repr(C)]` with `vtbl` at offset 0, `ext_vtbl` at offset 1
/// pointer-width, and `annotation_vtbl` at offset 2 pointer-widths, so
/// `IUnknown` slots subtract two pointer-widths and forward to `Context<M>::vtbl`.
pub static VTBL: ID3DUserDefinedAnnotationVtbl = ID3DUserDefinedAnnotationVtbl {
    query_interface: context_query_interface,
    add_ref: context_add_ref,
    release: context_release,
    begin_event,
    end_event,
    set_marker,
    get_status,
};

/// `ID3DUserDefinedAnnotation` vtable for use inside a [`TearOff<ID3DUserDefinedAnnotationVtbl>`].
pub static TEAR_OFF_VTBL: ID3DUserDefinedAnnotationVtbl = ID3DUserDefinedAnnotationVtbl {
    query_interface: TearOff::<()>::query_interface,
    add_ref: TearOff::<()>::add_ref,
    release: TearOff::<()>::release,
    begin_event,
    end_event,
    set_marker,
    get_status,
};

/// Recovers the enclosing `Context<M>` primary interface pointer from `&ctx.annotation_vtbl`.
const fn context_base(this: *mut c_void) -> *mut c_void {
    this.wrapping_byte_sub(2 * core::mem::size_of::<usize>())
}

unsafe extern "system-unwind" fn context_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    let base = context_base(this);
    // SAFETY: `base` points to the primary `vtbl` slot of `Context<M>`.
    let vtbl = unsafe { *base.cast::<*const IUnknownVtbl>() };
    // SAFETY: `vtbl` is a static `IUnknownVtbl`-compatible vtable.
    let qi = unsafe { (*vtbl).query_interface };
    // SAFETY: Forwarding `QueryInterface` to the enclosing `Context<M>`.
    unsafe { qi(base, riid, out) }
}

unsafe extern "system-unwind" fn context_add_ref(this: *mut c_void) -> u32 {
    let base = context_base(this);
    // SAFETY: `base` points to the primary `vtbl` slot of `Context<M>`.
    let vtbl = unsafe { *base.cast::<*const IUnknownVtbl>() };
    // SAFETY: `vtbl` is a static `IUnknownVtbl`-compatible vtable.
    let add_ref = unsafe { (*vtbl).add_ref };
    // SAFETY: Forwarding `AddRef` to the enclosing `Context<M>`.
    unsafe { add_ref(base) }
}

unsafe extern "system-unwind" fn context_release(this: *mut c_void) -> u32 {
    let base = context_base(this);
    // SAFETY: `base` points to the primary `vtbl` slot of `Context<M>`.
    let vtbl = unsafe { *base.cast::<*const IUnknownVtbl>() };
    // SAFETY: `vtbl` is a static `IUnknownVtbl`-compatible vtable.
    let release = unsafe { (*vtbl).release };
    // SAFETY: Forwarding `Release` to the enclosing `Context<M>`.
    unsafe { release(base) }
}

const unsafe extern "system-unwind" fn begin_event(
    _this: *mut c_void,
    _name: *const c_void,
) -> i32 {
    -1
}

const unsafe extern "system-unwind" fn end_event(_this: *mut c_void) -> i32 {
    -1
}

const unsafe extern "system-unwind" fn set_marker(_this: *mut c_void, _name: *const c_void) {}

const unsafe extern "system-unwind" fn get_status(_this: *mut c_void) -> i32 {
    0
}
