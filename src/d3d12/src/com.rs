//! COM base types, `DeviceChild` ownership, companion vtables, `slot!`, and D3D12 IIDs.

use core::{ffi::c_void, ops::Deref, ptr, ptr::NonNull};

pub use dxmt_util::{
    com::{
        ComObject, ComObjectWithInitialRef, Guid, Hresult, Object, add_ref, hresult::*, iid::*,
        qi_log, query, release, vtbl, write_out,
    },
    private_data::{PrivateData, WKPDID_D3D_DEBUG_OBJECT_NAME_W},
};

use crate::abi::vtbl::IUnknownVtbl;

/// Zeroes `*out` when `out` is non-null (`InitReturnPtr` in `com_pointer.hpp`).
///
/// # Safety
///
/// `out` must be null or point to a writable `*mut c_void`.
pub const unsafe fn init_out_ptr(out: *mut *mut c_void) {
    if !out.is_null() {
        // SAFETY: `out` is non-null and the caller guarantees it points to a writable slot.
        unsafe { out.write_unaligned(ptr::null_mut()) };
    }
}

/// Returns `true` when `ptr` is non-null and its first pointer-sized field equals `expected_vtbl`.
///
/// # Safety
///
/// `ptr` must be null or point to a readable vtable pointer.
#[must_use]
pub unsafe fn has_vtbl<V>(ptr: *const c_void, expected_vtbl: &'static V) -> bool {
    if ptr.is_null() {
        return false;
    }
    // SAFETY: `ptr` is non-null and the caller guarantees its vtable slot is readable.
    let vtbl_ptr = unsafe { ptr.cast::<*const V>().read_unaligned() };
    ptr::eq(vtbl_ptr, expected_vtbl)
}

/// Casts a non-null COM object pointer `ptr` to `&'a T` when its vtable matches `expected_vtbl`.
///
/// # Safety
///
/// `ptr` must be null or a valid COM object pointer, and any live object whose vtable pointer is
/// `expected_vtbl` must be a valid `T` outliving `'a`.
#[must_use]
pub unsafe fn cast_com<'a, T, V>(ptr: *const c_void, expected_vtbl: &'static V) -> Option<&'a T> {
    // SAFETY: forwarded from caller.
    if unsafe { has_vtbl(ptr, expected_vtbl) } {
        // SAFETY: only `T` instances carry `expected_vtbl` at offset 0.
        Some(unsafe { &*ptr.cast::<T>() })
    } else {
        None
    }
}

/// Adjusts a primary COM object pointer `this` forward by `ext_offset` bytes.
///
/// # Safety
///
/// `this` must be a non-null pointer to `T`, and `ext_offset` must be `offset_of!(T, <ext_vtbl>)`.
#[must_use]
pub const unsafe fn to_companion<T>(this: *mut T, ext_offset: usize) -> *mut c_void {
    // SAFETY: `ext_offset` is within the same allocation `T`.
    unsafe { this.cast::<c_void>().byte_add(ext_offset) }
}

/// Adjusts a companion interface pointer `ext_this` backward by `ext_offset` bytes.
///
/// # Safety
///
/// `ext_this` must point to the companion vtable field at byte offset `ext_offset` inside a live
/// `T`.
#[must_use]
pub const unsafe fn from_companion<T>(ext_this: *mut c_void, ext_offset: usize) -> *mut T {
    // SAFETY: `ext_this` was produced by adding `ext_offset` to a `*mut T` within the same
    // allocation.
    unsafe { ext_this.byte_sub(ext_offset).cast::<T>() }
}

/// A heap-allocated COM implementation struct backed by [`ComObject`].
pub trait ComRefCounted: Sized {
    /// Returns the object's two-count reference counter.
    fn com(&self) -> &ComObject;
}

/// Increments the public reference count of `this` (`AddRef`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>`.
pub unsafe fn add_ref_com<T: ComRefCounted>(this: *mut T) -> u32 {
    // SAFETY: `this` is a live `T`.
    let obj = unsafe { &*this };
    obj.com().add_ref()
}

/// Decrements the public reference count of `this` (`Release`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>` whose caller owns one public reference.
pub unsafe fn release_com<T: ComRefCounted>(this: *mut T) -> u32 {
    // SAFETY: `this` is a live `T`.
    let (remaining, free) = unsafe { (*this).com().release_and_free() };
    if free {
        // SAFETY: the last private reference was just released and `DESTROYING_BIT` is set.
        unsafe { drop(Box::from_raw(this)) };
    }
    remaining
}

/// Increments the public reference count of `this` through a companion pointer at `OFFSET`.
///
/// # Safety
///
/// `this` must point to the companion vtable field at byte offset `OFFSET` inside a live `Box<T>`.
pub unsafe fn companion_add_ref<T: ComRefCounted, const OFFSET: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at byte offset `OFFSET` inside a live `T`.
    let base = unsafe { from_companion::<T>(this, OFFSET) };
    // SAFETY: `base` points to a live `Box<T>`.
    unsafe { add_ref_com(base) }
}

/// Decrements the public reference count of `this` through a companion pointer at `OFFSET`.
///
/// # Safety
///
/// `this` must point to the companion vtable field at byte offset `OFFSET` inside a live `Box<T>`
/// whose caller owns one public reference.
pub unsafe fn companion_release<T: ComRefCounted, const OFFSET: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at byte offset `OFFSET` inside a live `T`.
    let base = unsafe { from_companion::<T>(this, OFFSET) };
    // SAFETY: `base` points to a live `Box<T>`.
    unsafe { release_com(base) }
}

/// Increments the private reference count of `this` (`AddRefPrivate`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>`.
pub unsafe fn add_ref_private_com<T: ComRefCounted>(this: *mut T) {
    // SAFETY: `this` is a live `T`.
    let obj = unsafe { &*this };
    obj.com().add_ref_private();
}

/// Decrements the private reference count of `this` (`ReleasePrivate`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>` whose caller owns one private reference.
pub unsafe fn release_private_com<T: ComRefCounted>(this: *mut T) {
    // SAFETY: `this` is a live `T`.
    let free = unsafe { (*this).com().release_private() };
    if free {
        // SAFETY: the last private reference was just released and `DESTROYING_BIT` is set.
        unsafe { drop(Box::from_raw(this)) };
    }
}

/// Smart pointer holding a public reference to a [`ComRefCounted`] COM object (`Com<T>`).
pub struct ComHold<T: ComRefCounted> {
    ptr: NonNull<T>,
}

// SAFETY: `T` is a COM object with atomic reference counts; sharing or sending `ComHold<T>`
// requires `T: Send + Sync`.
unsafe impl<T: ComRefCounted + Send + Sync> Send for ComHold<T> {}
// SAFETY: see `Send` above.
unsafe impl<T: ComRefCounted + Send + Sync> Sync for ComHold<T> {}

impl<T: ComRefCounted> ComHold<T> {
    /// Acquires a new public reference to `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a live `Box<T>`.
    #[must_use]
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        // SAFETY: `ptr` points to a live `Box<T>`.
        unsafe { add_ref_com(ptr.as_ptr()) };
        Self { ptr }
    }

    /// Raw pointer to the held `T`.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: ComRefCounted> Clone for ComHold<T> {
    fn clone(&self) -> Self {
        // SAFETY: `self.ptr` is kept alive by `self`.
        unsafe { Self::new(self.ptr) }
    }
}

impl<T: ComRefCounted> Deref for ComHold<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `self` holds a live public reference to `self.ptr`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ComRefCounted> Drop for ComHold<T> {
    fn drop(&mut self) {
        // SAFETY: `self` owns one public reference acquired in `new`.
        unsafe { release_com(self.ptr.as_ptr()) };
    }
}

/// Smart pointer holding a private reference to a [`ComRefCounted`] COM object (`Com<T, false>`).
///
/// Used by placed resources (`heap_`) and command lists (`allocator_`, `pso_*`, `rootsig_*`) so an
/// internal hold keeps the target allocation alive after its last public `Release`.
pub struct PrivateHold<T: ComRefCounted> {
    ptr: NonNull<T>,
}

// SAFETY: `T` is a COM object with atomic reference counts; sharing or sending `PrivateHold<T>`
// requires `T: Send + Sync`.
unsafe impl<T: ComRefCounted + Send + Sync> Send for PrivateHold<T> {}
// SAFETY: see `Send` above.
unsafe impl<T: ComRefCounted + Send + Sync> Sync for PrivateHold<T> {}

impl<T: ComRefCounted> PrivateHold<T> {
    /// Acquires a new private reference to `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a live `Box<T>`.
    #[must_use]
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        // SAFETY: `ptr` points to a live `Box<T>`.
        unsafe { add_ref_private_com(ptr.as_ptr()) };
        Self { ptr }
    }

    /// Raw pointer to the held `T`.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: ComRefCounted> Clone for PrivateHold<T> {
    fn clone(&self) -> Self {
        // SAFETY: `self.ptr` is kept alive by `self`.
        unsafe { Self::new(self.ptr) }
    }
}

impl<T: ComRefCounted> Deref for PrivateHold<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `self` holds a live private reference to `self.ptr`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ComRefCounted> Drop for PrivateHold<T> {
    fn drop(&mut self) {
        // SAFETY: `self` owns one private reference acquired in `new`.
        unsafe { release_private_com(self.ptr.as_ptr()) };
    }
}

/// Stores `name` under [`WKPDID_D3D_DEBUG_OBJECT_NAME_W`] in `private_data`.
///
/// # Safety
///
/// `name` must be null or point to a NUL-terminated `WCHAR` sequence.
pub unsafe fn set_name_private_data(private_data: &PrivateData, name: *const u16) -> Hresult {
    #[cfg(windows)]
    type Wchar = u16;
    #[cfg(not(windows))]
    type Wchar = u32;

    if name.is_null() {
        // SAFETY: null `data` erases the entry.
        return unsafe { private_data.set_data(WKPDID_D3D_DEBUG_OBJECT_NAME_W, 0, ptr::null()) };
    }
    let wide = name.cast::<Wchar>();
    let mut len = 0usize;
    loop {
        // SAFETY: `wide` is non-null and `len` has not yet reached the terminating NUL.
        let ch_ptr = unsafe { wide.add(len) };
        // SAFETY: `ch_ptr` points within the caller-supplied NUL-terminated wide string.
        if unsafe { ch_ptr.read_unaligned() } == 0 {
            break;
        }
        len += 1;
    }
    let byte_len = u32::try_from((len + 1) * size_of::<Wchar>()).unwrap_or(u32::MAX);
    // SAFETY: `name` points to `byte_len` readable bytes.
    unsafe { private_data.set_data(WKPDID_D3D_DEBUG_OBJECT_NAME_W, byte_len, name.cast()) }
}

/// Common header for `MTLD3D12DeviceChild` classes (`d3d12_device_child.hpp`).
#[repr(C)]
pub struct DeviceChild<D: ComRefCounted> {
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: ComHold<D>,
}

impl<D: ComRefCounted> DeviceChild<D> {
    /// Creates a `DeviceChild` header holding a reference to `device` (`device_->AddRef()`).
    ///
    /// # Safety
    ///
    /// `device` must point to a live `Box<D>`.
    #[must_use]
    pub unsafe fn new(device: NonNull<D>) -> Self {
        // SAFETY: `device` is a live `Box<D>`.
        let device = unsafe { ComHold::new(device) };
        Self {
            com: ComObject::new(),
            private_data: PrivateData::default(),
            device,
        }
    }

    /// `ID3D12Object::GetPrivateData`.
    ///
    /// # Safety
    ///
    /// `guid` and `data_size` must be valid pointers or null; `data` must point to `*data_size`
    /// writable bytes when non-null.
    pub unsafe fn get_private_data(
        &self,
        guid: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: forwarded from COM slot with caller's pointers.
        unsafe { self.private_data.get_data(guid, data_size, data) }
    }

    /// `ID3D12Object::SetPrivateData`.
    ///
    /// # Safety
    ///
    /// `guid` must be valid or null; `data` must point to `data_size` readable bytes when
    /// non-null.
    pub unsafe fn set_private_data(
        &self,
        guid: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: forwarded from COM slot with caller's pointers.
        unsafe { self.private_data.set_data(guid, data_size, data) }
    }

    /// `ID3D12Object::SetPrivateDataInterface`.
    ///
    /// # Safety
    ///
    /// `guid` must be valid or null; `unknown` must be null or a valid `IUnknown` pointer.
    pub unsafe fn set_private_data_interface(
        &self,
        guid: *const c_void,
        unknown: *const c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: forwarded from COM slot with caller's pointers.
        unsafe { self.private_data.set_interface(guid, unknown.cast_mut()) }
    }

    /// `ID3D12Object::SetName` (`d3d12_device_child.hpp:50-56`).
    ///
    /// # Safety
    ///
    /// `name` must be null or point to a NUL-terminated UTF-16 string.
    pub unsafe fn set_name(&self, name: *const u16) -> Hresult {
        // SAFETY: forwarded from COM slot with caller's pointer.
        unsafe { set_name_private_data(&self.private_data, name) }
    }

    /// `ID3D12DeviceChild::GetDevice`: forwards to the parent device's `QueryInterface`.
    ///
    /// # Safety
    ///
    /// `D`'s first field must be a vtable pointer starting with [`IUnknownVtbl`], and `riid` /
    /// `out` must be valid `QueryInterface` arguments.
    pub unsafe fn get_device(&self, riid: *const c_void, out: *mut *mut c_void) -> Hresult {
        let dev_ptr = self.device.as_ptr().cast::<c_void>();
        // SAFETY: `D` is a COM object whose first field is a vtable starting with `IUnknownVtbl`.
        let unk = unsafe { vtbl::<IUnknownVtbl>(dev_ptr) };
        // SAFETY: `dev_ptr` is live (`self.device` holds a reference) and `riid`/`out` are
        // forwarded from the caller.
        unsafe { (unk.query_interface)(dev_ptr, riid, out) }
    }
}

/// Stack reservation for D3D12 worker threads (`dxmt::thread` in `util/thread.hpp:55`, P16).
pub const THREAD_STACK_SIZE: usize = 0x10_0000;

/// Spawns a worker thread named `name` with [`THREAD_STACK_SIZE`] and `AbortOnPanic` (P16, P20).
///
/// # Panics
///
/// Panics if the operating system fails to spawn the thread.
pub fn spawn_thread<F>(name: &str, f: F) -> std::thread::JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    let truncated = &name[..name.floor_char_boundary(15)];
    std::thread::Builder::new()
        .name(truncated.to_owned())
        .stack_size(THREAD_STACK_SIZE)
        .spawn(move || {
            let _guard = dxmt_core::ffi::AbortOnPanic;
            f();
        })
        .expect("failed to spawn D3D12 worker thread")
}

/// Defines an `unsafe extern "system-unwind" fn` COM vtable slot thunk (P9, P20).
#[macro_export]
macro_rules! slot {
    ($scope:literal, ($($arg:ident : $arg_ty:ty),* $(,)?) -> $ret:ty => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) -> $ret {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $crate::scope!($scope);
            $body
        }
        thunk
    }};
    ($scope:literal, ($($arg:ident : $arg_ty:ty),* $(,)?) => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $crate::scope!($scope);
            $body;
        }
        thunk
    }};
    (($($arg:ident : $arg_ty:ty),* $(,)?) -> $ret:ty => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) -> $ret {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $body
        }
        thunk
    }};
    (($($arg:ident : $arg_ty:ty),* $(,)?) => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $body;
        }
        thunk
    }};
}

#[cfg(test)]
mod tests;
